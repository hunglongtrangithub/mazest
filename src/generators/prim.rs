use rand::Rng;
use rand_set::RandSetDefault;

use crate::generators::get_rng;
use crate::maze::{Cell, Maze, Orientation, PathType, WallType, get_neighbors};

/// Requires the maze to be at least 3x3.
pub fn randomized_prim(maze: &mut Maze, seed: Option<u64>) {
    if maze.is_empty() {
        return;
    }

    let mut rng = get_rng(seed);

    // Initialize the maze with walls
    (0..maze.height())
        .for_each(|y| (0..maze.width()).for_each(|x| maze[(x, y)] = Cell::Wall(WallType::Wall)));

    // Initialize the starting point
    let start: (u8, u8) = (
        rng.random_range(0..maze.width()),
        rng.random_range(0..maze.height()),
    );
    maze[start] = Cell::Path(PathType::Empty);

    // Get the neighbors of the starting point and add them to the frontier set
    // Currently, all neighbors are walls at this point
    let mut frontiers = get_neighbors(start, maze)
        .filter(|&coord| maze[coord] == Cell::Wall(WallType::Wall))
        .collect::<RandSetDefault<_>>();
    // Mark all frontier cells as marked in the maze for visualization
    frontiers
        .iter()
        .for_each(|&coord| maze[coord] = Cell::Wall(WallType::Mark));

    // Pick a random frontier cell
    while let Some(&frontier) = frontiers.get_rand() {
        // Remove the frontier from the set
        frontiers.remove(&frontier);

        // Get the neighbors of the frontier cell that are part of the maze (i.e., not walls)
        let empty_neighbors = get_neighbors(frontier, maze)
            .filter(|&coord| maze[coord] == Cell::Path(PathType::Empty))
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
            maze[frontier] = Cell::Path(PathType::Empty);

            maze.render().ok();

            // Add the neighbors of the frontier cell that are walls to the frontier set
            let neighbors_to_mark = get_neighbors(frontier, maze)
                .filter(|&coord| maze[coord] == Cell::Wall(WallType::Wall))
                .collect::<Vec<_>>();

            for coord in neighbors_to_mark {
                // Only mark the cell if it hasn't been added to the frontier set before
                if frontiers.insert(coord) {
                    maze[coord] = Cell::Wall(WallType::Mark);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::cell::Cell;

    #[test]
    fn test_get_neighbors() {
        let maze = Maze::new(7, 7);
        let neighbors = get_neighbors((3, 3), &maze).collect::<Vec<_>>();
        assert_eq!(neighbors, vec![(2, 3), (4, 3), (3, 2), (3, 4)]);
    }

    #[test]
    fn test_randomized_prim() {
        let mut maze = Maze::new(7, 7);
        randomized_prim(&mut maze, None);
        // Check that the start cell is empty
        assert_eq!(maze[(1, 1)], Cell::Path(PathType::Empty));
        // Check that there are some empty cells in the maze
        assert!(
            maze.grid()
                .iter()
                .any(|cell| *cell == Cell::Path(PathType::Empty))
        );
        // Check that there are some walls in the maze
        assert!(
            maze.grid()
                .iter()
                .any(|cell| *cell == Cell::Wall(WallType::Wall))
        );
    }
}
