//! Maze generation algorithms.
//!
//! This module includes implementations of several maze generation algorithms:
//! - Randomized Depth-First Search (DFS)
//! - Prim's Algorithm
//! - Recursive Division
//! - Kruskal's Algorithm
//!
//! All algorithms generate perfect mazes (i.e., mazes without loops and with a unique path between any two points).
//! Each algorithm can be selected and applied to a [`Maze`] instance.

use rand::{SeedableRng, rngs::StdRng};

mod kruskal;
mod prim;
mod recur_backtrack;
mod recur_div;

use prim::randomized_prim;
use recur_backtrack::recursive_backtrack;
use recur_div::recursive_division;

use crate::{generators::kruskal::randomized_kruskal, maze::Maze};

/// Get a random number generator, optionally seeded for reproducibility.
fn get_rng(seed: Option<u64>) -> StdRng {
    match seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_os_rng(),
    }
}

pub enum Generator {
    RecurBacktrack,
    Prim,
    RecurDiv,
    Kruskal,
}

impl std::fmt::Display for Generator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Generator::RecurBacktrack => write!(f, "Recursive Backtracking"),
            Generator::Prim => write!(f, "Prim's Algorithm"),
            Generator::RecurDiv => write!(f, "Recursive Division"),
            Generator::Kruskal => write!(f, "Kruskal's Algorithm"),
        }
    }
}

pub fn generate_maze(maze: &mut Maze, generator: Generator, seed: Option<u64>) {
    match generator {
        Generator::RecurBacktrack => recursive_backtrack(maze, seed),
        Generator::Prim => randomized_prim(maze, seed),
        Generator::RecurDiv => recursive_division(maze, seed),
        Generator::Kruskal => randomized_kruskal(maze, seed),
    }
}
