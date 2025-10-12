use std::cmp::Ordering;
use std::rc::Rc;

mod astar;
mod bfs;
mod dfs;
mod dijkstra;

use crate::GridCell;
use crate::maze::Maze;
use astar::solve_astart;
use bfs::solve_bfs;
use dfs::solve_dfs;
use dijkstra::solve_dijkstra;

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

impl TrackedCell {
    /// Total cost for A* (traveling cost + heuristic cost)
    fn total_cost(&self) -> usize {
        self.traveling_cost + self.heuristic_cost
    }
}

impl Eq for TrackedCell {}

impl PartialEq for TrackedCell {
    fn eq(&self, other: &Self) -> bool {
        self.total_cost() == other.total_cost()
    }
}

impl Ord for TrackedCell {
    fn cmp(&self, other: &Self) -> Ordering {
        self.total_cost().cmp(&other.total_cost())
    }
}

impl PartialOrd for TrackedCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub enum Solver {
    Dfs,
    Bfs,
    Dijkstra,
    AStar,
}

impl std::fmt::Display for Solver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Solver::Dfs => write!(f, "Depth-First Search (DFS)"),
            Solver::Bfs => write!(f, "Breadth-First Search (BFS)"),
            Solver::Dijkstra => write!(f, "Dijkstra's Algorithm"),
            Solver::AStar => write!(f, "A* Search Algorithm"),
        }
    }
}

pub fn solve_maze(maze: &mut Maze, solver: Solver) -> bool {
    let start = (0, 0);
    let goal = (maze.width() - 1, maze.height() - 1);
    maze.set(start, GridCell::START);
    maze.set(goal, GridCell::GOAL);

    match solver {
        Solver::Dfs => solve_dfs(maze, start, goal),
        Solver::Bfs => solve_bfs(maze, start, goal),
        Solver::Dijkstra => solve_dijkstra(maze, start, goal),
        Solver::AStar => solve_astart(maze, start, goal),
    }
}
