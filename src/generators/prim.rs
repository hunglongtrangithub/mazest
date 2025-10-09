use rand::{Rng, SeedableRng, rngs::StdRng};
use rand_set::RandSetDefault;

use crate::maze::{Cell, Maze};
/// Get neighbors of a cell.
/// A neighbor is considered a cell that is two steps away in the cardinal directions (up, down, left, right).
fn get_neighbors(coord: (u16, u16), maze: &Maze) -> impl Iterator<Item = (u16, u16)> {
    let (x, y) = coord;

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
    .filter(|&(nx, ny)| nx < maze.width() && ny < maze.height())
}

/// Requires the maze to be at least 3x3.
pub fn randomized_prim(maze: &mut Maze) {
    if maze.width() < 3 || maze.height() < 3 {
        return;
    }

    let mut rng = StdRng::seed_from_u64(0);

    // Initialize the maze with walls
    (0..maze.height()).for_each(|y| {
        (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall);
    });

    // Initialize the starting point. Ensure the coordinates are odd.
    let start: (u16, u16) = (
        rng.random_range(1..maze.width() - 1) | 1,
        rng.random_range(1..maze.height() - 1) | 1,
    );
    maze[start] = Cell::Empty;

    let mut frontiers = get_neighbors(start, maze)
        // Technically, all neighbors are walls at this point
        .filter(|&coord| maze[coord] == Cell::Wall)
        .collect::<RandSetDefault<_>>();

    // Pick a random frontier cell
    while let Some(&frontier) = frontiers.get_rand() {
        // Remove the frontier from the set
        frontiers.remove(&frontier);

        // Mark all frontier cells as frontier in the maze for visualization
        frontiers
            .iter()
            .for_each(|&coord| maze[coord] = Cell::Frontier);
        maze.render().ok();

        // Get the neighbors of the frontier cell that are part of the maze (i.e., not walls)
        let empty_neighbors = get_neighbors(frontier, maze)
            .filter(|&coord| maze[coord] == Cell::Empty)
            .collect::<Vec<_>>();

        if !empty_neighbors.is_empty() {
            // Pick a random neighbor
            let neighbor_index = rng.random_range(0..empty_neighbors.len());
            let neighbor = empty_neighbors[neighbor_index];

            // Carve a passage between the frontier and the neighbor
            let passage = (
                if frontier.0 > neighbor.0 {
                    neighbor.0 + (frontier.0 - neighbor.0) / 2
                } else {
                    frontier.0 + (neighbor.0 - frontier.0) / 2
                },
                if frontier.1 > neighbor.1 {
                    neighbor.1 + (frontier.1 - neighbor.1) / 2
                } else {
                    frontier.1 + (neighbor.1 - frontier.1) / 2
                },
            );
            maze[frontier] = Cell::Empty;
            maze[passage] = Cell::Empty;

            maze.render().ok();

            // Add the neighbors of the frontier cell that are walls to the frontier set
            get_neighbors(frontier, maze)
                .filter(|&coord| maze[coord] == Cell::Wall)
                .for_each(|coord| {
                    frontiers.insert(coord);
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::Cell;

    #[test]
    fn test_get_neighbors() {
        let maze = Maze::new(7, 7);
        let neighbors = get_neighbors((3, 3), &maze).collect::<Vec<_>>();
        assert_eq!(neighbors, vec![(1, 3), (5, 3), (3, 1), (3, 5)]);
    }

    #[test]
    fn test_randomized_prim() {
        let mut maze = Maze::new(7, 7);
        randomized_prim(&mut maze);
        // Check that the start cell is empty
        assert_eq!(maze[(1, 1)], Cell::Empty);
        // Check that there are some empty cells in the maze
        assert!(maze.grid().iter().any(|cell| *cell == Cell::Empty));
        // Check that there are some walls in the maze
        assert!(maze.grid().iter().any(|cell| *cell == Cell::Wall));
    }
}
