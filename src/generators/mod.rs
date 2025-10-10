use rand::{SeedableRng, rngs::StdRng};

mod dfs;
mod kruskal;
mod prim;
mod recur_div;

pub use dfs::randomized_dfs;
pub use prim::randomized_prim;
pub use recur_div::recursive_division;

use crate::{generators::kruskal::randomized_kruskal, maze::Maze};
// TODO: Add Kruskal's generator

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

/// Get a random number generator, optionally seeded for reproducibility.
fn get_rng(seed: Option<u64>) -> StdRng {
    match seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_os_rng(),
    }
}

pub enum Generator {
    Dfs,
    Prim,
    RecurDiv,
    Kruskal,
}

impl std::fmt::Display for Generator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Generator::Dfs => write!(f, "Randomized Depth-First Search (DFS)"),
            Generator::Prim => write!(f, "Prim's Algorithm"),
            Generator::RecurDiv => write!(f, "Recursive Division"),
            Generator::Kruskal => write!(f, "Kruskal's Algorithm"),
        }
    }
}

pub fn generate_maze(maze: &mut Maze, generator: Generator, seed: Option<u64>) {
    match generator {
        Generator::Dfs => randomized_dfs(maze, seed),
        Generator::Prim => randomized_prim(maze, seed),
        Generator::RecurDiv => recursive_division(maze, seed),
        Generator::Kruskal => randomized_kruskal(maze, seed),
    }
}
