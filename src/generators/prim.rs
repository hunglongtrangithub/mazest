use rand::Rng;
use rand_set::RandSetDefault;

use crate::generators::get_rng;
use crate::maze::{Maze, Orientation, cell::GridCell, get_neighbors};

/// Requires the maze to be at least 3x3.
pub fn randomized_prim(maze: &mut Maze, seed: Option<u64>) {
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
    maze.set(start, GridCell::EMPTY);

    // Get the neighbors of the starting point and add them to the frontier set
    // Currently, all neighbors are walls at this point
    let mut frontiers = get_neighbors(start, maze)
        .filter(|&coord| maze[coord] == GridCell::WALL)
        .collect::<RandSetDefault<_>>();
    // Mark all frontier cells as marked in the maze for visualization
    frontiers.iter().for_each(|&coord| {
        maze.set(coord, GridCell::MARK);
    });

    // Pick a random frontier cell
    while let Some(&frontier) = frontiers.get_rand() {
        // Remove the frontier from the set
        frontiers.remove(&frontier);

        // Get the neighbors of the frontier cell that are part of the maze (i.e., not walls)
        let empty_neighbors = get_neighbors(frontier, maze)
            .filter(|&coord| maze[coord] == GridCell::EMPTY)
            .collect::<Vec<_>>();

        if !empty_neighbors.is_empty() {
            // Pick a random neighbor
            let neighbor_index = rng.random_range(0..empty_neighbors.len());
            let neighbor = empty_neighbors[neighbor_index];

            // Carve a passage between the frontier and the neighbor
            let (from, orientation) = if frontier.0 == neighbor.0 {
                // Same column, so the wall is horizontal
                (
                    std::cmp::min_by_key(frontier, neighbor, |c| c.1),
                    Orientation::Horizontal,
                )
            } else {
                // Same row, so the wall is vertical
                (
                    std::cmp::min_by_key(frontier, neighbor, |c| c.0),
                    Orientation::Vertical,
                )
            };
            maze.remove_wall_cell_after(from, orientation);

            // Mark the frontier cell as part of the maze
            maze.set(frontier, GridCell::EMPTY);

            // Add the neighbors of the frontier cell that are walls to the frontier set
            let neighbors_to_mark = get_neighbors(frontier, maze)
                .filter(|&coord| maze[coord] == GridCell::WALL)
                .collect::<Vec<_>>();

            for coord in neighbors_to_mark {
                // Only mark the cell if it hasn't been added to the frontier set before
                if frontiers.insert(coord) {
                    maze.set(coord, GridCell::MARK);
                }
            }
        }
    }
}
