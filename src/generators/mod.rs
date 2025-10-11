use rand::{SeedableRng, rngs::StdRng};

mod dfs;
mod kruskal;
mod prim;
mod recur_div;

use dfs::randomized_dfs;
use prim::randomized_prim;
use recur_div::recursive_division;

use crate::{generators::kruskal::randomized_kruskal, maze::Maze};
// TODO: Add Kruskal's generator

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
