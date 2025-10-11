use crate::maze::{Cell, Maze, PathType, get_neighbors};

pub fn solve_dfs(maze: &mut Maze, start: (u8, u8), goal: (u8, u8)) -> bool {
    if maze.is_empty() {
        return false;
    }

    // Stack for DFS
    let mut stack = vec![start];
    let mut visited = std::collections::HashSet::new();
    visited.insert(start);

    while let Some(current) = stack.pop() {
        if current == goal {
            maze[current] = Cell::Path(PathType::Goal);
            return true; // Goal found
        }

        // Mark the current cell as visited
        if maze[current] != Cell::Path(PathType::Start) {
            maze[current] = Cell::Path(PathType::Visited);
        }
        maze.render().ok();

        // Get neighbors that are paths and not visited
        let neighbors = get_neighbors(current, maze)
            .filter(|&c| {
                (maze[c] == Cell::Path(PathType::Empty) || maze[c] == Cell::Path(PathType::Goal))
                    && !visited.contains(&c)
            })
            .collect::<Vec<_>>();

        for neighbor in neighbors {
            visited.insert(neighbor);
            stack.push(neighbor);
        }
    }

    false // No path found
}
