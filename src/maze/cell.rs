use crossterm::style::{Color, Stylize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
/// Represents a cell in the maze, which can be either a path or a wall.
pub enum Cell {
    Path(PathType),
    Wall(WallType),
}

#[derive(Debug, Clone, PartialEq)]
/// Represents different types of path cells in the maze.
pub enum PathType {
    Empty,
    Visited,
    Start,
    Goal,
}

#[derive(Debug, Clone, PartialEq)]
/// Represents different types of wall cells in the maze.
pub enum WallType {
    Wall,
    Mark,
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let styled_symbol = match self {
            Cell::Path(path) => match path {
                PathType::Empty => "  ".with(Color::Reset),
                PathType::Visited => ". ".with(Color::Blue),
                PathType::Start => "S ".with(Color::Green),
                PathType::Goal => "G ".with(Color::Red),
            },
            Cell::Wall(wall) => match wall {
                WallType::Wall => "⬜".with(Color::White),
                WallType::Mark => "🟪".with(Color::Magenta),
            },
        };
        write!(f, "{}", styled_symbol)
    }
}
