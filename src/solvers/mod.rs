mod bfs;
mod dfs;

use std::rc::Rc;

use crate::Cell;
use crate::maze::Maze;
use bfs::solve_bfs;
use dfs::solve_dfs;

#[derive(Default)]
struct TrackedCell {
    coord: (u8, u8),
    parent: Option<Rc<TrackedCell>>,
    distance_cost: usize,
    heuristc_cost: usize,
}

pub enum Solver {
    Dfs,
    Bfs,
}

impl std::fmt::Display for Solver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Solver::Dfs => write!(f, "Depth-First Search (DFS)"),
            Solver::Bfs => write!(f, "Breadth-First Search (BFS)"),
        }
    }
}

pub fn solve_maze(maze: &mut Maze, solver: Solver) -> bool {
    let start = (0, 0);
    let goal = (maze.width() - 1, maze.height() - 1);
    maze[start] = Cell::START;
    maze[goal] = Cell::GOAL;

    match solver {
        Solver::Dfs => solve_dfs(maze, start, goal),
        Solver::Bfs => solve_bfs(maze, start, goal),
    }
}
