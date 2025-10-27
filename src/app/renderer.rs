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
}

impl Renderer {
    pub fn new(max_history_grid_events: usize) -> Self {
        Self {
            stdout: std::io::stdout(),
            grid_dims: None,
            history: GridEventHistory::new(max_history_grid_events),
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

    /// Render loop that processes events from the receiver and renders them at specified intervals
    /// Returns Ok(true) if rendering completed successfully
    /// Returns Ok(false) if rendering was cancelled
    /// Returns Err if there was an I/O error
    pub fn render(
        &mut self,
        grid_event_rx: Receiver<GridEvent>,
        user_action_event_rx: Receiver<UserActionEvent>,
        render_refresh_time: Duration,
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
                                            // Resume rendering
                                            break;
                                        }
                                        UserActionEvent::Pause => {
                                            // Already paused, ignore
                                        }
                                        UserActionEvent::Forward => {
                                            // Browse forward in history
                                            if let Some(event) = self.history.history_forward() {
                                                tracing::debug!(
                                                    "Rendering history forward event: {:?}",
                                                    event
                                                );
                                            }
                                        }
                                        UserActionEvent::Backward => {
                                            // Browse backward in history
                                            if let Some(event) = self.history.history_backward() {
                                                tracing::debug!(
                                                    "Rendering history backward event: {:?}",
                                                    event
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No action event, continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Channel disconnected, ignore and continue
                }
            }
            // Block and wait for the next event
            match grid_event_rx.recv() {
                Err(_e) => {
                    // Channel disconnected, exit the thread
                    break;
                }
                Ok(event) => {
                    // Render the event
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Ok(false);
                    }

                    match event {
                        GridEvent::Initial {
                            cell,
                            width,
                            height,
                        } => {
                            self.grid_dims = Some((width, height));
                            if !Renderer::check_resize(
                                &mut self.stdout,
                                width,
                                height,
                                cancel_signal,
                            )? {
                                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                return Ok(false);
                            }

                            // Clear screen
                            // Move to top-left corner
                            // Print the whole grid with the specified cell

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
                                if !Renderer::check_resize(
                                    &mut self.stdout,
                                    width,
                                    height,
                                    cancel_signal,
                                )? {
                                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
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

                    // Add event to history
                    self.history.add_event(event);

                    // Sleep a bit to simulate rendering time
                    std::thread::sleep(render_refresh_time);
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = self.grid_dims {
            queue!(self.stdout, cursor::MoveTo(0, height), cursor::Show,)?;
            self.stdout.flush()?;
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    }
}
