pub mod dfs;
pub mod prim;

use crate::maze::Maze;
// TODO: Add Kruskal's and Recursive Division generators

/// Get neighbors of a cell.
/// A neighbor is considered a cell that is one step away in the cardinal directions (up, down, left, right).
fn get_neighbors(coord: (u8, u8), maze: &Maze) -> impl Iterator<Item = (u8, u8)> {
    let neighbors: Vec<(u8, u8)> = if maze.is_in_bounds(coord) {
        let (x, y) = coord;
        vec![
            // NOTE: This way of handling underflow/overflow is overflow-safe.
            // When x < 1 or y < 1, wrap x - 1 or y - 1 to u8::MAX to avoid underflow,
            // and automatically filter it out in the comparison.
            // When x + 1 or y + 1 exceeds u8::MAX, set it to u8::MAX to avoid overflow,
            // and automatically filter it out in the comparison (as the largest maze index numerically
            // possible is u8::MAX - 1, while the largest dimension numerically possible is u8::MAX).
            (x.wrapping_sub(1), y),
            (x.saturating_add(1), y),
            (x, y.wrapping_sub(1)),
            (x, y.saturating_add(1)),
        ]
    } else {
        // No neighbors if the coordinate is out of bounds
        vec![]
    };

    neighbors
        .into_iter()
        .filter(|(nx, ny)| *nx < maze.width() && *ny < maze.height())
}
