use std::{
    collections::HashMap,
    io::{StdoutLock, Write},
    sync::{atomic::AtomicBool, mpsc::Receiver},
    time::Duration,
};

use crossterm::{
    QueueableCommand, cursor, queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{UserActionEvent, history::GridEventHistory},
    maze::{cell::GridCell, grid::GridEvent},
};

/// Struct to manage render refresh time scaling based on a quantized level scale
struct RenderRefreshTimeScale {
    delta: Duration,
    /// number of discrete levels (quantization). e.g. 10
    levels: usize,
    /// current level in [0, levels - 1], 0 is slowest, levels-1 is fastest
    level: usize,
}

impl Default for RenderRefreshTimeScale {
    fn default() -> Self {
        // Base duration multiplier for topmost level
        let base = Duration::from_micros(10);
        Self {
            delta: base,
            levels: 20, // default quantization
            level: 10,  // default mid-speed
        }
    }
}

impl RenderRefreshTimeScale {
    /// Create a calibrated RenderRefreshTimeScale based on the grid dimensions
    fn calibrated(grid_width: u8, grid_height: u8) -> Self {
        let mut scale = Self::default();
        // Map grid size to a sensible starting level.
        // Larger grids -> faster rendering (higher level index).
        let size = grid_width.max(grid_height) as f32;
        let max = u8::MAX as f32;
        let frac = (size / max).clamp(0.0, 1.0);
        // large size => faster, but clamp properly
        let lvl = (frac * frac * (scale.levels.saturating_sub(1) as f32)).round() as usize;
        scale.level = lvl.min(scale.levels.saturating_sub(1));
        scale
    }

    /// Create a scale bar string representing the current render refresh time scale
    /// The scale is quantized into `self.levels` segments. If terminal width is smaller
    /// than the requested number of segments, segments will be capped to width.
    fn make_scale_bar(&self, width: u16) -> String {
        let w = width as usize;
        if w == 0 {
            return "".to_string();
        }

        // desired number of segments = self.levels, but cap to available character width
        let seg_count = self.levels.min(w).max(1);
        // integer width per segment and remainder distribution
        let base_seg_w = w / seg_count;
        let rem = w % seg_count;

        // Build segment sizes: first `rem` segments get +1 char
        let seg_sizes = (0..seg_count).map(|i| base_seg_w + if i < rem { 1 } else { 0 });

        // Create the bar: filled segments up to `self.level`, rest empty
        seg_sizes
            .enumerate()
            .map(|(i, seg_w)| {
                let ch = if i <= self.level { '█' } else { '░' };
                ch.to_string().repeat(seg_w)
            })
            .collect::<String>()
    }

    /// Get the current duration based on the square of the level
    /// Level levels-1 -> delta * 1^2 (fastest), level 0 -> delta * levels^2 (slowest)
    fn current(&self) -> Duration {
        let factor = ((self.levels - self.level) as u32).saturating_add(1);
        self.delta * factor * factor
    }

    /// Speed up the rendering by increasing the current level (toward levels-1).
    fn speed_up(&mut self) {
        if self.level < self.levels.saturating_sub(1) {
            self.level += 1;
        } else {
            self.level = self.levels.saturating_sub(1);
        }
    }

    /// Slow down the rendering by decreasing the current level (toward 0).
    fn slow_down(&mut self) {
        if self.level > 0 {
            self.level -= 1;
        } else {
            self.level = 0;
        }
    }
}

/// Compact representation of the current grid state.
/// Stores the initial fill cell and only the cells that differ from it.
#[derive(Default)]
struct GridState {
    /// initial cell and grid dimensions (width, height)
    initial: Option<(GridCell, u16, u16)>,
    /// map of (x,y) -> cell for cells that differ from initial cell
    changes: HashMap<(u16, u16), GridCell>,
}

impl GridState {
    fn dims(&self) -> Option<(u16, u16)> {
        self.initial.map(|(_, w, h)| (w, h))
    }

    /// Apply a GridEvent to the compact state. Safe to call for both [`GridEvent::Initial`]` and [`GridEvent::Update`].
    /// If an [`GridEvent::Initial`] is received, it clears the changes map and sets the initial cell and dimensions.
    /// If an [`GridEvent::Update`] is received, it updates the changes map accordingly. The `old` value is ignored.
    fn add_event(&mut self, event: GridEvent) {
        match event {
            GridEvent::Initial {
                cell,
                width,
                height,
            } => {
                self.initial = Some((cell, width, height));
                self.changes.clear();
            }
            GridEvent::Update { coord, new, .. } => {
                if let Some((initial, _, _)) = self.initial {
                    if new == initial {
                        self.changes.remove(&coord);
                    } else {
                        self.changes.insert(coord, new);
                    }
                }
                // If no initial, ignore update
            }
        }
    }

    /// Render the compact state to the provided stdout. This draws the initial filled grid,
    /// then overlays changed cells. Does nothing if initial not set.
    fn recover(&self, stdout: &mut StdoutLock) -> std::io::Result<()> {
        if let Some((initial, width, height)) = self.initial {
            // Render initial filled grid
            stdout.queue(cursor::MoveTo(0, 0))?;
            for _y in 0..height {
                for _x in 0..width {
                    stdout.queue(style::Print(initial))?;
                }
                stdout.queue(style::Print("\r\n"))?;
            }
            // Overlay changed cells
            for (&(x, y), &cell) in self.changes.iter() {
                queue!(
                    stdout,
                    cursor::MoveTo(x * GridCell::CELL_WIDTH, y),
                    style::Print(cell)
                )?;
            }
            stdout.flush()?;
        }
        Ok(())
    }
}

pub enum RendererStatus {
    /// Rendering completed successfully
    Completed,
    /// Rendering was cancelled
    Cancelled,
}

pub struct Renderer<'a> {
    /// Standard output handle to write to the terminal.
    /// Locked for exclusive access during rendering.
    /// This lock is held for the lifetime of the Renderer instance.
    stdout: StdoutLock<'a>,
    /// History of grid events for browsing & recovery
    history: GridEventHistory,
    /// Compact current grid state (initial cell + diffs)
    grid_state: GridState,
    /// Render refresh time scale
    render_refresh_time_scale: RenderRefreshTimeScale,
}

impl<'a> Renderer<'a> {
    /// Number of log rows reserved at the bottom of the terminal
    pub const NUM_LOG_ROWS: u16 = 1;

    /// Create a new Renderer instance
    /// `max_history_grid_events` specifies the maximum number of grid events to keep in history
    /// `maze_dims` is an optional tuple of (width, height) to calibrate the render refresh time scale
    pub fn new(max_history_grid_events: usize, maze_dims: Option<(u8, u8)>) -> Self {
        Self {
            stdout: std::io::stdout().lock(),
            history: GridEventHistory::new(max_history_grid_events),
            grid_state: GridState::default(),
            render_refresh_time_scale: match maze_dims {
                Some((width, height)) => RenderRefreshTimeScale::calibrated(width, height),
                None => RenderRefreshTimeScale::default(),
            },
        }
    }

    /// Recover the grid state by replaying the history of grid events from the most recent initial event
    /// to the current state.
    /// This is when the renderer needs to revert past an initial event.
    /// Returns:
    /// - `Err` if there was an I/O error
    /// - `Ok(true)` if recovery was performed
    /// - `Ok(false)` if recovery was skipped due to missing initial event
    fn recover_from_history(&mut self) -> std::io::Result<bool> {
        // Walk backward to the most recent initial event or beginning of history
        let mut counter = 0; // Count number of events to replay
        let (width, height, cell) = loop {
            if let Some(event) = self.history.history_backward() {
                counter += 1;
                if let GridEvent::Initial {
                    width,
                    height,
                    cell,
                } = event
                {
                    // Reached initial event, set grid state and break
                    self.grid_state.initial = Some((cell, width, height));
                    break (width, height, cell);
                }
            } else {
                // No more events — exited normally
                tracing::warn!(
                    "No initial event found in history during recovery, skipping grid state recovery"
                );
                // Recover history position
                for _ in 0..counter {
                    self.history.history_forward();
                }
                return Ok(false);
            }
        };

        // Render the initial filled grid
        self.stdout.queue(cursor::MoveTo(0, 0))?;
        for _y in 0..height {
            for _x in 0..width {
                self.stdout.queue(style::Print(cell))?;
            }
            self.stdout.queue(style::Print("\r\n"))?;
        }
        self.grid_state.add_event(GridEvent::Initial {
            cell,
            width,
            height,
        });
        // Walk forward through the history to re-apply all update events
        for _ in 0..counter {
            let event = self.history.history_forward();
            if let Some(GridEvent::Update { coord, old, new }) = event {
                // Move the cursor to the specified coordinate and print the new cell
                queue!(
                    self.stdout,
                    cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1),
                    style::Print(new)
                )?;
                self.grid_state
                    .add_event(GridEvent::Update { coord, old, new });
            }
        }
        self.stdout.flush()?;
        Ok(true)
    }

    /// Check if terminal size is sufficient for the current grid dimensions (if set)
    /// If not, display a message and wait for user to press Esc or resize the terminal.
    /// Returns:
    /// - `Ok(RendererStatus::Completed)` if terminal size is sufficient
    /// - `Ok(RendererStatus::Cancelled)` if rendering was cancelled by user
    /// - `Err` if there was an I/O error
    fn check_resize(
        &mut self,
        user_action_event_rx: &Receiver<UserActionEvent>,
    ) -> std::io::Result<RendererStatus> {
        match self.grid_state.dims() {
            Some((width, height)) => {
                let (term_width, term_height) = terminal::size()?;
                if term_width < width * GridCell::CELL_WIDTH
                    || term_height.saturating_sub(Self::NUM_LOG_ROWS) < height
                {
                    tracing::info!("Terminal size too small for grid display, pausing rendering");
                    let msg = format!(
                        "Terminal size is too small ({}x{}) for the grid dimensions ({}x{}) to display.\r\n",
                        width * GridCell::CELL_WIDTH,
                        height,
                        width,
                        height
                    );
                    queue!(
                        self.stdout,
                        terminal::Clear(ClearType::All),
                        cursor::MoveTo(0, 0),
                        style::PrintStyledContent(
                            msg.with(Color::Yellow).attribute(Attribute::Bold)
                        ),
                        style::PrintStyledContent(
                            "Please resize the terminal, or press Esc to exit...\r\n"
                                .with(Color::Blue)
                                .attribute(Attribute::Bold)
                        )
                    )?;
                    self.stdout.flush()?;

                    // Listen for user action events to resume or cancel
                    loop {
                        match user_action_event_rx.recv() {
                            Err(_e) => {
                                // Main thread has disconnected, treat as cancelled
                                tracing::info!("Rendering cancelled due to terminal resize");
                                return Ok(RendererStatus::Cancelled);
                            }
                            Ok(event) => match event {
                                UserActionEvent::Cancel => {
                                    tracing::info!(
                                        "Rendering cancelled by user due to terminal resize"
                                    );
                                    return Ok(RendererStatus::Cancelled);
                                }
                                UserActionEvent::Resize => {
                                    // Check terminal size again
                                    let (new_term_width, new_term_height) = terminal::size()?;
                                    if new_term_width >= width * GridCell::CELL_WIDTH
                                        && new_term_height.saturating_sub(Self::NUM_LOG_ROWS)
                                            >= height
                                    {
                                        // Terminal resized sufficiently, recover display
                                        tracing::info!(
                                            "Terminal resized sufficiently, recovering grid display"
                                        );
                                        self.grid_state.recover(&mut self.stdout)?;
                                        break;
                                    } else {
                                        // Still too small, continue waiting
                                        continue;
                                    }
                                }
                                _ => {
                                    // Ignore other events
                                    continue;
                                }
                            },
                        }
                    }
                }
                Ok(RendererStatus::Completed)
            }
            None => Ok(RendererStatus::Completed), // No grid dimensions set, skip check
        }
    }

    /// Render a single grid event to the terminal
    /// - If event is [`GridEvent::Initial`], it will update the grid dimensions and render the initial grid.
    /// - If event is [`GridEvent::Update`], it will update the specified cell in the grid only if grid dimensions are set.
    ///
    /// If `save_to_history` is true, the event will be added to the history after successful rendering.
    /// Returns:
    /// - `Ok(RendererStatus::Completed)` if rendering completed successfully
    /// - `Ok(RendererStatus::Completed)` if rendering was cancelled. Event will not be rendered or added to history.
    /// - `Err` if there was an I/O error
    fn render_grid_event(
        &mut self,
        event: GridEvent,
        user_action_event_rx: &Receiver<UserActionEvent>,
        save_to_history: bool,
    ) -> std::io::Result<RendererStatus> {
        // Render the event
        match event {
            GridEvent::Initial {
                cell,
                width,
                height,
            } => {
                if let RendererStatus::Cancelled = self.check_resize(user_action_event_rx)? {
                    return Ok(RendererStatus::Cancelled);
                }

                self.stdout.queue(cursor::MoveTo(0, 0))?;
                for _y in 0..height {
                    for _x in 0..width {
                        self.stdout.queue(style::Print(cell))?;
                    }
                    self.stdout.queue(style::Print("\r\n"))?;
                }
                self.stdout.flush()?;
            }
            GridEvent::Update { coord, new, .. } => {
                if self.grid_state.dims().is_some() {
                    if let RendererStatus::Cancelled = self.check_resize(user_action_event_rx)? {
                        return Ok(RendererStatus::Cancelled);
                    }
                    // Move the cursor to the specified coordinate and print the
                    // new cell using the grid dimensions
                    queue!(
                        self.stdout,
                        cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1),
                        style::Print(new)
                    )?;
                    self.stdout.flush()?;
                }
            }
        }

        // Always update compact grid state so recovery uses latest state
        self.grid_state.add_event(event);

        if save_to_history {
            // Add event to history
            self.history.add_event(event);
        }
        Ok(RendererStatus::Completed)
    }

    /// Get the current grid width in terminal columns
    fn get_width(&self) -> std::io::Result<u16> {
        // Get grid width / terminal width for logging purposes
        let width = match self.grid_state.dims() {
            Some((w, _)) => w * GridCell::CELL_WIDTH,
            None => terminal::size()?.0,
        };
        Ok(width)
    }

    /// Log a message to the terminal below the grid without disrupting the grid display
    /// The cursor position is saved and restored after logging
    /// Returns `Err` if there was an I/O error
    /// If msg is None, clears the log line
    fn log_to_terminal(
        &mut self,
        msg: Option<style::StyledContent<String>>,
    ) -> std::io::Result<()> {
        queue!(
            self.stdout,
            // Save cursor position first
            cursor::SavePosition,
            // Move cursor to the log line (below the grid)
            cursor::MoveTo(
                0,
                match self.grid_state.dims() {
                    Some((_, height)) => height, // Move cursor right below the grid
                    None => 0, // Default to top line if grid dimensions not yet set
                }
            ),
            // Clear previous log line
            terminal::Clear(ClearType::CurrentLine),
            // Print the log message
            style::PrintStyledContent(match msg {
                Some(msg) => {
                    let width = terminal::size()?.0 as usize;
                    let content = msg.content();
                    if content.width() > width {
                        // Truncate message to fit terminal width
                        let truncated = format!("{}...", &content[..width.saturating_sub(3)]);
                        style::StyledContent::new(*msg.style(), truncated)
                    } else {
                        msg
                    }
                }
                None => "".to_string().with(Color::Reset),
            }),
            // Go back to previous cursor position
            cursor::RestorePosition,
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    /// Handle a single user action event in the paused state
    /// Returns `Ok(RendererStatus::Completed)` if rendering was completed successfully
    /// Returns `Ok(RendererStatus::Cancelled)` if rendering was cancelled
    /// Returns `Err` if there was an I/O error
    fn handle_user_action_event(
        &mut self,
        event: &UserActionEvent,
        user_action_event_rx: &Receiver<UserActionEvent>,
        grid_event_rx: &Receiver<GridEvent>,
    ) -> std::io::Result<RendererStatus> {
        match event {
            UserActionEvent::Resume => {
                // Clear any log messages
                self.log_to_terminal(None)?;
                tracing::debug!("Resuming rendering from pause");
                // Step forward in the history and exit pause loop
                while let Some(event) = self.history.history_forward() {
                    if let RendererStatus::Cancelled =
                        self.render_grid_event(event, user_action_event_rx, false)?
                    {
                        return Ok(RendererStatus::Cancelled);
                    }
                    // Do we sleep here to simulate rendering time?
                    std::thread::sleep(self.render_refresh_time_scale.current());
                }
            }
            UserActionEvent::Pause => {
                // Already paused, ignore
            }
            UserActionEvent::Forward => {
                // Step forward and get the event
                let event = self.history.history_forward();
                if let Some(event) = event {
                    tracing::debug!("Rendering history forward event: {:?}", event);
                    if let RendererStatus::Cancelled =
                        self.render_grid_event(event, user_action_event_rx, false)?
                    {
                        return Ok(RendererStatus::Cancelled);
                    }
                    self.log_to_terminal(Some(event.to_string().with(Color::Green)))?;
                } else {
                    tracing::debug!("Attempting to step into the future");
                    match grid_event_rx.try_recv() {
                        Ok(event) => {
                            tracing::debug!("Rendering new future event: {:?}", event);
                            if let RendererStatus::Cancelled =
                                self.render_grid_event(event, user_action_event_rx, true)?
                            {
                                return Ok(RendererStatus::Cancelled);
                            }
                            self.log_to_terminal(Some(event.to_string().with(Color::Green)))?;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No action event, continue
                            tracing::debug!("No future event available at the moment");
                            self.log_to_terminal(Some(
                                "No future event available at the moment"
                                    .to_string()
                                    .with(Color::Yellow),
                            ))?;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // Channel disconnected, ignore and continue
                            // This loop will just wait for Resume to exit the pause state,
                            // And the main render loop will eventually exit when it detects the disconnection
                            tracing::debug!("Grid event channel disconnected");
                            self.log_to_terminal(Some(
                                "Grid event channel disconnected. Resume to exit the rendering"
                                    .to_string()
                                    .with(Color::Red),
                            ))?;
                        }
                    };
                }
            }
            UserActionEvent::Backward => {
                // Get the current event
                if let Some(event) = self.history.current_event() {
                    // Craft the revert event
                    match event {
                        GridEvent::Initial { .. } => {
                            if self.history.history_backward().is_none() {
                                // Cannot go back further
                                tracing::debug!("Already at the oldest event, cannot go backward");
                                self.log_to_terminal(Some(
                                    "Already at the oldest event, cannot go backward"
                                        .to_string()
                                        .with(Color::Yellow),
                                ))?;
                            } else {
                                // Recover the grid state up to right before this initial event
                                tracing::debug!(
                                    "Reverting to state before initial event, recovering grid state"
                                );
                                if !self.recover_from_history()? {
                                    // Recovery skipped due to missing initial event
                                    self.log_to_terminal(Some(
                                        "No initial event found in history during revert, cannot go backward"
                                            .to_string()
                                            .with(Color::Yellow),
                                    ))?;
                                    self.history.history_forward(); // Move back to current position
                                } else {
                                    self.log_to_terminal(Some(
                                        "Reverted to state before initial event"
                                            .to_string()
                                            .with(Color::Yellow),
                                    ))?;
                                }
                            }
                        }
                        GridEvent::Update { coord, old, new } => {
                            // Move backward in history
                            if self.history.history_backward().is_none() {
                                // Cannot go back further
                                self.log_to_terminal(Some(
                                    "Already at the oldest event after revert"
                                        .to_string()
                                        .with(Color::Yellow),
                                ))?;
                            } else {
                                // Only render the revert event when not at the oldest event
                                let revert_event = GridEvent::Update {
                                    coord,
                                    old: new,
                                    new: old,
                                };

                                tracing::debug!(
                                    "Rendering history backward event: {:?}",
                                    revert_event
                                );
                                if let RendererStatus::Cancelled = self.render_grid_event(
                                    revert_event,
                                    user_action_event_rx,
                                    false,
                                )? {
                                    return Ok(RendererStatus::Cancelled);
                                }
                                self.log_to_terminal(Some(
                                    revert_event.to_string().with(Color::Yellow),
                                ))?;
                            }
                        }
                    };
                }
            }
            UserActionEvent::SpeedUp => {
                // Increase rendering speed (decrease refresh time)
                self.render_refresh_time_scale.speed_up();
                tracing::debug!(
                    "Increased rendering speed, new refresh time: {:?}",
                    self.render_refresh_time_scale.current()
                );
                self.log_to_terminal(Some(
                    self.render_refresh_time_scale
                        .make_scale_bar(self.get_width()?)
                        .stylize(),
                ))?;
            }
            UserActionEvent::Resize => {
                // Check terminal size against current grid dimensions
                if let RendererStatus::Cancelled = self.check_resize(user_action_event_rx)? {
                    // Rendering was cancelled, exit
                    return Ok(RendererStatus::Cancelled);
                }
            }
            UserActionEvent::SlowDown => {
                // Decrease rendering speed (increase refresh time)
                self.render_refresh_time_scale.slow_down();
                tracing::debug!(
                    "Decreased rendering speed, new refresh time: {:?}",
                    self.render_refresh_time_scale.current()
                );
                self.log_to_terminal(Some(
                    self.render_refresh_time_scale
                        .make_scale_bar(self.get_width()?)
                        .stylize(),
                ))?;
            }
            UserActionEvent::Cancel => {
                // Clear any log messages
                self.log_to_terminal(None)?;
                tracing::info!("Rendering cancelled by user");
                // Exit rendering loop
                return Ok(RendererStatus::Cancelled);
            }
        };
        Ok(RendererStatus::Completed)
    }

    /// Handle user action events in a blocking manner until a Resume event is received
    /// This function returns when a Resume event is received
    fn listen_to_user_action_events(
        &mut self,
        user_action_event_rx: &Receiver<UserActionEvent>,
        grid_event_rx: &Receiver<GridEvent>,
    ) -> std::io::Result<RendererStatus> {
        // Pause rendering until Resume event is received
        loop {
            match user_action_event_rx.recv() {
                Err(_e) => {
                    // Main thread has disconnected, exit pause loop
                    break;
                }
                Ok(event) => {
                    if let RendererStatus::Cancelled =
                        self.handle_user_action_event(&event, user_action_event_rx, grid_event_rx)?
                    {
                        return Ok(RendererStatus::Cancelled);
                    } else if let UserActionEvent::Resume = event {
                        // Exit pause loop on Resume, otherwise continue listening
                        break;
                    }
                }
            }
        }
        Ok(RendererStatus::Completed)
    }

    /// Render loop that processes events from the user action and grid event channels
    /// Returns:
    /// - `Ok(RendererStatus::Completed)` if rendering completed successfully
    /// - `Ok(RendererStatus::Cancelled)` if rendering was cancelled
    /// - `Err` if there was an I/O error
    pub fn render(
        &mut self,
        grid_event_rx: Receiver<GridEvent>,
        user_action_event_rx: Receiver<UserActionEvent>,
        cancel: &AtomicBool,
        done: &AtomicBool,
    ) -> std::io::Result<RendererStatus> {
        queue!(self.stdout, terminal::Clear(ClearType::All), cursor::Hide)?;
        self.stdout.flush()?;

        loop {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                // Canceled by main thread, exit render loop
                tracing::info!("Rendering cancelled by main thread");
                return Ok(RendererStatus::Cancelled);
            }

            // Try to receive user action events without blocking
            match user_action_event_rx.try_recv() {
                Ok(action_event) => {
                    tracing::debug!("Received user action event: {:?}", action_event);
                    match action_event {
                        UserActionEvent::Pause => {
                            // Block and handle subsequent user action events
                            tracing::info!("Pausing rendering on user request");
                            if let RendererStatus::Cancelled = self.listen_to_user_action_events(
                                &user_action_event_rx,
                                &grid_event_rx,
                            )? {
                                tracing::info!("Rendering cancelled during pause");
                                return Ok(RendererStatus::Cancelled);
                            }
                        }
                        UserActionEvent::SpeedUp
                        | UserActionEvent::SlowDown
                        | UserActionEvent::Resize
                        | UserActionEvent::Cancel => {
                            // Handle these events immediately
                            tracing::info!(
                                "Handling user action event immediately: {:?}",
                                action_event
                            );
                            if let RendererStatus::Cancelled = self.handle_user_action_event(
                                &action_event,
                                &user_action_event_rx,
                                &grid_event_rx,
                            )? {
                                tracing::info!("Rendering cancelled by user action");
                                return Ok(RendererStatus::Cancelled);
                            }
                        }
                        _ => {}
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No action event, continue to check grid events
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Channel disconnected, ignore and check grid events
                }
            }
            // Block and wait for the next grid event
            match grid_event_rx.recv() {
                Err(_e) => {
                    // Compute thread has finished sending events, exit render loop
                    // Clear logs first
                    self.log_to_terminal(None)?;
                    break;
                }
                Ok(event) => {
                    // Render the grid event
                    if let RendererStatus::Cancelled =
                        self.render_grid_event(event, &user_action_event_rx, true)?
                    {
                        tracing::info!("Rendering cancelled by user");
                        return Ok(RendererStatus::Cancelled);
                    }

                    // Sleep a bit to simulate rendering time
                    std::thread::sleep(self.render_refresh_time_scale.current());
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = self.grid_state.dims() {
            queue!(self.stdout, cursor::MoveTo(0, height))?;
            self.stdout.flush()?;
        }
        // Set done flag
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(RendererStatus::Completed)
    }
}
