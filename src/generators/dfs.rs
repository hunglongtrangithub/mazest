use crate::maze::Maze;
use crate::maze::cell::{Cell, PathType, WallType};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::generators::get_neighbors;

pub fn randomized_dfs(maze: &mut Maze) {
    if maze.is_empty() {
        return;
    }

    let mut rng = StdRng::seed_from_u64(0);

    // Initialize the maze with walls
    (0..maze.height())
        .for_each(|y| (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall(WallType::Block)));

    // Initialize the starting point
    let start: (u8, u8) = (
        rng.random_range(0..maze.width()),
        rng.random_range(0..maze.height()),
    );
    maze[start] = Cell::Path(PathType::Empty);

    // Initialize the stack with the starting point
    // The stack will keep only path cells
    let mut stack = vec![start];

    while let Some(cell) = stack.pop() {
        let neighbors = get_neighbors(cell, maze)
            .filter(|&c| maze[c] == Cell::Wall(WallType::Block))
            .collect::<Vec<_>>();

        if !neighbors.is_empty() {
            let neighbor = neighbors[rng.random_range(0..neighbors.len())];
            maze.remove_wall(cell, neighbor);
            maze[neighbor] = Cell::Path(PathType::Empty);
            maze.render().ok();
            // Put the cell back first so we can look at another neighbor  of this cell later
            stack.push(cell);
            // Put the neighbor to carve the maze in that neighbor's direction
            stack.push(neighbor);
        }
    }
}
