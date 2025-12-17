use crossterm::style::{Color, Stylize};

use std::fmt;

use crate::maze::Orientation;

/// Represents a cell in the grid, which can be either a path or a wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridCell {
    Path(PathType),
    Wall(WallType),
}

impl GridCell {
    pub const EMPTY: GridCell = GridCell::Path(PathType::Empty);
    pub const WALL: GridCell = GridCell::Wall(WallType::Wall);
    pub const MARK: GridCell = GridCell::Wall(WallType::Mark);
    pub const GOAL: GridCell = GridCell::Path(PathType::Goal);
    pub const START: GridCell = GridCell::Path(PathType::Start);
    pub const VISITED: GridCell = GridCell::Path(PathType::Visited);
    pub const PACMAN: GridCell = GridCell::Path(PathType::Pacman);
    /// The width of each cell when rendered, in character widths.
    pub const CELL_WIDTH: u16 = 2;
}

/// Represents different types of path cells in the maze.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum PathType {
    /// Marks a cell as part of the solution route, with the given orientation.
    Route(Orientation),
    /// An empty cell, not part of any route or visited path.
    #[default]
    Empty,
    /// A cell that has been visited during maze traversal or solving.
    Visited,
    /// The starting cell of the maze.
    Start,
    /// The goal or ending cell of the maze.
    Goal,
    /// Pacman cell
    Pacman,
    /// Ghost cell
    Ghost,
}

/// Represents different types of wall cells in the maze.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum WallType {
    #[default]
    Wall,
    Mark,
}

impl fmt::Display for GridCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let styled_symbol = match self {
            GridCell::Path(path) => match path {
                PathType::Route(orientation) => match orientation {
                    Orientation::Horizontal => "".with(Color::Yellow),
                    Orientation::Vertical => " ".with(Color::Yellow),
                    // Orientation::Horizontal => "🟨".with(Color::Yellow),
                    // Orientation::Vertical => "🟨".with(Color::Yellow),
                },
                PathType::Empty => "  ".with(Color::Reset),
                // PathType::Visited => "* ".with(Color::Blue),
                PathType::Visited => "* ".with(Color::Blue),
                PathType::Start => "🟩".with(Color::Green),
                PathType::Goal => "🟥".with(Color::Red),
                PathType::Pacman => "🟡".with(Color::Yellow),
                PathType::Ghost => "👻".with(Color::Cyan),
            },
            GridCell::Wall(wall) => match wall {
                WallType::Wall => "⬜".with(Color::White),
                WallType::Mark => "🟪".with(Color::Magenta),
            },
        };

        #[cfg(debug_assertions)]
        {
            use unicode_width::UnicodeWidthStr;
            assert_eq!(
                styled_symbol.content().width(),
                GridCell::CELL_WIDTH as usize,
                "Each cell must occupy exactly two character widths."
            );
        }

        write!(f, "{}", styled_symbol)
    }
}
