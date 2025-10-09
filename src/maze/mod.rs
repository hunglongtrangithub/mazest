pub mod cell;
mod grid;

use crossterm::execute;

use cell::{Cell, PathType, WallType};
use grid::Grid;

pub struct Maze {
    grid: Grid,
    width: u8,
    height: u8,
}

impl Maze {
    /// Creates a new maze with the given width and height.
    pub fn new(width: u8, height: u8) -> Self {
        // n cells in each dimension -> n + 1 walls -> 2n + 1 total
        let grid_height = height as u16 * 2 + 1;
        let grid_width = width as u16 * 2 + 1;
        let mut maze = Maze {
            grid: Grid::new(grid_width, grid_height, Cell::Wall(WallType::Block)),
            width,
            height,
        };
        (0..height).for_each(|y| {
            (0..width).for_each(|x| {
                maze[(x, y)] = Cell::Path(PathType::Empty);
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

    /// Checks if two coordinates are adjacent in the maze (horizontally or vertically).
    fn are_adjacent(&self, a: (u8, u8), b: (u8, u8)) -> bool {
        let dx = if a.0 > b.0 { a.0 - b.0 } else { b.0 - a.0 };
        let dy = if a.1 > b.1 { a.1 - b.1 } else { b.1 - a.1 };
        (dx == 1 && dy == 0) || (dx == 0 && dy == 1)
    }

    /// Removes the wall between two adjacent cells a and b.
    pub fn remove_wall(&mut self, a: (u8, u8), b: (u8, u8)) -> bool {
        // Assert that a and b are in bounds and adjacent
        assert!(self.is_in_bounds(a), "Coordinate a is out of bounds");
        assert!(self.is_in_bounds(b), "Coordinate b is out of bounds");
        assert!(
            self.are_adjacent(a, b),
            "Coordinates a and b are not adjacent"
        );

        // Calculate the wall position in the grid
        // Quick Math :)
        let wall_x = a.0 as u16 + b.0 as u16 + 1;
        let wall_y = a.1 as u16 + b.1 as u16 + 1;
        match self.grid[(wall_x, wall_y)] {
            Cell::Wall(_) => {
                self.grid[(wall_x, wall_y)] = Cell::Path(PathType::Empty);
                true
            }
            _ => false, // Wall already removed
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_maze_indexing() {
        let mut maze = Maze::new(5, 5);
        maze[(2, 3)] = Cell::Path(PathType::Start);
        assert_eq!(maze[(2, 3)], Cell::Path(PathType::Start));
    }

    #[test]
    fn test_remove_wall() {
        let mut maze = Maze::new(5, 5);
        assert!(maze.remove_wall((1, 1), (1, 2)));
        // Trying to remove the same wall again should return false
        assert!(!maze.remove_wall((1, 1), (1, 2)));
        // Check that the wall has been removed in the grid
        assert_eq!(maze.grid[(3, 5)], Cell::Path(PathType::Empty));
    }

    #[test]
    fn test_out_of_bounds() {
        let maze = Maze::new(5, 5);
        assert!(!maze.is_in_bounds((5, 5)));
        assert!(!maze.is_in_bounds((0, 5)));
        assert!(!maze.is_in_bounds((5, 0)));
        assert!(maze.is_in_bounds((4, 4)));
    }

    #[test]
    fn test_are_adjacent() {
        let maze = Maze::new(5, 5);
        assert!(maze.are_adjacent((1, 1), (1, 2)));
        assert!(maze.are_adjacent((1, 1), (2, 1)));
        assert!(!maze.are_adjacent((1, 1), (2, 2)));
        assert!(!maze.are_adjacent((0, 0), (2, 0)));
    }
}
