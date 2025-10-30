mod history;
mod renderer;

use std::{
    io::{Stdout, Write},
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, KeyCode},
    queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};
use rand::Rng;

use crate::{
    app::renderer::{RenderRefreshTimeScale, Renderer, RendererStatus},
    generators::{Generator, generate_maze},
    maze::{Maze, cell::GridCell, grid::GridEvent},
    solvers::{Solver, solve_maze},
};

enum UserInputEvent {
    KeyPress(event::KeyEvent),
    Resize,
}

#[derive(Debug)]
enum UserActionEvent {
    /// Pause the animation
    Pause,
    /// Resume the animation
    Resume,
    /// Step forward in history or to the future when paused
    Forward,
    /// Step backward in history when paused
    Backward,
    /// Terminal resize
    Resize,
    /// Increase animation speed
    SpeedUp,
    /// Decrease animation speed
    SlowDown,
    /// Cancel rendering
    Cancel,
}

pub struct App {
    /// Timeout for receiving input events, a.k.a. how often to check for render done/cancel flags
    input_recv_timeout: Duration,
    /// Timeout for polling input events in the input thread, a.k.a.
    /// how often to check for render done/cancel flags
    user_input_event_poll_timeout: Duration,
    /// maximum number of grid events to keep for history browsing when paused or grid state
    /// recovery
    max_history_grid_events: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            // render_refresh_rate: Duration::from_micros(20),
            input_recv_timeout: Duration::from_millis(100),
            user_input_event_poll_timeout: Duration::from_millis(100),
            max_history_grid_events: 10000,
        }
    }
}

impl App {
    /// Maximum number of grid events to buffer in the channel between compute and render threads
    const MAX_EVENTS_IN_CHANNEL_BUFFER: usize = 1000;
    /// Available maze generators
    const GENERATORS: [Generator; 4] = [
        Generator::RecurBacktrack,
        Generator::Kruskal,
        Generator::Prim,
        Generator::RecurDiv,
    ];
    /// Available maze solvers
    const SOLVERS: [Solver; 4] = [Solver::Dfs, Solver::Bfs, Solver::Dijkstra, Solver::AStar];

    /// Set a panic hook to restore terminal state on panic
    /// This ensures that the terminal is not left in raw mode or alternate screen on panic
    /// even if the panic occurs in a different thread
    fn set_panic_hook() {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = App::restore_terminal(&mut std::io::stdout()); // ignore any errors as we are already failing
            hook(panic_info);
        }));
    }

    /// Setup terminal in raw mode and enter alternate screen
    /// Also sets a panic hook to restore terminal on panic
    pub fn setup_terminal(stdout: &mut Stdout) -> std::io::Result<()> {
        terminal::enable_raw_mode()?;
        App::set_panic_hook();
        crossterm::queue!(
            stdout,
            terminal::EnterAlternateScreen,
            terminal::Clear(ClearType::All),
            cursor::Hide,
            cursor::MoveTo(0, 0)
        )?;
        stdout.flush()?;
        Ok(())
    }

    /// Restore terminal to original state
    /// Leave alternate screen and disable raw mode
    pub fn restore_terminal(stdout: &mut Stdout) -> std::io::Result<()> {
        queue!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
        stdout.flush()?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    /// Main application loop
    pub fn run(&self, stdout: &mut Stdout) -> std::io::Result<()> {
        // Ask user for maze dimensions
        let (width, height) = match App::ask_maze_dimensions(stdout)? {
            Some(dims) => dims,
            None => {
                return Ok(());
            }
        };

        // Ask user for maze generation algorithm
        let mut generator = match App::select_from_menu(
            stdout,
            "Select maze generation algorithm (use arrow keys and Enter, or Esc to exit):",
            &App::GENERATORS,
        )? {
            Some(generator) => {
                stdout.execute(style::PrintStyledContent(
                    format!("Selected generator: {}\r\n", generator)
                        .with(Color::Green)
                        .attribute(Attribute::Bold),
                ))?;
                generator
            }
            None => {
                return Ok(());
            }
        };

        // Ask user for maze solving algorithm
        let mut solver = match App::select_from_menu(
            stdout,
            "Select maze solving algorithm (use arrow keys and Enter, or Esc to exit):",
            &App::SOLVERS,
        )? {
            Some(solver) => {
                stdout.execute(style::PrintStyledContent(
                    format!("Selected solver: {}\r\n", solver)
                        .with(Color::Green)
                        .attribute(Attribute::Bold),
                ))?;
                solver
            }
            None => {
                return Ok(());
            }
        };

        queue!(
            stdout,
            style::PrintStyledContent(
                "Controls:\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)
            ),
            style::PrintStyledContent("  Enter: Pause/Resume animation\r\n".with(Color::Cyan)),
            style::PrintStyledContent(
                "  ←/→: Step backward/forward when paused\r\n".with(Color::Cyan)
            ),
            style::PrintStyledContent("  ↑/↓: Speed up/slow down animation\r\n".with(Color::Cyan)),
            style::PrintStyledContent("  Esc: Exit\r\n\r\n".with(Color::Cyan)),
        )?;

        stdout.flush()?;
        // Ask if user wants to loop generation and solving
        let loop_animation = match App::select_from_menu(
            stdout,
            "Loop maze generation and solving? Will randomize generator & solver combination. (use arrow keys and Enter, or Esc to exit):",
            &["Yes", "No"],
        )? {
            Some(choice) => choice == "Yes",
            None => {
                return Ok(());
            }
        };

        // Flag to indicate rendering is done. Set to true by the render thread when it finishes.
        let render_done = Arc::new(AtomicBool::new(false));
        // Flag to indicate rendering should be cancelled. Set to true by the main thread on Esc key event.
        let render_cancel = Arc::new(AtomicBool::new(false));

        let (user_input_event_tx, user_input_event_rx) =
            std::sync::mpsc::channel::<UserInputEvent>();
        let user_input_event_poll_timeout = self.user_input_event_poll_timeout;
        let render_done_for_input = render_done.clone();
        let render_cancel_for_input = render_cancel.clone();
        // Spawn a thread to listen for user input
        let input_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
            App::listen_to_user_input(
                user_input_event_tx,
                user_input_event_poll_timeout,
                &render_done_for_input,
                &render_cancel_for_input,
            )
        });

        let (grid_event_tx, grid_event_rx) =
            std::sync::mpsc::sync_channel::<GridEvent>(App::MAX_EVENTS_IN_CHANNEL_BUFFER);
        let (user_action_event_tx, user_action_event_rx) =
            std::sync::mpsc::channel::<UserActionEvent>();

        // Spawn a thread to listen for grid updates and render the maze
        let max_history_grid_events = self.max_history_grid_events;
        let render_cancel_for_render = render_cancel.clone();
        let render_done_for_render = render_done.clone();
        let render_thread_handle = std::thread::spawn(move || {
            let mut renderer = Renderer::new(max_history_grid_events, Some((width, height)));
            renderer.render(
                grid_event_rx,
                user_action_event_rx,
                &render_cancel_for_render,
                &render_done_for_render,
            )
        });

        // Spawn a thread to generate maze and solve it
        let combos = App::GENERATORS
            .iter()
            .flat_map(|&generator| App::SOLVERS.iter().map(move |&solver| (generator, solver)))
            .collect::<Vec<(Generator, Solver)>>();
        let render_cancel_for_compute = render_cancel.clone();
        let compute_thread_handle = std::thread::spawn(move || -> bool {
            if !loop_animation {
                return App::compute(width, height, grid_event_tx, generator, solver);
            }
            // Looping mode: randomly select generator and solver each iteration
            let mut rng = rand::rng();
            loop {
                let goal_reached =
                    App::compute(width, height, grid_event_tx.clone(), generator, solver);
                // Check if rendering was cancelled
                if render_cancel_for_compute.load(std::sync::atomic::Ordering::Relaxed) {
                    tracing::info!("Compute thread detected render cancel, exiting loop");
                    return goal_reached;
                }
                // Randomly select new generator and solver combination for next iteration
                (generator, solver) = combos[rng.random_range(0..combos.len())];
            }
        });

        // Main thread loop to listen for user input events during rendering
        self.app_loop(
            user_input_event_rx,
            user_action_event_tx,
            render_done,
            render_cancel,
        );

        // Wait for input thread to finish
        let _ = input_thread_handle.join();

        // Wait for compute thread to finish
        let goal_reached = compute_thread_handle
            .join()
            .expect("Compute thread panicked");

        // Wait for render thread to finish
        let completed = render_thread_handle
            .join()
            .expect("Render thread panicked")?;

        if let RendererStatus::Cancelled = completed {
            tracing::info!("Rendering was cancelled by user.");
            return Ok(());
        }

        let msg = if goal_reached {
            "Path found! "
        } else {
            "No path found. "
        };
        stdout.execute(style::PrintStyledContent(
            msg.with(Color::Green).attribute(Attribute::Bold),
        ))?;

        stdout.execute(style::PrintStyledContent(
            "Press Esc to exit...\r\n"
                .with(Color::Blue)
                .attribute(Attribute::Bold),
        ))?;
        // Wait for user to press Esc
        App::wait_for_esc()?;
        Ok(())
    }

    /// Profiling mode: run animations in the background without rendering to terminal
    pub fn profile(
        &self,
        width: u8,
        height: u8,
        solver: Solver,
        generator: Generator,
        num_animation_iterations: Option<usize>,
    ) -> std::io::Result<()> {
        let (grid_event_tx, grid_event_rx) =
            std::sync::mpsc::sync_channel::<GridEvent>(App::MAX_EVENTS_IN_CHANNEL_BUFFER);

        // Spawn a thread to listen for grid updates and render the maze
        let render_refresh_time = RenderRefreshTimeScale::calibrated(width, height).current();
        let render_thread_handle = std::thread::spawn(move || {
            loop {
                match grid_event_rx.recv() {
                    Err(_e) => {
                        // Channel disconnected, exit the thread
                        break;
                    }
                    Ok(_event) => {
                        // For profiling mode, we just discard the event
                        std::thread::sleep(render_refresh_time);
                    }
                }
            }
        });

        let compute_thread_handle = std::thread::spawn(move || match num_animation_iterations {
            Some(iterations) => {
                for _ in 0..iterations {
                    App::compute(width, height, grid_event_tx.clone(), generator, solver);
                }
            }
            None => {
                App::compute(width, height, grid_event_tx, generator, solver);
            }
        });

        // Wait for compute thread to finish
        compute_thread_handle
            .join()
            .expect("Compute thread panicked");

        // Wait for render thread to finish
        render_thread_handle.join().expect("Render thread panicked");

        Ok(())
    }

    /// App loop after starting input and render threads
    fn app_loop(
        &self,
        user_input_event_rx: Receiver<UserInputEvent>,
        user_action_event_tx: Sender<UserActionEvent>,
        render_done: Arc<AtomicBool>,
        render_cancel: Arc<AtomicBool>,
    ) {
        tracing::info!("Started main app loop");
        // Flag to indicate if the animation is currently paused
        let mut is_paused = false;
        loop {
            // Check if render is done
            if render_done.load(std::sync::atomic::Ordering::Relaxed) {
                // Drop the receiver to signal input thread to exit
                drop(user_input_event_rx);
                break;
            }

            let event = match user_input_event_rx.recv_timeout(self.input_recv_timeout) {
                Err(e) => {
                    match e {
                        std::sync::mpsc::RecvTimeoutError::Timeout => {
                            // Skip to next iteration to check render_done again
                            continue;
                        }
                        std::sync::mpsc::RecvTimeoutError::Disconnected => {
                            // Input thread has exited, break the loop
                            break;
                        }
                    }
                }
                Ok(event) => match event {
                    UserInputEvent::KeyPress(key_event) => {
                        match key_event.code {
                            // Exit on Esc key
                            KeyCode::Esc => {
                                tracing::debug!("[app loop] Esc key pressed, notifying renderer");
                                // Error only happens if user_input_event_rx is dropped, which
                                // means Renderer::render has exited already
                                user_action_event_tx.send(UserActionEvent::Cancel).ok();
                                render_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                break;
                            }
                            KeyCode::Enter => {
                                // Toggle pause/resume on Enter key
                                let event = if is_paused {
                                    UserActionEvent::Resume
                                } else {
                                    UserActionEvent::Pause
                                };
                                // Toggle pause state
                                is_paused = !is_paused;
                                Some(event)
                            }
                            KeyCode::Left if is_paused => {
                                // Step backward when paused
                                Some(UserActionEvent::Backward)
                            }
                            KeyCode::Right if is_paused => {
                                // Step forward when paused
                                Some(UserActionEvent::Forward)
                            }
                            KeyCode::Up => {
                                // Speed up animation
                                Some(UserActionEvent::SpeedUp)
                            }
                            KeyCode::Down => {
                                // Slow down animation
                                Some(UserActionEvent::SlowDown)
                            }
                            _ => None, // Ignore other keys
                        }
                    }
                    UserInputEvent::Resize => Some(UserActionEvent::Resize),
                },
            };

            // Send the user action event to the render thread
            if let Some(event) = event {
                if user_action_event_tx.send(event).is_err() {
                    // Render thread has exited
                    break;
                }
            }
        }
        // The user_input_event_rx and user_action_event_tx are dropped here
        tracing::info!("Exiting main app loop");
    }

    /// Listen for user input events (key presses and resize)
    /// This function runs in a separate thread, and is the only place where user input is read
    fn listen_to_user_input(
        user_input_event_tx: Sender<UserInputEvent>,
        event_poll_timeout: Duration,
        render_done: &AtomicBool,
        render_cancel: &AtomicBool,
    ) -> std::io::Result<()> {
        loop {
            // Check if render is done or canceled
            if render_done.load(std::sync::atomic::Ordering::Relaxed)
                || render_cancel.load(std::sync::atomic::Ordering::Relaxed)
            {
                return Ok(());
            }

            // Poll for events with a timeout
            if !event::poll(event_poll_timeout)? {
                // No event available, continue loop to check flags again
                continue;
            }

            // Read the next event
            // We only care about key presses events for now
            let input_event = match event::read()? {
                event::Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                    UserInputEvent::KeyPress(key_event)
                }
                event::Event::Resize(_, _) => UserInputEvent::Resize,
                _ => continue, // Ignore other events
            };

            // Should exit input thread on Esc key
            let should_exit = matches!(
                input_event,
                UserInputEvent::KeyPress(event::KeyEvent {
                    code: KeyCode::Esc,
                    ..
                })
            );

            // Send the input event to the main thread
            if user_input_event_tx.send(input_event).is_err() {
                // Receiver has been dropped, exit the thread
                return Ok(());
            }

            if should_exit {
                tracing::debug!("[input loop] Esc key pressed, exiting");
                return Ok(());
            }
        }
    }

    /// Generate and solve the maze
    /// Returns whether the goal was reached
    fn compute(
        width: u8,
        height: u8,
        grid_event_tx: std::sync::mpsc::SyncSender<GridEvent>,
        generator: Generator,
        solver: Solver,
    ) -> bool {
        let mut maze = Maze::new(width, height, Some(grid_event_tx));
        // Generate the maze using the selected algorithm
        generate_maze(&mut maze, generator, None);

        // Solve the maze using the selected algorithm
        solve_maze(&mut maze, solver)
        // Maze is dropped here, as well as the grid_event_tx sender
    }

    /// Wait for the user to press the Esc key
    /// This function blocks until Esc is pressed
    fn wait_for_esc() -> std::io::Result<()> {
        loop {
            if let event::Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
                if code == KeyCode::Esc && kind == event::KeyEventKind::Press {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get user input with real-time validation and feedback
    /// Returns None if user cancels input with Esc
    /// Returns Some(T) if user inputs a valid input and presses Enter, where T is the validated type
    fn prompt_with_validation<F, T>(
        stdout: &mut Stdout,
        prompt: &str,
        validate: F,
    ) -> std::io::Result<Option<T>>
    where
        F: Fn(&str) -> Result<T, String>,
    {
        // Save cursor position so we can restore / redraw
        queue!(stdout, cursor::Hide, cursor::SavePosition)?;
        stdout.flush()?;

        let mut input = String::new();

        let number_option = loop {
            // Re-render prompt line
            queue!(
                stdout,
                cursor::RestorePosition,
                terminal::Clear(ClearType::FromCursorDown)
            )?;

            // Print prompt
            stdout.queue(style::PrintStyledContent(
                prompt.with(Color::Cyan).attribute(Attribute::Bold),
            ))?;

            // Decide color based on validity
            let validation_result = validate(input.trim());
            match validation_result {
                Ok(_) => {
                    stdout.queue(style::SetForegroundColor(Color::Green))?;
                }
                Err(_) => {
                    stdout.queue(style::SetForegroundColor(Color::Red))?;
                }
            }

            queue!(stdout, style::Print(&input), style::ResetColor)?;

            stdout.queue(style::Print(" \r\n"))?;

            // Error message line (if any)
            if let Err(msg) = validation_result {
                stdout.queue(style::PrintStyledContent(
                    msg.with(Color::DarkGrey).attribute(Attribute::Dim),
                ))?;
            }

            stdout.flush()?;

            // Wait for key event
            if let event::Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
                match code {
                    KeyCode::Enter => {
                        match validate(&input) {
                            Ok(n) => break Some(n), // valid number, exit loop
                            Err(_) => continue,     // invalid, re-render
                        }
                        // otherwise, stay in loop
                    }
                    KeyCode::Char(c) if kind == event::KeyEventKind::Press => {
                        if !c.is_whitespace() && !c.is_control() {
                            input.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Esc => {
                        // User cancelled input
                        break None;
                    }
                    _ => {}
                }
            }
        };
        // Cleanup
        queue!(
            stdout,
            cursor::RestorePosition,
            terminal::Clear(ClearType::FromCursorDown),
            cursor::Show
        )?;
        stdout.flush()?;

        Ok(number_option)
    }

    /// Calculate max maze size based on terminal size and cell size
    /// Ensures the size is odd and at least 3
    fn get_max_maze_size(term_size: u16, cell_size: u16) -> u8 {
        // Get default grid dimension based on terminal size. Make sure they are odd and at least 3.
        let odd_and_min_3 = |n: u16| if n % 2 == 0 && n > 0 { n - 1 } else { n }.max(3);
        let max_grid_size = odd_and_min_3(term_size / cell_size);

        // Default maze dimensions are half the grid dimensions, capped at u8::MAX
        (max_grid_size / 2).min(u8::MAX as u16) as u8
    }

    /// Ask user for maze dimensions (width and height between 1 and 255)
    /// Returns None if user cancels input with Esc
    /// Returns Some((width, height)) if user inputs valid dimensions
    fn ask_maze_dimensions(stdout: &mut Stdout) -> std::io::Result<Option<(u8, u8)>> {
        stdout.execute(style::PrintStyledContent(
            "Enter maze dimensions (width and height between 1 and 255), or press Esc to exit. \
Maximum acceptable values are based on current terminal size.\r\n"
                .with(Color::Blue),
        ))?;

        // Validation closure based on default sizes
        let validate = |s: &str, is_width| {
            let max_size = if let Ok((term_width, term_height)) = terminal::size() {
                if is_width {
                    App::get_max_maze_size(term_width, GridCell::CELL_WIDTH)
                } else {
                    // Reserve rows for logs
                    App::get_max_maze_size(term_height.saturating_sub(Renderer::NUM_LOG_ROWS), 1)
                }
            } else {
                // Fallback to max size if terminal size cannot be determined
                u8::MAX
            };

            if s.trim().is_empty() {
                return Ok(max_size);
            }

            let error_msg = format!("Please enter a valid number between 1 and {}.", max_size);
            s.parse::<u8>()
                .map_err(|_| error_msg.clone())
                .and_then(|n| match n {
                    1..=255 if n <= max_size => Ok(n),
                    _ => Err(error_msg),
                })
        };

        let validate_width = |s: &str| validate(s, true);
        let validate_height = |s: &str| validate(s, false);

        let width = match App::prompt_with_validation(stdout, "Width: ", validate_width)? {
            Some(w) => w,
            None => return Ok(None),
        };
        stdout.execute(style::PrintStyledContent(
            format!("Width set to {}\r\n", width)
                .with(Color::Green)
                .attribute(Attribute::Bold),
        ))?;

        let height = match App::prompt_with_validation(stdout, "Height: ", validate_height)? {
            Some(h) => h,
            None => return Ok(None),
        };
        stdout.execute(style::PrintStyledContent(
            format!("Height set to {}\r\n", height)
                .with(Color::Green)
                .attribute(Attribute::Bold),
        ))?;

        Ok(Some((width, height)))
    }

    /// Present a menu of options to the user and let them select one using arrow keys
    /// Returns None if user cancels input with Esc
    /// Returns Some(T) if user selects an option and presses Enter, where T is the option type
    fn select_from_menu<T: std::fmt::Display + Copy>(
        stdout: &mut Stdout,
        prompt: &str,
        options: &[T],
    ) -> std::io::Result<Option<T>> {
        if options.is_empty() {
            return Ok(None);
        }

        // Save cursor position so we can restore / redraw
        queue!(stdout, cursor::Hide, cursor::SavePosition)?;

        let mut selected = 0;

        let selected_option = loop {
            // Re-render prompt line
            queue!(
                stdout,
                cursor::RestorePosition,
                terminal::Clear(ClearType::FromCursorDown)
            )?;

            // Print prompt
            stdout.queue(style::PrintStyledContent(prompt.with(Color::Yellow)))?;

            // Print options
            for (i, option) in options.iter().enumerate() {
                if i == selected {
                    stdout.queue(style::SetAttribute(Attribute::Reverse))?;
                }
                stdout.queue(style::Print(format!("\r\n{}", option)))?;
                if i == selected {
                    stdout.queue(style::SetAttribute(Attribute::NoReverse))?;
                }
            }
            stdout.queue(style::Print("\r\n"))?;

            stdout.flush()?;

            // Wait for key event
            if let event::Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
                if kind != event::KeyEventKind::Press {
                    // Only handle key press events
                    continue;
                }
                match code {
                    KeyCode::Up => {
                        selected = match selected {
                            0 => options.len() - 1,
                            _ => selected - 1,
                        };
                    }
                    KeyCode::Down => {
                        selected = if selected >= options.len() - 1 {
                            0
                        } else {
                            selected + 1
                        };
                    }
                    KeyCode::Enter => {
                        break Some(options[selected]);
                    }
                    KeyCode::Esc => {
                        // User cancelled input
                        break None;
                    }
                    _ => {}
                }
            }
        };
        // Cleanup
        queue!(
            stdout,
            cursor::RestorePosition,
            terminal::Clear(ClearType::FromCursorDown),
            cursor::Show
        )?;
        stdout.flush()?;

        Ok(selected_option)
    }
}
