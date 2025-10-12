use std::{cmp::Reverse, collections::BinaryHeap, rc::Rc};

use super::TrackedCell;
use crate::maze::{GridCell, Maze, Orientation};

pub fn solve_dijkstra(maze: &mut Maze, start: (u8, u8), goal: (u8, u8)) -> bool {
    if maze.is_empty() {
        return false;
    }

    maze.set(start, GridCell::START);

    // Priority queue for Dijkstra's algorithm
    // Using Reverse to turn the max-heap into a min-heap
    let mut pq: BinaryHeap<Reverse<TrackedCell>> = BinaryHeap::new();
    pq.push(Reverse(TrackedCell {
        coord: start,
        parent: None,
        traveling_cost: 0,
        heuristic_cost: 0,
    }));
    let mut visited = std::collections::HashSet::new();
    visited.insert(start);

    // Hash map to track the minimum cost to reach each cell
    let mut costs = std::collections::HashMap::new();
    costs.insert(start, 0);

    while let Some(Reverse(current)) = pq.pop() {
        if current.coord == goal {
            maze.set(current.coord, GridCell::GOAL);
            // Backtrack to mark the path
            let mut child = Rc::new(current);
            while let Some(parent) = child.parent.as_ref() {
                let (from, orientation) = if child.coord.0 == parent.coord.0 {
                    // Same column, so the path is verical
                    (
                        std::cmp::min_by_key(child.coord, parent.coord, |c| c.1),
                        Orientation::Vertical,
                    )
                } else {
                    // Same row, so the path is horizontal
                    (
                        std::cmp::min_by_key(child.coord, parent.coord, |c| c.0),
                        Orientation::Horizontal,
                    )
                };
                maze.set_path_cell_after(from, orientation);
                maze.render().ok();
                child = parent.clone();
            }
            maze.render().ok();
            return true; // Goal found
        }

        // Mark the current cell as visited
        if maze[current.coord] != GridCell::START {
            maze.set(current.coord, GridCell::VISITED);
        }
        maze.render().ok();

        let rc_current = Rc::new(current);
        let new_cost = rc_current.traveling_cost + 1; // Uniform cost for each step

        // Get neighbors that are paths and not visited
        let valid_neighbors = {
            let (x, y) = rc_current.coord;
            [
                (x.wrapping_sub(1), y),   // Left
                (x.saturating_add(1), y), // Right
                (x, y.wrapping_sub(1)),   // Up
                (x, y.saturating_add(1)), // Down
            ]
        }
        .into_iter()
        .enumerate()
        // Keep only in-bounds neighbors
        .filter(|&(_, c)| maze.is_in_bounds(c))
        .filter(|&(i, c)| {
            let is_neighbor_unvisited =
                !visited.contains(&c) && (maze[c] == GridCell::PATH || maze[c] == GridCell::GOAL);
            let (from, orientation) = match i {
                0 => (c, Orientation::Vertical),                  // Left
                1 => (rc_current.coord, Orientation::Vertical),   // Right
                2 => (c, Orientation::Horizontal),                // Up
                3 => (rc_current.coord, Orientation::Horizontal), // Down
                _ => unreachable!(),
            };
            let is_neighbor_reachable = !maze.is_wall_cell_after(from, orientation);
            // Only consider the neighbor if it is unvisited and reachable
            is_neighbor_unvisited && is_neighbor_reachable
        })
        // Only consider neigbors that we can reach with a lower cost
        .filter(|&(_, c)| {
            let is_cheaper = match costs.get(&c) {
                Some(&existing_cost) => new_cost < existing_cost,
                None => true,
            };
            if is_cheaper {
                costs.insert(c, new_cost);
            }
            is_cheaper
        })
        // Map to TrackedCell structs
        .map(|(_, c)| TrackedCell {
            coord: c,
            parent: Some(rc_current.clone()),
            traveling_cost: new_cost,
            heuristic_cost: 0,
        })
        .collect::<Vec<_>>();

        valid_neighbors.into_iter().for_each(|neighbor| {
            visited.insert(neighbor.coord);
            pq.push(Reverse(neighbor));
        });
    }

    false // No path found
}
