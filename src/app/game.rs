use crate::{
    app,
    generators::{Generator, generate_maze},
    maze::Maze,
};
use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::KeyCode,
    queue,
    style::{self, Attribute, Color, Stylize},
};
use std::io::{Stdout, Write};

struct GameState {
    maze: Maze,
}

impl GameState {
    fn new(width: u8, height: u8, generator: Generator) -> Self {
        let mut maze = Maze::new(width, height, None);
        generate_maze(&mut maze, generator, None);
        GameState { maze }
    }

    fn render_initial_maze(&self, stdout: &mut Stdout) -> std::io::Result<()> {
        let grid = self.maze.grid();
        stdout.queue(cursor::MoveTo(0, 0))?;
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                stdout.queue(style::Print(grid[(x, y)]))?;
            }
            stdout.queue(style::Print("\r\n"))?;
        }
        stdout.flush()?;
        Ok(())
    }
}

pub fn run(stdout: &mut Stdout) -> std::io::Result<()> {
    // Ask user for maze dimensions
    let (width, height) = match app::ask_maze_dimensions(stdout)? {
        Some(dims) => dims,
        None => {
            return Ok(());
        }
    };

    // Ask user for maze generation algorithm
    let generator = match app::select_from_menu(
        stdout,
        "Select maze generation algorithm (use arrow keys and Enter, or Esc to exit):",
        &app::GENERATORS,
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

    queue!(
            stdout,
            style::PrintStyledContent(
                "Move your Pacman through the maze using arrow keys to its destination before time's over!\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)
            ),
            style::PrintStyledContent(
                "Controls:\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)
            ),
            style::PrintStyledContent(
                "  ←/→/↑/↓: Step up/down/left/right to control Pacman\r\n".with(Color::Cyan)
            ),
            style::PrintStyledContent("  Esc: Exit game\r\n\r\n".with(Color::Cyan)),
        )?;

    let game_state = GameState::new(width, height, generator);
    game_state.render_initial_maze(stdout)?;
    app::log_terminal(
        stdout,
        game_state.maze.grid().height(),
        Some(
            "Game loop not implemented. Press Enter to exit the game."
                .with(Color::Magenta)
                .attribute(Attribute::Bold),
        ),
    )?;

    // Wait for Enter key to start the game
    app::wait_for_keypress(KeyCode::Enter)?;
    Ok(())
}
