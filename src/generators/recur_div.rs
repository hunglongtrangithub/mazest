use crate::{
    generators::get_rng,
    maze::{Maze, Orientation},
};
use rand::{Rng, rngs::StdRng};

pub fn recursive_division(maze: &mut Maze, seed: Option<u64>) {
    if maze.is_empty() {
        return;
    }

    // Clear all walls in the maze, except the boundary walls
    maze.clear_walls();

    // Initialize the RNG
    let mut rng = get_rng(seed);

    // Start the recursive division
    divide(maze, (0, 0), maze.width(), maze.height(), &mut rng);

    fn divide(maze: &mut Maze, top_left: (u8, u8), width: u8, height: u8, rng: &mut StdRng) {
        if width < 2 || height < 2 {
            return;
        }

        let (x, y) = top_left;

        let orientation = match width.cmp(&height) {
            std::cmp::Ordering::Less => Orientation::Horizontal,
            std::cmp::Ordering::Greater => Orientation::Vertical,
            std::cmp::Ordering::Equal => {
                if rng.random_bool(0.5) {
                    Orientation::Horizontal
                } else {
                    Orientation::Vertical
                }
            }
        };

        match orientation {
            Orientation::Horizontal => {
                // Randomly choose a y coordinate for the horizontal wall
                let diff = rng.random_range(0..height - 1);
                let y_wall = y + diff;

                // Randomly choose a position for the hole in the wall
                let x_hole = x + rng.random_range(0..width);

                // Place the wall line horizontally after row index y_wall
                maze.insert_wall_line_after(y_wall, x, x + width - 1, Orientation::Horizontal);
                // Create a hole in the wall line
                maze.remove_wall_cell_after((x_hole, y_wall), Orientation::Horizontal);
                maze.render().ok();

                let upper_height = diff + 1;
                let lower_height = height - upper_height;

                // Recursively divide the regions above and below the wall
                divide(maze, (x, y), width, upper_height, rng);
                divide(maze, (x, y_wall + 1), width, lower_height, rng);
            }
            Orientation::Vertical => {
                // Choose a random x coordinate for the vertical wall
                let diff = rng.random_range(0..width - 1);
                let x_wall = x + diff;

                // Randomly choose a position for the hole in the wall
                let y_hole = y + rng.random_range(0..height);

                // Place the wall vertically after column index x_wall
                maze.insert_wall_line_after(x_wall, y, y + height - 1, Orientation::Vertical);
                // Create a hole in the wall line
                maze.remove_wall_cell_after((x_wall, y_hole), Orientation::Vertical);
                maze.render().ok();

                let left_width = diff + 1;
                let right_width = width - left_width;

                // Recursively divide the regions left and right of the wall
                divide(maze, (x, y), left_width, height, rng);
                divide(maze, (x_wall + 1, y), right_width, height, rng);
            }
        }
    }
}
