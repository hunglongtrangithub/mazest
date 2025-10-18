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
    execute, queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};

use crate::maze::{cell::GridCell, grid::GridEvent};
use crate::{
    generators::{Generator, generate_maze},
    maze, solvers,
};

enum InputEvent {
    KeyPress(event::KeyEvent),
    Resize(u16, u16),
}

pub struct App {
    /// Interval at which to render the event buffer
    render_interval: Duration,
    /// Time taken to render each grid update when grid size is u8::MAX
    render_refresh_rate: Duration,
}

impl Default for App {
    fn default() -> Self {
        Self {
            render_interval: Duration::from_millis(1000),
            render_refresh_rate: Duration::from_micros(20),
        }
    }
}

impl App {
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
        execute!(stdout, terminal::EnterAlternateScreen)?;
        crossterm::execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::Hide,
            cursor::MoveTo(0, 0)
        )?;
        Ok(())
    }

    /// Restore terminal to original state
    /// Leave alternate screen and disable raw mode
    pub fn restore_terminal(stdout: &mut Stdout) -> std::io::Result<()> {
        execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
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
            execute!(stdout, style::PrintStyledContent(
                "Terminal size is too small for the maze dimensions to display. Please resize the terminal.\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)),
                style::PrintStyledContent("Press Esc to exit...\r\n".with(Color::Blue).attribute(Attribute::Bold))
            )?;
            // Wait for user to press Esc
            App::wait_for_esc()?;
            return Ok(());
        }

        // Ask user for maze generation algorithm
        let generator = match App::select_from_menu(
            stdout,
            "Select maze generation algorithm (use arrow keys and Enter, or Esc to exit):",
            &[
                Generator::RecurBacktrack,
                Generator::Kruskal,
                Generator::Prim,
                Generator::RecurDiv,
            ],
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
        let solver = match App::select_from_menu(
            stdout,
            "Select maze solving algorithm (use arrow keys and Enter, or Esc to exit):",
            &[
                solvers::Solver::Dfs,
                solvers::Solver::Bfs,
                solvers::Solver::Dijkstra,
                solvers::Solver::AStar,
            ],
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
            "Loop maze generation and solving? (use arrow keys and Enter, or Esc to exit):",
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

        let (input_event_tx, input_event_rx) = std::sync::mpsc::channel::<InputEvent>();
        let render_done_for_input = render_done.clone();
        // Spawn a thread to listen for user input
        let input_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
            App::listen_to_user_input(input_event_tx, &render_done_for_input)
        });

        let (grid_event_tx, grid_event_rx) = std::sync::mpsc::channel::<GridEvent>();

        // Spawn a thread to listen for grid updates and render the maze
        let render_refresh_time = self.calculate_render_refresh_time(width, height);
        let render_interval = self.render_interval;
        let render_cancel_for_render = render_cancel.clone();
        let render_done_for_render = render_done.clone();
        let render_thread_handle = std::thread::spawn(move || {
            App::render(
                render_interval,
                grid_event_rx,
                render_refresh_time,
                &render_cancel_for_render,
                &render_done_for_render,
            )
        });

        // Spawn a thread to generate maze and solve it
        let render_cancel_for_compute = render_cancel.clone();
        let compute_thread_handle = std::thread::spawn(move || -> bool {
            if !loop_animation {
                return App::compute(width, height, grid_event_tx, generator, solver);
            }
            loop {
                let goal_reached =
                    App::compute(width, height, grid_event_tx.clone(), generator, solver);
                // Check if rendering was cancelled
                if render_cancel_for_compute.load(std::sync::atomic::Ordering::Relaxed) {
                    return goal_reached;
                }
            }
        });

        // Listen for user input to cancel rendering
        loop {
            match input_event_rx.try_recv() {
                Err(e) => {
                    match e {
                        std::sync::mpsc::TryRecvError::Empty => {
                            // No input, check if render is done
                            if render_done.load(std::sync::atomic::Ordering::Relaxed) {
                                // Render is done, break the loop
                                drop(input_event_rx);
                                break;
                            }
                            // Sleep a bit before trying again to avoid busy polling from
                            // input_event_rx (avg typing speed is much slower than this)
                            std::thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        std::sync::mpsc::TryRecvError::Disconnected => {
                            // Input thread has exited, break the loop
                            break;
                        }
                    }
                }
                Ok(event) => match event {
                    InputEvent::KeyPress(key_event) => {
                        if key_event.code == KeyCode::Esc {
                            render_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                            // Close the channel to signal input thread to exit (if not already)
                            drop(input_event_rx);
                            break;
                        }
                    }
                    InputEvent::Resize(_width, _height) => {
                        // Ignore resize events for now
                        // TODO: Send terminal resize events to the render thread
                    }
                },
            }
        }

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

    /// Listen for user input events (key presses and resize)
    /// Returns the thread handle and the receiver for input events
    fn listen_to_user_input(
        input_event_tx: Sender<InputEvent>,
        render_done: &AtomicBool,
    ) -> std::io::Result<()> {
        loop {
            // Check if render is done
            if render_done.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            // Poll for events with a timeout
            if !event::poll(Duration::from_millis(100))? {
                // No event available, continue loop to check render_done again
                continue;
            }

            // Read the next event
            // We only care about key presses and resize events
            let input_event = match event::read()? {
                event::Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                    InputEvent::KeyPress(key_event)
                }
                event::Event::Resize(width, height) => InputEvent::Resize(width, height),
                _ => continue,
            };
            // Should exit input thread on Esc key
            let should_exit = matches!(
                input_event,
                InputEvent::KeyPress(event::KeyEvent {
                    code: KeyCode::Esc,
                    ..
                })
            );
            if input_event_tx.send(input_event).is_err() {
                // Receiver has been dropped, exit the thread
                return Ok(());
            }
            if should_exit {
                return Ok(());
            }
        }
    }

    /// Generate and solve the maze
    /// Returns whether the goal was reached
    fn compute(
        width: u8,
        height: u8,
        grid_event_tx: Sender<GridEvent>,
        generator: Generator,
        solver: solvers::Solver,
    ) -> bool {
        let mut maze = maze::Maze::new(width, height, Some(grid_event_tx));
        // Generate the maze using the selected algorithm
        generate_maze(&mut maze, generator, None);

        // Solve the maze using the selected algorithm
        solvers::solve_maze(&mut maze, solver)
        // Maze is dropped here, which will close the grid event channel
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
        execute!(stdout, cursor::Hide, cursor::SavePosition)?;

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

            execute!(stdout, style::Print(&input), style::ResetColor)?;

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
        execute!(
            stdout,
            cursor::RestorePosition,
            terminal::Clear(ClearType::FromCursorDown),
            cursor::Show
        )?;

        Ok(number_option)
    }

    /// Ask user for maze dimensions (width and height between 1 and 255)
    /// Returns None if user cancels input with Esc
    /// Returns Some((width, height)) if user inputs valid dimensions
    fn ask_maze_dimensions(stdout: &mut Stdout) -> std::io::Result<Option<(u8, u8)>> {
        stdout.execute(style::PrintStyledContent(
            "Enter maze dimensions (width and height between 1 and 255), or press Esc to exit. Default values are based on terminal size.\r\n"
                .with(Color::Blue),
        ))?;

        let validate = |s: &str, default_size: u8| {
            if s.trim().is_empty() {
                return Ok(default_size);
            }
            s.parse::<u8>()
                .map_err(|_| "Please enter a number between 1 and 255".to_string())
                .and_then(|n| match n {
                    1..=255 => Ok(n),
                    _ => Err("Number must be between 1 and 255".to_string()),
                })
        };

        let (term_width, term_height) = terminal::size()?;

        // Get default grid dimensions based on terminal size. Make sure they are odd and at least 3.
        let odd_and_min_3 = |n: u16| if n % 2 == 0 { n - 1 } else { n }.max(3);
        let (default_grid_width, default_grid_height) = (
            odd_and_min_3(term_width / GridCell::CELL_WIDTH),
            odd_and_min_3(term_height),
        );

        // Default maze dimensions are half the grid dimensions, capped at u8::MAX
        let (default_maze_width, default_maze_height) = (
            (default_grid_width / 2).min(u8::MAX as u16) as u8,
            (default_grid_height / 2).min(u8::MAX as u16) as u8,
        );

        // Validation closures based on default sizes
        let validate_width = |s: &str| validate(s, default_maze_width);
        let validate_height = |s: &str| validate(s, default_maze_height);

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
                match code {
                    KeyCode::Up => {
                        selected = match selected {
                            0 => options.len() - 1,
                            _ => selected - 1,
                        };
                    }
                    KeyCode::Down if kind == event::KeyEventKind::Press => {
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
        execute!(
            stdout,
            cursor::RestorePosition,
            terminal::Clear(ClearType::FromCursorDown),
            cursor::Show
        )?;

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
    fn check_resize(stdout: &mut Stdout, width: u16, height: u16) -> std::io::Result<bool> {
        let (term_width, term_height) = terminal::size()?;
        if term_width < width * GridCell::CELL_WIDTH || term_height < height {
            let msg = format!(
                "Terminal size is too small ({}x{}) for the grid dimensions ({}x{}) to display. Please resize the terminal.\r\n",
                width * GridCell::CELL_WIDTH,
                height,
                width,
                height
            );
            execute!(
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
            App::wait_for_esc()?;
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
    ) -> std::io::Result<bool> {
        for event in event_buffer.drain(..) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(false);
            }
            // print!("Last rendered event: {:?}\r\n", event);
            match event {
                GridEvent::Initial {
                    cell,
                    width,
                    height,
                } => {
                    *grid_dims = Some((width, height));
                    if !App::check_resize(stdout, width, height)? {
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
                        if !App::check_resize(stdout, *width, *height)? {
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
        receiver: Receiver<GridEvent>,
        render_refresh_time: Duration,
        cancel: &AtomicBool,
        done: &AtomicBool,
    ) -> std::io::Result<bool> {
        let mut stdout = std::io::stdout();
        let mut event_buffer = Vec::new();
        let mut last_render = std::time::Instant::now();
        let mut grid_dims = None;

        execute!(stdout, terminal::Clear(ClearType::All), cursor::Hide,)?;
        loop {
            // Block and wait for the next event
            match receiver.recv() {
                Err(_e) => {
                    // Channel disconnected, render the remaining buffer and exit
                    if !App::process_events(
                        &mut event_buffer,
                        &mut stdout,
                        &mut grid_dims,
                        render_refresh_time,
                        cancel,
                    )? {
                        // Cancelled
                        return Ok(false);
                    }
                    break;
                }
                Ok(event) => {
                    event_buffer.push(event);
                    if last_render.elapsed() >= render_interval {
                        // Reset the timer
                        last_render = std::time::Instant::now();
                        // Render all buffered events
                        if !App::process_events(
                            &mut event_buffer,
                            &mut stdout,
                            &mut grid_dims,
                            render_refresh_time,
                            cancel,
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
            execute!(stdout, cursor::MoveTo(0, height), cursor::Show,)?;
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    }
}
