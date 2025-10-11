pub mod cell;
mod grid;

use crossterm::execute;

pub use cell::{Cell, PathType};
use grid::Grid;

#[derive(Debug, Clone, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

pub struct Maze {
    grid: Grid,
    width: u8,
    height: u8,
}

impl Maze {
    /// Creates a new maze with the given width and height.
    /// The maze is initialized with walls, and the internal grid is sized to accommodate walls between cells.
    pub fn new(width: u8, height: u8) -> Self {
        // n cells in each dimension -> n + 1 walls -> 2n + 1 total
        let grid_height = height as u16 * 2 + 1;
        let grid_width = width as u16 * 2 + 1;
        let mut maze = Maze {
            grid: Grid::new(grid_width, grid_height, Cell::WALL),
            width,
            height,
        };
        (0..height).for_each(|y| {
            (0..width).for_each(|x| {
                maze[(x, y)] = Cell::PATH;
            });
        });
        maze
    }

    #[cfg(test)]
    /// Returns a reference to the internal grid data for testing purposes.
    pub fn grid(&self) -> &[Cell] {
        &self.grid.data
    }

    /// Returns the height of the maze in cells.
    pub fn height(&self) -> u8 {
        self.height
    }
    /// Returns the width of the maze in cells.
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Checks if the maze is empty (zero width and height).
    pub fn is_empty(&self) -> bool {
        self.width == 0 && self.height == 0
    }

    /// Renders the maze to the terminal.
    pub fn render(&self) -> std::io::Result<()> {
        execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0),
        )?;
        self.grid.display();
        match std::env::var("DEBUG") {
            Ok(val) if val == "1" => {
                println!("Press Enter to continue...");
                // Wait for Enter key press
                std::io::stdin().read_line(&mut String::new())?;
            }
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Ok(())
    }

    /// Checks if the given coordinate is within the bounds of the maze.
    pub fn is_in_bounds(&self, coord: (u8, u8)) -> bool {
        coord.0 < self.width && coord.1 < self.height
    }

    /// Removes the wall adjacent to the given cell in the specified direction.
    ///
    /// # Arguments
    /// * `from` - The cell coordinate (x, y) to remove a wall from
    /// * `orientation` - The orientation of the wall to remove:
    ///   - `Vertical`: Removes the wall to the right of the cell (vertical wall between `from` and `(from.0+1, from.1)`)
    ///   - `Horizontal`: Removes the wall below the cell (horizontal wall between `from` and `(from.0, from.1+1)`)
    ///
    /// # Returns
    /// `true` if a wall was removed, `false` if no wall existed at that position
    ///
    /// # Panics
    /// * If `from` is out of bounds
    /// * If `from` is in the rightmost column and `orientation` is `Vertical`
    /// * If `from` is in the bottommost row and `orientation` is `Horizontal`
    pub fn remove_wall_cell_after(&mut self, from: (u8, u8), orientation: Orientation) -> bool {
        if !self.is_in_bounds(from) {
            panic!("The given coordinate is out of bounds");
        }
        let wall_coord = match orientation {
            Orientation::Horizontal => {
                if from.1 + 1 >= self.height {
                    panic!("Cannot remove wall after the bottommost cell");
                }
                (from.0 as u16 * 2 + 1, from.1 as u16 * 2 + 2)
            }
            Orientation::Vertical => {
                if from.0 + 1 >= self.width {
                    panic!("Cannot remove wall after the rightmost cell");
                }
                (from.0 as u16 * 2 + 2, from.1 as u16 * 2 + 1)
            }
        };
        if matches!(self.grid[wall_coord], Cell::Wall(_)) {
            self.grid[wall_coord] = Cell::PATH;
            true
        } else {
            false
        }
    }

    /// Inserts a line of walls after the specified row or column, within a given range.
    ///
    /// This function creates a wall line that spans from `start` to `end` (inclusive) in the
    /// perpendicular direction to the wall orientation.
    ///
    /// # Arguments
    /// * `from` - The row or column index to insert walls after
    /// * `start` - The starting cell index for the wall line (inclusive)
    /// * `end` - The ending cell index for the wall line (inclusive)
    /// * `orientation` - Determines which type of wall line to insert:
    ///   - `Horizontal`: Inserts a horizontal wall line after row `from` (between rows `from` and `from+1`),
    ///     spanning from column `start` to column `end`
    ///   - `Vertical`: Inserts a vertical wall line after column `from` (between columns `from` and `from+1`),
    ///     spanning from row `start` to row `end`
    ///
    /// # Panics
    /// * If `from >= height - 1` and `orientation` is `Horizontal` (no row below to separate)
    /// * If `from >= width - 1` and `orientation` is `Vertical` (no column to the right to separate)
    /// * If `start` or `end` is out of bounds (>= width for Horizontal, >= height for Vertical)
    ///
    pub fn insert_wall_line_after(
        &mut self,
        from: u8,
        start: u8,
        end: u8,
        orientation: Orientation,
    ) {
        match orientation {
            Orientation::Horizontal => {
                if from + 1 >= self.height {
                    panic!("Cannot insert wall line after the bottommost row");
                }
                if start >= self.width || end >= self.width {
                    panic!(
                        "The range for inserting walls (start={}, end={}) is out of bounds",
                        start, end
                    );
                }
                let y_wall = from as u16 * 2 + 2;
                let start = start as u16 * 2 + 1;
                let end = end as u16 * 2 + 1;
                (start..=end).for_each(|x| {
                    self.grid[(x, y_wall)] = Cell::WALL;
                });
            }
            Orientation::Vertical => {
                if from + 1 >= self.width {
                    panic!("Cannot insert wall line after the rightmost column");
                }
                if start >= self.height || end >= self.height {
                    panic!(
                        "The range for inserting walls (start={}, end={}) is out of bounds",
                        start, end
                    );
                }
                let x_wall = from as u16 * 2 + 2;
                let start = start as u16 * 2 + 1;
                let end = end as u16 * 2 + 1;
                (start..=end).for_each(|y| {
                    self.grid[(x_wall, y)] = Cell::WALL;
                });
            }
        }
    }

    /// Checks if there is a wall cell after the specified cell in the given orientation.
    /// `orientation` determines the orientation of the wall to check:
    /// - `Horizontal`: Checks the wall cell below the specified cell (between `from` and `(from.0, from.1+1)`)
    /// - `Vertical`: Checks the wall cell to the right of the specified cell (between `from` and `(from.0+1, from.1)`)
    ///
    pub fn is_wall_cell_after(&self, from: (u8, u8), orientation: Orientation) -> bool {
        if !self.is_in_bounds(from) {
            panic!("The given coordinate is out of bounds");
        }
        let wall_coord = match orientation {
            Orientation::Horizontal => {
                if from.1 + 1 >= self.height {
                    panic!("Cannot check wall after the bottommost cell");
                }
                (from.0 as u16 * 2 + 1, from.1 as u16 * 2 + 2)
            }
            Orientation::Vertical => {
                if from.0 + 1 >= self.width {
                    panic!("Cannot check wall after the rightmost cell");
                }
                (from.0 as u16 * 2 + 2, from.1 as u16 * 2 + 1)
            }
        };
        matches!(self.grid[wall_coord], Cell::Wall(_))
    }

    /// Set the wall cell after the specified cell in the given orientation to be a path (removing the wall).
    /// `orientation` determines the orientation of the path to set:
    /// - `Vertical`: Sets the path cell below the specified cell (between `from` and `(from.0, from.1+1)`)
    /// - `Horizontal`: Sets the path cell to the right of the specified cell (between `from` and `(from.0+1, from.1)`)
    pub fn set_path_cell_after(&mut self, from: (u8, u8), orientation: Orientation) {
        let wall_coord = match orientation {
            Orientation::Horizontal => {
                if from.0 + 1 >= self.width {
                    panic!("Cannot set path cell after the rightmost cell");
                }
                (from.0 as u16 * 2 + 2, from.1 as u16 * 2 + 1)
            }
            Orientation::Vertical => {
                if from.1 + 1 >= self.height {
                    panic!("Cannot set path cell after the bottommost cell");
                }
                (from.0 as u16 * 2 + 1, from.1 as u16 * 2 + 2)
            }
        };
        self.grid[wall_coord] = Cell::Path(PathType::Path(orientation));
    }

    /// Clears all existing walls within the maze. Boundary walls are preserved.
    pub fn clear_walls(&mut self) {
        (0..self.grid.height()).for_each(|y| {
            (0..self.grid.width()).for_each(|x| {
                // Ignore boundary walls
                if self.grid.is_boundary(x, y) {
                    return;
                }
                self.grid[(x, y)] = Cell::PATH;
            });
        });
    }

    /// Fills all empty paths within the maze with walls. Boundary walls are preserved.
    pub fn fill_walls(&mut self) {
        (0..self.grid.height()).for_each(|y| {
            (0..self.grid.width()).for_each(|x| {
                // Ignore boundary walls
                if self.grid.is_boundary(x, y) {
                    return;
                }
                self.grid[(x, y)] = Cell::WALL;
            });
        });
    }
}

impl std::ops::Index<(u8, u8)> for Maze {
    type Output = Cell;

    fn index(&self, index: (u8, u8)) -> &Self::Output {
        let grid_index = (index.0 as u16 * 2 + 1, index.1 as u16 * 2 + 1);
        &self.grid[grid_index]
    }
}

impl std::ops::IndexMut<(u8, u8)> for Maze {
    fn index_mut(&mut self, index: (u8, u8)) -> &mut Self::Output {
        let grid_index = (index.0 as u16 * 2 + 1, index.1 as u16 * 2 + 1);
        &mut self.grid[grid_index]
    }
}

/// Get neighbors of a cell.
/// A neighbor is considered a cell that is one step away in the cardinal directions (up, down, left, right).
pub fn get_neighbors(coord: (u8, u8), maze: &Maze) -> impl Iterator<Item = (u8, u8)> {
    let neighbors: Vec<(u8, u8)> = if maze.is_in_bounds(coord) {
        let (x, y) = coord;
        vec![
            // NOTE: This way of handling underflow/overflow is overflow-safe.
            // When x < 1 or y < 1, wrap x - 1 or y - 1 to u8::MAX to avoid underflow,
            // and automatically filter it out in the comparison.
            // When x + 1 or y + 1 exceeds u8::MAX, set it to u8::MAX to avoid overflow,
            // and automatically filter it out in the comparison (as the largest maze index numerically
            // possible is u8::MAX - 1, while the largest dimension numerically possible is u8::MAX).
            (x.wrapping_sub(1), y),
            (x.saturating_add(1), y),
            (x, y.wrapping_sub(1)),
            (x, y.saturating_add(1)),
        ]
    } else {
        // No neighbors if the coordinate is out of bounds
        vec![]
    };

    neighbors.into_iter().filter(move |&c| maze.is_in_bounds(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_maze_indexing() {
        let mut maze = Maze::new(5, 5);
        maze[(2, 3)] = Cell::START;
        assert_eq!(maze[(2, 3)], Cell::START);
    }

    #[test]
    fn test_remove_wall() {
        let mut maze = Maze::new(5, 5);
        assert!(maze.remove_wall_cell_after((1, 1), Orientation::Vertical));
        // Trying to remove the same wall again should return false
        assert!(!maze.remove_wall_cell_after((1, 1), Orientation::Vertical));
        // Check that the wall has been removed in the grid
        assert_eq!(maze.grid[(3, 5)], Cell::PATH);
    }

    #[test]
    fn test_out_of_bounds() {
        let maze = Maze::new(5, 5);
        assert!(!maze.is_in_bounds((5, 5)));
        assert!(!maze.is_in_bounds((0, 5)));
        assert!(!maze.is_in_bounds((5, 0)));
        assert!(maze.is_in_bounds((4, 4)));
    }
}
