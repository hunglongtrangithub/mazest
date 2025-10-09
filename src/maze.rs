use crossterm::{
    queue,
    style::{Color, Stylize},
};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Start,
    Goal,
    Wall,
    Path,
    Frontier,
    Visited,
    Empty,
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let styled_symbol = match self {
            Cell::Start => "S ".with(Color::Green),
            Cell::Goal => "G ".with(Color::Red),
            Cell::Wall => "# ".with(Color::White),
            Cell::Path => "* ".with(Color::Yellow),
            Cell::Frontier => "+ ".with(Color::Magenta),
            Cell::Visited => ". ".with(Color::Blue),
            Cell::Empty => "  ".with(Color::Reset),
        };
        write!(f, "{}", styled_symbol)
    }
}

pub struct Maze {
    grid: Box<[Cell]>,
    width: u16,
    height: u16,
}

impl Maze {
    pub fn new(width: u16, height: u16) -> Self {
        let width = if width < 3 { 3 } else { width | 1 };
        let height = if height < 3 { 3 } else { height | 1 };
        let grid = vec![Cell::Wall; width as usize * height as usize].into_boxed_slice();
        Maze {
            grid,
            width,
            height,
        }
    }

    #[cfg(test)]
    pub fn grid(&self) -> &[Cell] {
        &self.grid
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    fn ravel_index(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn display(&self) {
        for y in 0..self.height {
            for x in 0..self.width {
                print!("{}", self[(x, y)]);
            }
            println!();
        }
    }

    pub fn render(&self) -> std::io::Result<()> {
        queue!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0),
        )?;
        self.display();
        // // Wait for Enter key press
        // std::io::stdin().read_line(&mut String::new())?;
        std::thread::sleep(std::time::Duration::from_millis(10));
        Ok(())
    }
}

impl std::ops::Index<(u16, u16)> for Maze {
    type Output = Cell;

    fn index(&self, index: (u16, u16)) -> &Self::Output {
        let (x, y) = index;
        &self.grid[self.ravel_index(x, y)]
    }
}

impl std::ops::IndexMut<(u16, u16)> for Maze {
    fn index_mut(&mut self, index: (u16, u16)) -> &mut Self::Output {
        let (x, y) = index;
        &mut self.grid[self.ravel_index(x, y)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_maze_indexing() {
        let mut maze = Maze::new(5, 5);
        maze[(2, 3)] = Cell::Start;
        assert_eq!(maze[(2, 3)], Cell::Start);
    }
}
