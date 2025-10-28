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

use crate::{
    app::{UserActionEvent, history::GridEventHistory},
    maze::{cell::GridCell, grid::GridEvent},
};

pub struct Renderer {
    /// Standard output handle to write to the terminal
    stdout: Stdout,
    /// Current grid dimensions (width, height)
    grid_dims: Option<(u16, u16)>,
    /// History of grid events for browsing
    history: GridEventHistory,
    /// Time to wait between rendering events to simulate refresh time
    render_refresh_time: Duration,
}

impl Renderer {
    pub fn new(max_history_grid_events: usize, render_refresh_time: Duration) -> Self {
        Self {
            stdout: std::io::stdout(),
            grid_dims: None,
            history: GridEventHistory::new(max_history_grid_events),
            render_refresh_time,
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

    /// Handle user action events in a blocking manner until a Resume event is received
    /// This function returns when a Resume event is received
    fn handle_user_action_events(
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
                    match event {
                        UserActionEvent::Resume => {
                            // Step forward in the history and exit pause loop
                            while let Some(event) = self.history.history_forward().copied() {
                                tracing::debug!(
                                    "Rendering history forward event for resume: {:?}",
                                    event
                                );
                                self.render_grid_event(&event, cancel_signal)?;
                                // TODO: Do we sleep here to simulate rendering time?
                                std::thread::sleep(self.render_refresh_time);
                            }
                            break;
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
                            } else {
                                tracing::debug!("Attempting to step into the future");
                                match grid_event_rx.try_recv() {
                                    Ok(event) => {
                                        tracing::debug!("Rendering new future event: {:?}", event);
                                        self.render_grid_event(&event, cancel_signal)?;
                                        // Add event to history
                                        self.history.add_event(event);
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                                        // No action event, continue
                                        tracing::debug!("No future event available at the moment");
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        // Channel disconnected, ignore and continue
                                        // This loop will just wait for Resume to exit the pause state,
                                        // And the main render loop will eventually exit when it detects the disconnection
                                        tracing::debug!("Grid event channel disconnected");
                                    }
                                }
                            }
                        }
                        UserActionEvent::Backward => {
                            // Get the current event
                            let event = self.history.current_event().copied();
                            if let Some(event) = event {
                                // Craft the revert event
                                let revert_event = match event {
                                    GridEvent::Initial { .. } => None, // Cannot revert initial event
                                    GridEvent::Update { coord, old, new } => {
                                        Some(GridEvent::Update {
                                            coord,
                                            old: new,
                                            new: old,
                                        })
                                    }
                                };
                                // Render the revert event
                                if let Some(revert_event) = revert_event {
                                    tracing::debug!(
                                        "Rendering history backward event: {:?}",
                                        revert_event
                                    );
                                    self.render_grid_event(&revert_event, cancel_signal)?;
                                    // Move backward in history
                                    self.history.history_backward();
                                }
                            }
                        }
                        UserActionEvent::Resize => {
                            // Check terminal size against current grid dimensions
                            if let Some((width, height)) = self.grid_dims {
                                if !Renderer::check_resize(
                                    &mut self.stdout,
                                    width,
                                    height,
                                    cancel_signal,
                                )? {
                                    // Rendering was cancelled, exit
                                    break;
                                }
                            }
                        }
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
                    if let UserActionEvent::Pause = action_event {
                        // Block and handle subsequent user action events
                        self.handle_user_action_events(
                            &user_action_event_rx,
                            &grid_event_rx,
                            cancel_signal,
                        )?;
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
                    std::thread::sleep(self.render_refresh_time);
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
