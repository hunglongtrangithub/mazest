use crossterm::style::{Color, Stylize};
use std::fmt;

use crate::maze::Orientation;

#[derive(Debug, Clone, PartialEq)]
/// Represents a cell in the maze, which can be either a path or a wall.
pub enum Cell {
    Path(PathType),
    Wall(WallType),
}

impl Cell {
    /// Checks if the cell is a path type.
    pub fn is_path(&self) -> bool {
        matches!(self, Cell::Path(_))
    }

    /// Checks if the cell is a wall type.
    pub fn is_wall(&self) -> bool {
        matches!(self, Cell::Wall(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Represents different types of path cells in the maze.
#[derive(Default)]
pub enum PathType {
    Path(Orientation),
    #[default]
    Empty,
    Visited,
    Start,
    Goal,
}

#[derive(Debug, Clone, PartialEq)]
/// Represents different types of wall cells in the maze.
#[derive(Default)]
pub enum WallType {
    #[default]
    Wall,
    Mark,
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let styled_symbol = match self {
            Cell::Path(path) => match path {
                PathType::Path(orientation) => match orientation {
                    Orientation::Horizontal => "══".with(Color::Yellow),
                    Orientation::Vertical => "║ ".with(Color::Yellow),
                },
                PathType::Empty => "  ".with(Color::Reset),
                PathType::Visited => "* ".with(Color::Blue),
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
