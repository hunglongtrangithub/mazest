use std::{
    io::{Stdout, Write},
    sync::{
        Arc, Condvar, Mutex,
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
    generators::{Generator, generate_maze},
    maze::{Maze, cell::GridCell, grid::GridEvent},
    solvers::{Solver, solve_maze},
};

enum UserInputEvent {
    KeyPress(event::KeyEvent),
}

#[derive(Debug)]
enum UserActionEvent {
    Pause,
    Resume,
    Forward,
    Backward,
}

pub struct App {
    /// Interval at which to render the event buffer
    render_interval: Duration,
    /// Time taken to render each grid update when grid size is u8::MAX
    render_refresh_rate: Duration,
    /// Timeout for receiving input events, a.k.a. how often to check for render done/cancel flags
    input_recv_timeout: Duration,
    /// Timeout for polling input events in the input thread, a.k.a. how often to check for render done/cancel flags
    user_input_event_poll_timeout: Duration,
}

impl Default for App {
    fn default() -> Self {
        Self {
            render_interval: Duration::from_millis(100),
            render_refresh_rate: Duration::from_micros(20),
            input_recv_timeout: Duration::from_millis(100),
            user_input_event_poll_timeout: Duration::from_millis(100),
        }
    }
}

impl App {
    const MAX_EVENTS_IN_CHANNEL_BUFFER: usize = 1000;
    const MAX_EVENTS_IN_RENDER_BUFFER: usize = 1000;
    const GENERATORS: [Generator; 4] = [
        Generator::RecurBacktrack,
        Generator::Kruskal,
        Generator::Prim,
        Generator::RecurDiv,
    ];
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
        // Ask user for grid dimensions
        let (width, height) = match App::ask_maze_dimensions(stdout)? {
            Some(dims) => dims,
            None => {
                return Ok(());
            }
        };

        // Check if terminal height and width are sufficient
        let (term_width, term_height) = terminal::size()?;
        if term_width < width as u16 * GridCell::CELL_WIDTH || term_height < height as u16 {
            queue!(stdout, style::PrintStyledContent(
                "Terminal size is too small for the maze dimensions to display. Please resize the terminal.\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)),
                style::PrintStyledContent("Press Esc to exit...\r\n".with(Color::Blue).attribute(Attribute::Bold))
            )?;
            stdout.flush()?;
            // Wait for user to press Esc
            App::wait_for_esc()?;
            return Ok(());
        }

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
        // Flag to indicate rendering should be cancelled. Set to true by the input thread on Esc key.
        let render_cancel = Arc::new(AtomicBool::new(false));
        // Cancellation signal (mutex + condvar) for render thread to wait on from input thread
        // when terminal resize is insufficient
        let cancel_signal = (Arc::new(Mutex::new(false)), Arc::new(Condvar::new()));

        let (user_input_event_tx, user_input_event_rx) =
            std::sync::mpsc::channel::<UserInputEvent>();
        let user_input_event_poll_timeout = self.user_input_event_poll_timeout;
        let render_done_for_input = render_done.clone();
        let render_cancel_for_input = render_cancel.clone();
        let cancel_signal_for_input = cancel_signal.clone();
        // Spawn a thread to listen for user input
        let input_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
            App::listen_to_user_input(
                user_input_event_tx,
                user_input_event_poll_timeout,
                &render_done_for_input,
                &render_cancel_for_input,
                (&cancel_signal_for_input.0, &cancel_signal_for_input.1),
            )
        });

        let (grid_event_tx, grid_event_rx) =
            std::sync::mpsc::sync_channel::<GridEvent>(App::MAX_EVENTS_IN_CHANNEL_BUFFER);
        let (user_action_event_tx, user_action_event_rx) =
            std::sync::mpsc::channel::<UserActionEvent>();

        // Spawn a thread to listen for grid updates and render the maze
        let render_refresh_time = self.calculate_render_refresh_time(width, height);
        let render_interval = self.render_interval;
        let render_cancel_for_render = render_cancel.clone();
        let render_done_for_render = render_done.clone();
        let render_thread_handle = std::thread::spawn(move || {
            App::render(
                render_interval,
                grid_event_rx,
                user_action_event_rx,
                render_refresh_time,
                &render_cancel_for_render,
                &render_done_for_render,
                (&cancel_signal.0, &cancel_signal.1),
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
                    return goal_reached;
                }
                // Randomly select new generator and solver combination for next iteration
                (generator, solver) = combos[rng.random_range(0..combos.len())];
            }
        });

        // Main thread loop to listen for user input events during rendering
        let app_loop = |input_recv_timeout: Duration| {
            tracing::info!("Started main app loop");
            // Flag to indicate if the animation is currently paused
            let mut is_paused = false;
            loop {
                // Check if render is done, or canceled by input thread
                if render_done.load(std::sync::atomic::Ordering::Relaxed)
                    || render_cancel.load(std::sync::atomic::Ordering::Relaxed)
                {
                    // Drop the receiver to signal input thread to exit
                    drop(user_input_event_rx);
                    break;
                }

                match user_input_event_rx.recv_timeout(input_recv_timeout) {
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
                                    tracing::info!("Esc key pressed, cancelling render");
                                    render_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                    // Close the channels to signal input thread and render thread to exit (if not already)
                                    drop(user_input_event_rx);
                                    drop(user_action_event_tx);
                                    break;
                                }
                                KeyCode::Enter => {
                                    // Toggle pause/resume on Enter key
                                    let event = if is_paused {
                                        UserActionEvent::Resume
                                    } else {
                                        UserActionEvent::Pause
                                    };
                                    tracing::info!("Sending user action event: {:?}", event);
                                    if user_action_event_tx.send(event).is_ok() {
                                        // Toggle pause state
                                        is_paused = !is_paused;
                                    } else {
                                        // Receiver has been dropped, exit the loop
                                        break;
                                    }
                                }
                                KeyCode::Left if is_paused => {
                                    // Step backward when paused
                                    if user_action_event_tx
                                        .send(UserActionEvent::Backward)
                                        .is_err()
                                    {
                                        // Receiver has been dropped, exit the loop
                                        break;
                                    }
                                }
                                KeyCode::Right if is_paused => {
                                    // Step forward when paused
                                    if user_action_event_tx.send(UserActionEvent::Forward).is_err()
                                    {
                                        // Receiver has been dropped, exit the loop
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                    },
                }
            }
        };
        app_loop(self.input_recv_timeout);

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

        if !completed {
            return Ok(());
        }

        let msg = if goal_reached {
            "Path found!\r\n"
        } else {
            "No path found.\r\n"
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
        let render_refresh_time = self.calculate_render_refresh_time(width, height);
        let render_interval = self.render_interval;
        let render_thread_handle = std::thread::spawn(move || {
            let mut event_buffer = Vec::new();
            let mut last_render = std::time::Instant::now();
            loop {
                match grid_event_rx.recv() {
                    Err(_e) => {
                        // Channel disconnected, exit the thread
                        for _event in event_buffer.drain(..) {
                            std::thread::sleep(render_refresh_time);
                        }
                        break;
                    }
                    Ok(event) => {
                        // For profiling mode, we just discard the events
                        // In a real application, we could log them or analyze them
                        event_buffer.push(event);
                        if last_render.elapsed() >= render_interval
                            || event_buffer.len() >= App::MAX_EVENTS_IN_CHANNEL_BUFFER
                        {
                            // Reset the timer
                            last_render = std::time::Instant::now();

                            // Simulate rendering time
                            for _ in event_buffer.drain(..) {
                                std::thread::sleep(render_refresh_time);
                            }
                        }
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

    /// Listen for user input events (key presses and resize)
    fn listen_to_user_input(
        user_input_event_tx: Sender<UserInputEvent>,
        event_poll_timeout: Duration,
        render_done: &AtomicBool,
        render_cancel: &AtomicBool,
        cancel_signal: (&Mutex<bool>, &Condvar),
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
                // No event available, continue loop to check render_done again
                continue;
            }

            // Read the next event
            // We only care about key presses events for now
            let input_event = match event::read()? {
                event::Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                    UserInputEvent::KeyPress(key_event)
                }
                _ => continue,
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
                // Set cancel flag
                render_cancel.store(true, std::sync::atomic::Ordering::Relaxed);

                let (cancel_mutex, cancel_condvar) = cancel_signal;
                // Signal cancellation and exit the thread
                {
                    let mut cancelled = match cancel_mutex.lock() {
                        Ok(guard) => guard,
                        Err(_) => return Ok(()), // Mutex poisoned, exit thread
                    };
                    *cancelled = true;
                    cancel_condvar.notify_all();
                }
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

    /// Calculate default maze size based on terminal size and cell size
    /// Ensures the size is odd and at least 3
    fn get_default_maze_size(term_size: u16, cell_size: u16) -> u8 {
        // Get default grid dimension based on terminal size. Make sure they are odd and at least 3.
        let odd_and_min_3 = |n: u16| if n % 2 == 0 && n > 0 { n - 1 } else { n }.max(3);
        let default_grid_size = odd_and_min_3(term_size / cell_size);

        // Default maze dimensions are half the grid dimensions, capped at u8::MAX
        (default_grid_size / 2).min(u8::MAX as u16) as u8
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
            let default_size = if let Ok((term_width, term_height)) = terminal::size() {
                if is_width {
                    App::get_default_maze_size(term_width, GridCell::CELL_WIDTH)
                } else {
                    App::get_default_maze_size(term_height, 1)
                }
            } else {
                // Fallback to max size if terminal size cannot be determined
                u8::MAX
            };

            if s.trim().is_empty() {
                return Ok(default_size);
            }

            let error_msg = format!(
                "Please enter a valid number between 1 and {}.",
                default_size
            );
            s.parse::<u8>()
                .map_err(|_| error_msg.clone())
                .and_then(|n| match n {
                    1..=255 if n <= default_size => Ok(n),
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

    fn calculate_render_refresh_time(&self, grid_width: u8, grid_height: u8) -> Duration {
        let size = grid_width.max(grid_height) as usize;
        self.render_refresh_rate * (u8::MAX as u32 / size as u32).pow(2)
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

    /// Process and render all events in the event buffer
    /// Returns Ok(true) if processing completed successfully
    /// Returns Ok(false) if processing was cancelled
    /// Returns Err if there was an I/O error
    fn process_events(
        event_buffer: &mut Vec<GridEvent>,
        stdout: &mut Stdout,
        grid_dims: &mut Option<(u16, u16)>,
        render_refresh_time: Duration,
        cancel: &AtomicBool,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        for event in event_buffer.drain(..) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(false);
            }

            match event {
                GridEvent::Initial {
                    cell,
                    width,
                    height,
                } => {
                    *grid_dims = Some((width, height));
                    if !App::check_resize(stdout, width, height, cancel_signal)? {
                        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        return Ok(false);
                    }

                    // Clear screen
                    // Move to top-left corner
                    // Print the whole grid with the specified cell

                    stdout.queue(cursor::MoveTo(0, 0))?;
                    for _y in 0..height {
                        for _x in 0..width {
                            stdout.queue(style::Print(cell))?;
                        }
                        stdout.queue(style::Print("\r\n"))?;
                    }
                    stdout.flush()?;
                }
                GridEvent::Update {
                    coord,
                    old: _old,
                    new,
                } => match grid_dims {
                    Some((width, height)) => {
                        if !App::check_resize(stdout, *width, *height, cancel_signal)? {
                            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                            return Ok(false);
                        }
                        // Move the cursor to the specified coordinate and print the
                        // new cell using the grid dimensions
                        queue!(
                            stdout,
                            cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1),
                            style::Print(new)
                        )?;
                        stdout.flush()?;
                    }
                    // Skip if width and height are not set
                    None => continue,
                },
            }
            // Sleep a bit to simulate rendering time
            std::thread::sleep(render_refresh_time);
        }
        Ok(true)
    }

    /// Render loop that processes events from the receiver and renders them at specified intervals
    /// Returns Ok(true) if rendering completed successfully
    /// Returns Ok(false) if rendering was cancelled
    /// Returns Err if there was an I/O error
    fn render(
        render_interval: Duration,
        grid_event_rx: Receiver<GridEvent>,
        user_action_event_rx: Receiver<UserActionEvent>,
        render_refresh_time: Duration,
        cancel: &AtomicBool,
        done: &AtomicBool,
        cancel_signal: (&Mutex<bool>, &Condvar),
    ) -> std::io::Result<bool> {
        let mut stdout = std::io::stdout();
        let mut event_buffer = Vec::new();
        let mut last_render = std::time::Instant::now();
        let mut grid_dims: Option<(u16, u16)> = None;

        queue!(stdout, terminal::Clear(ClearType::All), cursor::Hide)?;
        stdout.flush()?;

        loop {
            // Try to receive user action events without blocking
            match user_action_event_rx.try_recv() {
                Ok(action_event) => {
                    tracing::info!("Received user action event: {:?}", action_event);
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
                                            // Ignore for now
                                        }
                                        UserActionEvent::Backward => {
                                            // Ignore for now
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
                    // Channel disconnected, render the remaining buffer and exit
                    if !App::process_events(
                        &mut event_buffer,
                        &mut stdout,
                        &mut grid_dims,
                        render_refresh_time,
                        cancel,
                        cancel_signal,
                    )? {
                        // Cancelled
                        return Ok(false);
                    }
                    break;
                }
                Ok(event) => {
                    event_buffer.push(event);
                    if last_render.elapsed() >= render_interval
                        || event_buffer.len() >= App::MAX_EVENTS_IN_RENDER_BUFFER
                    {
                        // Reset the timer
                        last_render = std::time::Instant::now();

                        // Render all buffered events
                        if !App::process_events(
                            &mut event_buffer,
                            &mut stdout,
                            &mut grid_dims,
                            render_refresh_time,
                            cancel,
                            cancel_signal,
                        )? {
                            // Cancelled
                            return Ok(false);
                        }
                    }
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = grid_dims {
            queue!(stdout, cursor::MoveTo(0, height), cursor::Show,)?;
            stdout.flush()?;
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    }
}
