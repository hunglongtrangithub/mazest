use super::cell::Cell;

pub struct Grid {
    pub data: Box<[Cell]>,
    width: u16,
    height: u16,
}

impl Grid {
    pub fn new(width: u16, height: u16, cell: Cell) -> Self {
        let data = vec![cell; width as usize * height as usize].into_boxed_slice();
        Grid {
            data,
            width,
            height,
        }
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn is_boundary(&self, x: u16, y: u16) -> bool {
        x == 0 || y == 0 || x == self.width - 1 || y == self.height - 1
    }

    fn ravel_index(&self, x: u16, y: u16) -> usize {
        // Overflow-safe since width and height are u16 (assuming usize is at least 32 bits)
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
}

impl std::ops::Index<(u16, u16)> for Grid {
    type Output = Cell;

    fn index(&self, index: (u16, u16)) -> &Self::Output {
        &self.data[self.ravel_index(index.0, index.1)]
    }
}

impl std::ops::IndexMut<(u16, u16)> for Grid {
    fn index_mut(&mut self, index: (u16, u16)) -> &mut Self::Output {
        &mut self.data[self.ravel_index(index.0, index.1)]
    }
}
