use crate::{
    generators::get_rng,
    maze::{Maze, Orientation, cell::GridCell},
};
use rand::Rng;

use crate::maze::get_neighbors;

pub fn recursive_backtrack(maze: &mut Maze, seed: Option<u64>) {
    if maze.is_empty() {
        return;
    }

    let mut rng = get_rng(seed);

    // Initialize the maze with walls
    (0..maze.height()).for_each(|y| {
        (0..maze.width()).for_each(|x| {
            maze.set((x, y), GridCell::WALL);
        })
    });

    // Initialize the starting point
    let start: (u8, u8) = (
        rng.random_range(0..maze.width()),
        rng.random_range(0..maze.height()),
    );
    maze.set(start, GridCell::PATH);

    // Initialize the stack with the starting point
    // The stack will keep only path cells
    let mut stack = vec![start];

    while let Some(cell) = stack.pop() {
        let neighbors = get_neighbors(cell, maze)
            .filter(|&c| maze[c] == GridCell::WALL)
            .collect::<Vec<_>>();

        if !neighbors.is_empty() {
            let neighbor = neighbors[rng.random_range(0..neighbors.len())];
            maze.set(neighbor, GridCell::MARK);

            let (from, orientation) = if cell.0 == neighbor.0 {
                // Same column, so the wall is horizontal
                (
                    std::cmp::min_by_key(cell, neighbor, |c| c.1),
                    Orientation::Horizontal,
                )
            } else {
                // Same row, so the wall is vertical
                (
                    std::cmp::min_by_key(cell, neighbor, |c| c.0),
                    Orientation::Vertical,
                )
            };
            maze.remove_wall_cell_after(from, orientation);

            maze.set(neighbor, GridCell::PATH);
            // Put the cell back first so we can look at another neighbor  of this cell later
            stack.push(cell);
            // Put the neighbor to carve the maze in that neighbor's direction
            stack.push(neighbor);
        }
    }
}
