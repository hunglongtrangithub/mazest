use crate::maze::Cell;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::maze::Maze;

fn randomized_depth_first(maze: &mut Maze) {
    // Initialize the maze with walls
    (0..maze.height()).for_each(|y| {
        (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall);
    });

    // Start the maze generation from a random cell
}

fn recursive_division(maze: &mut Maze) {
    // Initialize the maze with walls
    (0..maze.height()).for_each(|y| {
        (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall);
    });
}

/// Get unvisited neighbors of a cell.
/// A neighbor is considered a cell that is two steps away in the cardinal directions (up, down, left, right).
/// A neighbor is considered unvisited if it is a wall and within the maze bounds.
/// x and y are the coordinates of the current cell in the maze.
fn get_unvisited_neighbors(x: u16, y: u16, maze: &Maze) -> Vec<(u16, u16)> {
    if x >= maze.width() || y >= maze.height() {
        return Vec::new();
    }

    [
        // NOTE: This way of handling underflow/overflow is overflow-safe.
        // When x < 2 or y < 2, set x - 2 or y - 2 to u16::MAX to avoid underflow,
        // and automatically filter it out in the comparison.
        // When x + 2 or y + 2 exceeds u16::MAX, set it to u16::MAX to avoid overflow,
        // and automatically filter it out in the comparison (as the largest index numerically
        // possible is u16::MAX - 1, while the largest dimension numerically possible is u16::MAX).
        (x.checked_sub(2).unwrap_or(u16::MAX), y),
        (x.saturating_add(2), y),
        (x, y.checked_sub(2).unwrap_or(u16::MAX)),
        (x, y.saturating_add(2)),
    ]
    .into_iter()
    .filter(|&(nx, ny)| nx < maze.width() && ny < maze.height() && maze[(nx, ny)] == Cell::Wall)
    .collect::<Vec<_>>()
}

pub fn randomized_prim(maze: &mut Maze) {
    let mut rng = StdRng::seed_from_u64(0);

    // Initialize the maze with walls
    (0..maze.height()).for_each(|y| {
        (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall);
    });

    // Initialize the starting point
    let start: (u16, u16) = (
        rng.random_range(0..maze.width()),
        rng.random_range(0..maze.height()),
    );

    let mut frontiers = vec![start];
    while !frontiers.is_empty() {
        // Ramdonly select a cell from the frontiers
        let idx = rng.random_range(0..frontiers.len());
        let (cx, cy) = frontiers.swap_remove(idx);
        maze[(cx, cy)] = Cell::Path;

        let neighbors = get_unvisited_neighbors(cx, cy, maze);
        if neighbors.is_empty() {
            // No unvisited neighbors, continue to the next cell
            continue;
        }

        // Randomly select a neighbor
        let idx = rng.random_range(0..neighbors.len());
        let (nx, ny) = neighbors[idx];

        // Remove the wall between the cell and the neighbor
        let wall_x = if cx < nx {
            cx + (nx - cx) / 2
        } else {
            nx + (cx - nx) / 2
        };
        let wall_y = if cy < ny {
            cy + (ny - cy) / 2
        } else {
            ny + (cy - ny) / 2
        };
        maze[(wall_x, wall_y)] = Cell::Path;
        maze.render().ok();

        // Add the neighbors to the list of frontiers
        frontiers.extend(neighbors.iter());
    }
}
