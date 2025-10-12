use std::rc::Rc;

mod bfs;
mod dfs;
// mod dijkstra;

use crate::GridCell;
use crate::maze::Maze;
use bfs::solve_bfs;
use dfs::solve_dfs;

#[derive(Default)]
struct TrackedCell {
    /// Coordinates of the cell in the maze
    coord: (u8, u8),
    /// The parent cell from which this cell was reached
    parent: Option<Rc<TrackedCell>>,
    /// Cost to reach this cell from the start
    traveling_cost: usize,
    /// Estimated cost to reach the goal from this cell (for A* algorithm)
    heuristic_cost: usize,
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
    maze[start] = GridCell::START;
    maze[goal] = GridCell::GOAL;

    match solver {
        Solver::Dfs => solve_dfs(maze, start, goal),
        Solver::Bfs => solve_bfs(maze, start, goal),
    }
}
