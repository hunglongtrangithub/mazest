mod generators;
mod maze;
mod solvers;

use std::{
    io::{Read, Write},
    sync::mpsc::Receiver,
    time::Duration,
};

use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, KeyCode},
    execute, queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};

use crate::maze::GridCell;

#[derive(Debug)]
enum GridEvent {
    Initial {
        cell: GridCell,
        width: u16,
        height: u16,
    },
    Update {
        coord: (u16, u16),
        old: GridCell,
        new: GridCell,
    },
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
    pub fn run(&self) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        App::setup_terminal(&mut stdout)?;

        // Ask user for grid dimensions
        let (width, height) = match App::ask_grid_dimensions(&mut stdout)? {
            Some(dims) => dims,
            None => {
                println!("Input cancelled. Exiting.");
                App::restore_terminal(&mut stdout)?;
                return Ok(());
            }
        };
        // Check if terminal height and width are sufficient
        let (term_width, term_height) = terminal::size()?;
        if term_width < width as u16 * GridCell::CELL_WIDTH || term_height < height as u16 {
            print!(
                "Terminal size is too small for the maze dimensions to display. Please resize the terminal.\r\n"
            );
            stdout.flush()?;
            App::restore_terminal(&mut stdout)?;
            return Ok(());
        }

        // Ask user for maze generation algorithm
        let generator = match App::select_from_menu(
            &mut stdout,
            "Select maze generation algorithm (use arrow keys and Enter, or Esc to exit):",
            &[
                generators::Generator::RecurBacktrack,
                generators::Generator::Kruskal,
                generators::Generator::Prim,
                generators::Generator::RecurDiv,
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
                println!("Input cancelled. Exiting.");
                App::restore_terminal(&mut stdout)?;
                return Ok(());
            }
        };

        // Ask user for maze solving algorithm
        let solver = match App::select_from_menu(
            &mut stdout,
            "Select maze solving algorithm (use arrow keys and Enter):",
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
                println!("Input cancelled. Exiting.");
                App::restore_terminal(&mut stdout)?;
                return Ok(());
            }
        };

        self.spawn_compute_and_render_threads(width, height, generator, solver)?;
        App::restore_terminal(&mut stdout)?;
        Ok(())
    }

    /// Get user input with real-time validation and feedback
    /// Returns None if user cancels input with Esc
    /// Returns Some(T) if user inputs a valid input and presses Enter, where T is the validated type
    fn prompt_with_validation<F, T>(
        stdout: &mut std::io::Stdout,
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
            execute!(
                stdout,
                cursor::RestorePosition,
                terminal::Clear(ClearType::FromCursorDown)
            )?;

            // Print prompt
            stdout.execute(style::PrintStyledContent(
                prompt.with(Color::Cyan).attribute(Attribute::Bold),
            ))?;

            // Decide color based on validity
            let validation_result = validate(&input);
            match validation_result {
                Ok(_) => {
                    stdout.execute(style::SetForegroundColor(Color::Green))?;
                }
                Err(_) => {
                    stdout.execute(style::SetForegroundColor(Color::Red))?;
                }
            }

            execute!(stdout, style::Print(&input), style::ResetColor)?;

            // Print a space after input so cursor is visible
            stdout.execute(style::Print(" \r\n"))?;

            // Error message line (if any)
            if let Err(msg) = validation_result {
                stdout.execute(style::PrintStyledContent(
                    msg.with(Color::DarkGrey).attribute(Attribute::Dim),
                ))?;
            }

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
                        input.push(c);
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

    /// Ask user for grid dimensions (width and height between 1 and 255)
    /// Returns None if user cancels input with Esc
    /// Returns Some((width, height)) if user inputs valid dimensions
    fn ask_grid_dimensions(stdout: &mut std::io::Stdout) -> std::io::Result<Option<(u8, u8)>> {
        stdout.execute(style::PrintStyledContent(
            "Enter maze dimensions (width and height between 1 and 255), or press Esc to exit:\r\n"
                .with(Color::Blue),
        ))?;

        let validate = |s: &str| {
            s.parse::<u8>()
                .map_err(|_| "Please enter a number between 1 and 255".to_string())
                .and_then(|n| match n {
                    1..=255 => Ok(n),
                    _ => Err("Number must be between 1 and 255".to_string()),
                })
        };

        let width = match App::prompt_with_validation(stdout, "Width: ", validate)? {
            Some(w) => w,
            None => return Ok(None),
        };
        stdout.execute(style::PrintStyledContent(
            format!("Width set to {}\r\n", width)
                .with(Color::Green)
                .attribute(Attribute::Bold),
        ))?;

        let height = match App::prompt_with_validation(stdout, "Height: ", validate)? {
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
        stdout: &mut std::io::Stdout,
        prompt: &str,
        options: &[T],
    ) -> std::io::Result<Option<T>> {
        // Save cursor position so we can restore / redraw
        execute!(stdout, cursor::Hide, cursor::SavePosition)?;

        let mut selected = 0;

        let selected_option = loop {
            // Re-render prompt line
            execute!(
                stdout,
                cursor::RestorePosition,
                terminal::Clear(ClearType::FromCursorDown)
            )?;

            // Print prompt
            stdout.execute(style::PrintStyledContent(prompt.with(Color::Blue)))?;

            // Print options
            for (i, option) in options.iter().enumerate() {
                if i == selected {
                    stdout.execute(style::SetAttribute(Attribute::Reverse))?;
                }
                stdout.execute(style::Print(format!("\r\n{}", option)))?;
                if i == selected {
                    stdout.execute(style::SetAttribute(Attribute::NoReverse))?;
                }
            }
            stdout.execute(style::Print("\r\n"))?;
            stdout.flush()?;

            // Wait for key event
            if let event::Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
                match code {
                    KeyCode::Up if selected > 0 => {
                        selected -= 1;
                    }
                    KeyCode::Up => {}
                    KeyCode::Down if kind == event::KeyEventKind::Press => {
                        if selected < options.len() - 1 {
                            selected += 1;
                        }
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

    fn set_panic_hook() {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = App::restore_terminal(&mut std::io::stdout()); // ignore any errors as we are already failing
            hook(panic_info);
        }));
    }

    fn setup_terminal(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
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

    fn restore_terminal(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
        execute!(stdout, terminal::LeaveAlternateScreen)?;
        crossterm::execute!(stdout, cursor::Show)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    fn calculate_render_refresh_time(&self, grid_width: u8, grid_height: u8) -> Duration {
        let size = grid_width.max(grid_height) as usize;
        self.render_refresh_rate * u8::MAX as u32 / size as u32
    }

    fn spawn_compute_and_render_threads(
        &self,
        width: u8,
        height: u8,
        generator: generators::Generator,
        solver: solvers::Solver,
    ) -> std::io::Result<()> {
        let (grid_event_tx, grid_event_rx) = std::sync::mpsc::channel::<GridEvent>();
        let mut maze = maze::Maze::new(width, height, Some(grid_event_tx));

        // Spawn a thread to listen for grid updates and render the maze
        let render_refresh_time = self.calculate_render_refresh_time(width, height);
        let render_interval = self.render_interval;
        let render_thread_handle = std::thread::spawn(move || {
            App::render_loop(render_interval, grid_event_rx, render_refresh_time)
        });

        // Generate the maze using the selected algorithm
        generators::generate_maze(&mut maze, generator, None);

        // Solve the maze using the selected algorithm
        let goal_reached = solvers::solve_maze(&mut maze, solver);

        drop(maze); // Ensure maze is dropped and sender is closed

        // Wait for render thread to finish - ignore any errors
        render_thread_handle.join().ok();

        let mut stdout = std::io::stdout();
        if goal_reached {
            print!("Maze solved! Goal reached.\r\n");
        } else {
            print!("No path found to the goal.\r\n");
        }
        print!("Press Enter to exit...\r\n");
        stdout.flush()?;
        let mut buf = [0u8; 1];
        while std::io::stdin().read(&mut buf)? == 1 {
            if buf[0] == b'\r' {
                break;
            }
        }
        Ok(())
    }

    fn process_events(
        event_buffer: &mut Vec<GridEvent>,
        stdout: &mut std::io::Stdout,
        grid_dims: &mut Option<(u16, u16)>,
        render_refresh_time: Duration,
    ) -> std::io::Result<()> {
        let resize_msg = "Terminal size is too small for the maze dimensions to display. Please resize the terminal.";
        for event in event_buffer.drain(..) {
            // print!("Last rendered event: {:?}\r\n", event);
            match event {
                GridEvent::Initial {
                    cell,
                    width,
                    height,
                } => {
                    *grid_dims = Some((width, height));
                    // Check if terminal height and width are sufficient
                    let (term_width, term_height) = terminal::size()?;
                    if term_width < width * GridCell::CELL_WIDTH || term_height < height {
                        print!("{}\r\n", resize_msg);
                        stdout.flush()?;
                        return Ok(());
                    }
                    // Clear screen
                    // Move to top-left corner
                    // Print the whole grid with the specified cell

                    stdout.queue(crossterm::cursor::MoveTo(0, 0))?;
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
                        // Move the cursor to the specified coordinate and print the
                        // new cell using the grid dimensions

                        let (term_width, term_height) = terminal::size()?;
                        if term_width < *width * GridCell::CELL_WIDTH || term_height < *height {
                            print!("{}\r\n", resize_msg);
                            stdout.flush()?;
                            return Ok(());
                        }
                        stdout.queue(cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1))?;
                        stdout.queue(style::Print(new))?;
                        stdout.flush()?;
                    }
                    // Skip if width and height are not set
                    None => continue,
                },
            }
            // Sleep a bit to simulate rendering time
            std::thread::sleep(render_refresh_time);
        }
        Ok(())
    }

    fn render_loop(
        render_interval: Duration,
        receiver: Receiver<GridEvent>,
        render_refresh_time: Duration,
    ) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        let mut event_buffer = Vec::new();
        let mut last_render = std::time::Instant::now();
        let mut grid_dims = None;

        loop {
            // Block and wait for the next event
            match receiver.recv() {
                Err(_e) => {
                    // Channel disconnected, render the remaining buffer and exit
                    App::process_events(
                        &mut event_buffer,
                        &mut stdout,
                        &mut grid_dims,
                        render_refresh_time,
                    )?;
                    break;
                }
                Ok(event) => {
                    event_buffer.push(event);
                    if last_render.elapsed() >= render_interval {
                        // Reset the timer
                        last_render = std::time::Instant::now();
                        // Render all buffered events
                        App::process_events(
                            &mut event_buffer,
                            &mut stdout,
                            &mut grid_dims,
                            render_refresh_time,
                        )?;
                    }
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = grid_dims {
            queue!(stdout, crossterm::cursor::MoveTo(0, height))?;
            stdout.flush()?;
        }
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let app = App::default();
    app.run()
}
