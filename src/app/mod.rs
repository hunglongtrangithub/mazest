mod game;
mod visualize;

use crossterm::{QueueableCommand, event::Event, execute};
use std::{
    fmt::Display,
    io::{Stdout, Write},
};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

use crossterm::{
    ExecutableCommand, cursor,
    event::{self, KeyCode},
    queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};

use crate::{generators::Generator, maze::cell::GridCell, solvers::Solver};

/// Available maze generators
const GENERATORS: [Generator; 4] = [
    Generator::RecurBacktrack,
    Generator::Kruskal,
    Generator::Prim,
    Generator::RecurDiv,
];
/// Available maze solvers
const SOLVERS: [Solver; 4] = [Solver::Dfs, Solver::Bfs, Solver::Dijkstra, Solver::AStar];
/// All combinations of maze generators and solvers
const COMBOS: [(Generator, Solver); 16] = {
    let mut combos = [(Generator::RecurBacktrack, Solver::Dfs); 16];
    let mut index = 0;
    let mut gen_index = 0;
    while gen_index < GENERATORS.len() {
        let mut sol_index = 0;
        while sol_index < SOLVERS.len() {
            combos[index] = (GENERATORS[gen_index], SOLVERS[sol_index]);
            index += 1;
            sol_index += 1;
        }
        gen_index += 1;
    }
    combos
};

/// Number of rows reserved at the bottom of the terminal for logging or status messages
const NUM_STATUS_ROWS: u16 = 1;

/// Present a menu of options to the user and let them select one using up/down arrow keys
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
        if let Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
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

/// Calculate max maze size based on terminal size and cell size
/// Ensures the size is odd and at least 3
fn get_max_maze_size(term_size: u16, cell_size: u16) -> u8 {
    // Get default grid dimension based on terminal size. Make sure they are odd and at least 3.
    let max_grid_size = {
        let n = term_size / cell_size;
        if n.is_multiple_of(2) && n > 0 {
            n - 1
        } else {
            n
        }
        .max(3)
    };

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
                get_max_maze_size(term_width, GridCell::CELL_WIDTH)
            } else {
                // Reserve rows for logs
                get_max_maze_size(term_height.saturating_sub(NUM_STATUS_ROWS), 1)
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

    let width = match prompt_with_validation(stdout, "Width: ", validate_width)? {
        Some(w) => w,
        None => return Ok(None),
    };
    stdout.execute(style::PrintStyledContent(
        format!("Width set to {}\r\n", width)
            .with(Color::Green)
            .attribute(Attribute::Bold),
    ))?;

    let height = match prompt_with_validation(stdout, "Height: ", validate_height)? {
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
        // Re-render
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
        if let Event::Key(event::KeyEvent { code, kind, .. }) = event::read()? {
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

/// Wait for a specific key press event from the user
fn wait_for_keypress(key: KeyCode) -> std::io::Result<()> {
    loop {
        if let Event::Key(event::KeyEvent { code, kind, .. }) = event::read()?
            && kind == event::KeyEventKind::Press
            && code == key
        {
            break;
        }
    }
    Ok(())
}

/// Print a line of message below the grid without disrupting the grid display
/// The cursor position is saved and restored after logging
/// Returns `Err` if there was an I/O error
/// If msg is None, clears all messages from the reserved rows
fn log_terminal(
    stdout: &mut impl Write,
    grid_height: u16,
    msg: Option<style::StyledContent<impl Display + AsRef<str>>>,
) -> std::io::Result<()> {
    let term_width = terminal::size()?.0 as usize;
    queue!(
        stdout,
        // Save cursor position first
        cursor::SavePosition,
        // Move cursor to the log line (below the grid)
        cursor::MoveTo(0, grid_height),
        // Clear previous log line
        terminal::Clear(ClearType::CurrentLine),
    )?;
    if let Some(msg) = msg {
        let content = msg.content().as_ref();
        if content.width() > term_width {
            // Truncate message to fit terminal width. Reserve 1 char for '~'
            let (truncated, printed_width) = content.unicode_truncate(term_width.saturating_sub(1));
            stdout.queue(style::PrintStyledContent(style::StyledContent::new(
                *msg.style(),
                truncated,
            )))?;
            // If the remaining space is enough for a '~'
            if term_width - printed_width >= 1 {
                stdout.queue(style::PrintStyledContent("~".stylize()))?;
            }
        } else {
            // Just print the whole thing if it fits the terminal width
            stdout.queue(style::PrintStyledContent(msg))?;
        }
    }
    // Go back to previous cursor position
    stdout.queue(cursor::RestorePosition)?;
    stdout.flush()?;
    Ok(())
}

#[derive(Copy, Clone)]
enum AppMode {
    Visualize,
    Game,
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppMode::Visualize => write!(f, "Visualization Mode"),
            AppMode::Game => write!(f, "Game Mode"),
        }
    }
}

pub struct App {
    stdout: Stdout,
}

impl Default for App {
    fn default() -> Self {
        Self {
            stdout: std::io::stdout(),
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
            std::process::exit(1); // exit immediately after restoring terminal
        }));
    }

    /// Restore terminal to original state
    /// Leave alternate screen and disable raw mode
    fn restore_terminal(stdout: &mut Stdout) -> std::io::Result<()> {
        queue!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
        stdout.flush()?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    /// Setup terminal in raw mode and enter alternate screen
    /// Also sets a panic hook to restore terminal on panic
    fn setup_terminal(&mut self) -> std::io::Result<()> {
        terminal::enable_raw_mode()?;
        App::set_panic_hook();
        crossterm::queue!(
            self.stdout,
            terminal::EnterAlternateScreen,
            terminal::Clear(ClearType::All),
            cursor::Hide,
            cursor::MoveTo(0, 0)
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    /// Entry point to run the application.
    /// Sets up the terminal, runs the app logic, and restores the terminal state on exit.
    pub fn run(&mut self) -> std::io::Result<()> {
        self.setup_terminal()?;
        self.app()?;
        App::restore_terminal(&mut self.stdout)?;
        Ok(())
    }

    /// Main application logic
    fn app(&mut self) -> std::io::Result<()> {
        self.stdout.execute(style::PrintStyledContent(
            "Welcome to Mazest!\r\n"
                .with(Color::Yellow)
                .attribute(Attribute::Bold),
        ))?;

        let mode = match select_from_menu(
            &mut self.stdout,
            "Select app mode (use arrow keys and Enter, or Esc to exit):",
            &[AppMode::Visualize, AppMode::Game],
        )? {
            Some(m) => m,
            None => return Ok(()),
        };

        // Clear screen
        execute!(
            self.stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        match mode {
            AppMode::Visualize => {
                visualize::run(&mut self.stdout)?;
            }
            AppMode::Game => {
                game::run(&mut self.stdout)?;
            }
        }
        Ok(())
    }
}
