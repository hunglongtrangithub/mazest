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
    pub const PATH: Cell = Cell::Path(PathType::Empty);
    pub const WALL: Cell = Cell::Wall(WallType::Wall);
    pub const MARK: Cell = Cell::Wall(WallType::Mark);
    pub const GOAL: Cell = Cell::Path(PathType::Goal);
    pub const START: Cell = Cell::Path(PathType::Start);
    pub const VISITED: Cell = Cell::Path(PathType::Visited);
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
