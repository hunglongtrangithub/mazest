use std::{
    io::{Stdout, Write},
    sync::{Condvar, Mutex, atomic::AtomicBool, mpsc::Receiver},
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
pub struct RenderRefreshTimeScale {
    delta: Duration,
    /// number of discrete levels (quantization). e.g. 10
    levels: usize,
    /// current level in [0, levels - 1], 0 is slowest, levels-1 is fastest
    level: usize,
}

impl Default for RenderRefreshTimeScale {
    fn default() -> Self {
        // Base duration multiplier for level 1
        let base = Duration::from_micros(20);
        Self {
            delta: base,
            levels: 10, // default quantization
            level: 5,   // default mid-speed
        }
    }
}

impl RenderRefreshTimeScale {
    /// Create a calibrated RenderRefreshTimeScale based on the grid dimensions
    pub fn calibrated(grid_width: u8, grid_height: u8) -> Self {
        let mut scale = Self::default();
        // Map grid size to a sensible starting level.
        // Larger grids -> faster rendering (higher level index).
        let size = grid_width.max(grid_height) as f32;
        let max = u8::MAX as f32;
        let frac = (size / max).clamp(0.0, 1.0);
        // large size => faster, but clamp properly
        let lvl = (frac * (scale.levels.saturating_sub(1) as f32)).round() as usize;
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

    /// Get the current duration based on the square of (level+1).
    /// Level levels-1 -> delta * 1^2 (fastest), level 0 -> delta * levels^2 (slowest)
    pub fn current(&self) -> Duration {
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

pub struct Renderer {
    /// Standard output handle to write to the terminal
    stdout: Stdout,
    /// Current grid dimensions (width, height)
    grid_dims: Option<(u16, u16)>,
    /// History of grid events for browsing
    history: GridEventHistory,
    /// Render refresh time scale
    render_refresh_time_scale: RenderRefreshTimeScale,
}

impl Renderer {
    /// Create a new Renderer instance
    /// `max_history_grid_events` specifies the maximum number of grid events to keep in history
    /// `maze_dims` is an optional tuple of (width, height) to calibrate the render refresh time scale
    pub fn new(max_history_grid_events: usize, maze_dims: Option<(u8, u8)>) -> Self {
        Self {
            stdout: std::io::stdout(),
            grid_dims: None,
            history: GridEventHistory::new(max_history_grid_events),
            render_refresh_time_scale: match maze_dims {
                Some((width, height)) => RenderRefreshTimeScale::calibrated(width, height),
                None => RenderRefreshTimeScale::default(),
            },
        }
    }

    /// Check if terminal size is sufficient for the given grid dimensions
    /// If not, display a message and wait for user to press Esc, then return Ok(false)
    /// Returns Ok(true) if terminal size is sufficient
    /// Returns Err if there was an I/O error
    fn check_resize(
        stdout: &mut Stdout,
        width: u16,
        height: u16,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        let (term_width, term_height) = terminal::size()?;
        if term_width < width * GridCell::CELL_WIDTH || term_height < height {
            let msg = format!(
                "Terminal size is too small ({}x{}) for the grid dimensions ({}x{}) to display. Please resize the terminal.\r\n",
                width * GridCell::CELL_WIDTH,
                height,
                width,
                height
            );
            queue!(
                stdout,
                terminal::Clear(ClearType::All),
                cursor::MoveTo(0, 0),
                style::PrintStyledContent(msg.with(Color::Yellow).attribute(Attribute::Bold)),
                style::PrintStyledContent(
                    "Press Esc to exit...\r\n"
                        .with(Color::Blue)
                        .attribute(Attribute::Bold)
                )
            )?;
            stdout.flush()?;

            {
                let (cancel_mutex, cancel_condvar) = cancel_signal;
                // Wait for cancellation signal from input thread
                let mut canceled_guard = match cancel_mutex.lock() {
                    Ok(guard) => guard,
                    Err(_) => return Ok(false), // Mutex poisoned, treat as cancelled
                };
                while !*canceled_guard {
                    canceled_guard = match cancel_condvar.wait(canceled_guard) {
                        Ok(guard) => guard,
                        Err(_) => return Ok(false), // Mutex poisoned, treat as cancelled
                    };
                }
            }
            return Ok(false);
        }
        Ok(true)
    }

    /// Render a single grid event to the terminal
    /// Returns Ok(true) if rendering completed successfully
    /// Returns Ok(false) if rendering was cancelled
    /// Returns Err if there was an I/O error
    /// `cancel_signal` is used to wait for cancellation signal from user input thread if terminal resize is needed
    fn render_grid_event(
        &mut self,
        event: &GridEvent,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        // Render the event
        match event {
            GridEvent::Initial {
                cell,
                width,
                height,
            } => {
                let width = *width;
                let height = *height;
                self.grid_dims = Some((width, height));

                if !Renderer::check_resize(&mut self.stdout, width, height, cancel_signal)? {
                    return Ok(false);
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
            GridEvent::Update {
                coord,
                old: _old,
                new,
            } => match self.grid_dims {
                Some((width, height)) => {
                    if !Renderer::check_resize(&mut self.stdout, width, height, cancel_signal)? {
                        return Ok(false);
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
                // Skip if width and height are not set
                None => {}
            },
        }
        Ok(true)
    }

    fn get_width(&self) -> std::io::Result<u16> {
        // Get grid width / terminal width for logging purposes
        let width = match self.grid_dims {
            Some((w, _)) => w * GridCell::CELL_WIDTH,
            None => terminal::size()?.0,
        };
        Ok(width)
    }

    /// Log a message to the terminal below the grid without disrupting the grid display
    /// The cursor position is saved and restored after logging
    /// Returns Err if there was an I/O error
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
                match self.grid_dims {
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
    /// Returns Ok(true) if rendering should break from pause (on Resume)
    /// Returns Ok(false) otherwise
    /// Returns Err if there was an I/O error
    fn handle_user_action_event(
        &mut self,
        event: &UserActionEvent,
        grid_event_rx: &Receiver<GridEvent>,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        match event {
            UserActionEvent::Resume => {
                // Clear any log messages
                self.log_to_terminal(None)?;
                tracing::debug!("Resuming rendering from pause");
                // Step forward in the history and exit pause loop
                while let Some(event) = self.history.history_forward().copied() {
                    tracing::debug!("Rendering history forward event for resume: {:?}", event);
                    self.render_grid_event(&event, cancel_signal)?;
                    // TODO: Do we sleep here to simulate rendering time?
                    std::thread::sleep(self.render_refresh_time_scale.current());
                }
            }
            UserActionEvent::Pause => {
                // Already paused, ignore
            }
            UserActionEvent::Forward => {
                // Step forward and get the event
                let event = self.history.history_forward().copied();
                if let Some(event) = event {
                    tracing::debug!("Rendering history forward event: {:?}", event);
                    self.render_grid_event(&event, cancel_signal)?;
                    self.log_to_terminal(Some(event.to_string().with(Color::Green)))?;
                } else {
                    tracing::debug!("Attempting to step into the future");
                    match grid_event_rx.try_recv() {
                        Ok(event) => {
                            tracing::debug!("Rendering new future event: {:?}", event);
                            self.render_grid_event(&event, cancel_signal)?;
                            // Add event to history
                            self.history.add_event(event);
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
                let event = self.history.current_event().copied();
                if let Some(event) = event {
                    // Craft the revert event
                    let revert_event = match event {
                        GridEvent::Initial { .. } => None, // Cannot revert initial event
                        GridEvent::Update { coord, old, new } => Some(GridEvent::Update {
                            coord,
                            old: new,
                            new: old,
                        }),
                    };
                    // Render the revert event
                    if let Some(revert_event) = revert_event {
                        tracing::debug!("Rendering history backward event: {:?}", revert_event);
                        self.render_grid_event(&revert_event, cancel_signal)?;
                        // Move backward in history
                        self.history.history_backward();
                        self.log_to_terminal(Some(revert_event.to_string().with(Color::Yellow)))?;
                    }
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
                if let Some((width, height)) = self.grid_dims {
                    if !Renderer::check_resize(&mut self.stdout, width, height, cancel_signal)? {
                        // Rendering was cancelled, exit
                        return Ok(false);
                    }
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
        };

        // Only signal pause break on Resume event
        Ok(matches!(event, UserActionEvent::Resume))
    }

    /// Handle user action events in a blocking manner until a Resume event is received
    /// This function returns when a Resume event is received
    fn listen_to_user_action_events(
        &mut self,
        user_action_event_rx: &Receiver<UserActionEvent>,
        grid_event_rx: &Receiver<GridEvent>,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<()> {
        // Pause rendering until Resume event is received
        loop {
            match user_action_event_rx.recv() {
                Err(_e) => {
                    // Channel disconnected, ignore and continue
                    break;
                }
                Ok(event) => {
                    if self.handle_user_action_event(&event, grid_event_rx, cancel_signal)? {
                        // Resume event received, exit pause loop
                        break;
                    } else {
                        // Not a Resume event, continue pausing
                        continue;
                    }
                }
            }
        }
        Ok(())
    }

    /// Render loop that processes events from the user action and grid event channels
    /// Returns Ok(true) if rendering completed successfully
    /// Returns Ok(false) if rendering was cancelled
    /// Returns Err if there was an I/O error
    pub fn render(
        &mut self,
        grid_event_rx: Receiver<GridEvent>,
        user_action_event_rx: Receiver<UserActionEvent>,
        cancel: &AtomicBool,
        done: &AtomicBool,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        queue!(self.stdout, terminal::Clear(ClearType::All), cursor::Hide)?;
        self.stdout.flush()?;

        loop {
            // Try to receive user action events without blocking
            match user_action_event_rx.try_recv() {
                Ok(action_event) => {
                    tracing::debug!("Received user action event: {:?}", action_event);
                    match action_event {
                        UserActionEvent::Pause => {
                            // Block and handle subsequent user action events
                            tracing::info!("Pausing rendering on user request");
                            self.listen_to_user_action_events(
                                &user_action_event_rx,
                                &grid_event_rx,
                                cancel_signal,
                            )?;
                        }
                        UserActionEvent::SpeedUp
                        | UserActionEvent::SlowDown
                        | UserActionEvent::Resize => {
                            // Handle these events immediately
                            tracing::info!(
                                "Handling user action event immediately: {:?}",
                                action_event
                            );
                            self.handle_user_action_event(
                                &action_event,
                                &grid_event_rx,
                                cancel_signal,
                            )?;
                        }
                        _ => {}
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No action event, continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Channel disconnected, ignore and continue
                }
            }
            // Block and wait for the next grid event
            match grid_event_rx.recv() {
                Err(_e) => {
                    // Channel disconnected, exit the thread
                    break;
                }
                Ok(event) => {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Ok(false);
                    }

                    // Render the grid event
                    if !self.render_grid_event(&event, cancel_signal)? {
                        // Rendering was cancelled, set cancel flag and exit
                        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        return Ok(false);
                    }

                    // Add event to history
                    self.history.add_event(event);

                    // Sleep a bit to simulate rendering time
                    std::thread::sleep(self.render_refresh_time_scale.current());
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = self.grid_dims {
            queue!(self.stdout, cursor::MoveTo(0, height), cursor::Show,)?;
            self.stdout.flush()?;
        }
        // Set done flag
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    }
}
