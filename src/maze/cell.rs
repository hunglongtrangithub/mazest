use crossterm::style::{Color, Stylize};
use unicode_width::UnicodeWidthStr;

use std::fmt;

use crate::maze::Orientation;

/// Represents a cell in the grid, which can be either a path or a wall.
#[derive(Debug, Clone, PartialEq)]
pub enum GridCell {
    Path(PathType),
    Wall(WallType),
}

impl GridCell {
    pub const PATH: GridCell = GridCell::Path(PathType::Empty);
    pub const WALL: GridCell = GridCell::Wall(WallType::Wall);
    pub const MARK: GridCell = GridCell::Wall(WallType::Mark);
    pub const GOAL: GridCell = GridCell::Path(PathType::Goal);
    pub const START: GridCell = GridCell::Path(PathType::Start);
    pub const VISITED: GridCell = GridCell::Path(PathType::Visited);
}

/// Represents different types of path cells in the maze.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum PathType {
    Path(Orientation),
    #[default]
    Empty,
    Visited,
    Start,
    Goal,
}

/// Represents different types of wall cells in the maze.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum WallType {
    #[default]
    Wall,
    Mark,
}

impl fmt::Display for GridCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let styled_symbol = match self {
            GridCell::Path(path) => match path {
                PathType::Path(orientation) => match orientation {
                    Orientation::Horizontal => "══".with(Color::Yellow),
                    Orientation::Vertical => "║ ".with(Color::Yellow),
                },
                PathType::Empty => "  ".with(Color::Reset),
                PathType::Visited => "* ".with(Color::Blue),
                PathType::Start => "🟩".with(Color::Green),
                PathType::Goal => "🟥".with(Color::Red),
            },
            GridCell::Wall(wall) => match wall {
                WallType::Wall => "⬜".with(Color::White),
                WallType::Mark => "🟪".with(Color::Magenta),
            },
        };
        assert_eq!(
            styled_symbol.content().width(),
            2,
            "Each cell must occupy exactly two character widths."
        );
        write!(f, "{}", styled_symbol)
    }
}
