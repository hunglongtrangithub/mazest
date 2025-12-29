use crate::{
    app::game::UiEvent,
    generators::{Generator, generate_maze},
    maze::{
        Maze, Orientation,
        cell::{GridCell, PathType},
        grid::Grid,
    },
};
use std::sync::mpsc::Sender;

#[derive(Debug, Copy, Clone)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub struct GameState {
    /// The maze being played
    maze: Maze,
    /// Tracks where the player currently is
    current: (u8, u8),
    /// Goal position
    goal: (u8, u8),
    /// Sender to send UI events of the maze's grid to the render thread
    ui_event_tx: Sender<UiEvent>,
}

impl GameState {
    /// Set up the initial game state with:
    /// * Maze generation algorithm.
    /// * Maze width & height.
    /// * Start & goal positions. Either randomized (with `random_start_goal = true`) or top left
    ///   for start cell and bottom right for goal cell.
    ///
    /// Panics if either width or height is 0.
    /// Return the initialized [`GameState`].
    pub fn initialize(
        width: u8,
        height: u8,
        generator: Generator,
        ui_event_tx: Sender<UiEvent>,
    ) -> Self {
        // Get the initial maze
        let mut maze = Maze::new(width, height, None);
        // Carve the maze with the generator algorithm
        generate_maze(&mut maze, generator, None);

        let start = (0, 0);
        maze.set(start, GridCell::PACMAN);

        let goal = (width - 1, height - 1);
        maze.set(goal, GridCell::GOAL);

        GameState {
            maze,
            goal,
            current: start,
            ui_event_tx,
        }
    }

    /// Get the game state's maze grid reference.
    pub fn grid(&self) -> &Grid {
        self.maze.grid()
    }

    /// Check if the goal has been reached.
    pub fn goal_reached(&self) -> bool {
        self.current == self.goal
    }

    /// Attempt to move Pacman in the specified direction.
    /// Marks the previous cell as visited and updates current position if move is valid.
    /// Unmarks the path cell in between as empty if moving to an already visited cell.
    /// Returns the new position if the move is successful, None otherwise.
    pub fn move_pacman(&mut self, direction: Direction) -> Option<(u8, u8)> {
        // Calculate new position + determine orientation for wall checking + path orientation to set if no wall
        let (new_pos, check_pos, wall_orientation, path_orientation) = match direction {
            Direction::Left => {
                let new_x = self.current.0.checked_sub(1)?;
                let new_pos = (new_x, self.current.1);
                // Moving left: vertical wall to the right of new_pos
                (
                    new_pos,
                    new_pos,
                    Orientation::Vertical,
                    Orientation::Horizontal,
                )
            }
            Direction::Right => {
                let new_x = self.current.0.checked_add(1)?;
                if new_x >= self.maze.width() {
                    return None;
                }
                let new_pos = (new_x, self.current.1);
                // Moving right: vertical wall to the right of current
                (
                    new_pos,
                    self.current,
                    Orientation::Vertical,
                    Orientation::Horizontal,
                )
            }
            Direction::Up => {
                let new_y = self.current.1.checked_sub(1)?;
                let new_pos = (self.current.0, new_y);
                // Moving up: horizontal wall below new_pos
                (
                    new_pos,
                    new_pos,
                    Orientation::Horizontal,
                    Orientation::Vertical,
                )
            }
            Direction::Down => {
                let new_y = self.current.1.checked_add(1)?;
                if new_y >= self.maze.height() {
                    return None;
                }
                let new_pos = (self.current.0, new_y);
                // Moving down: horizontal wall below current
                (
                    new_pos,
                    self.current,
                    Orientation::Horizontal,
                    Orientation::Vertical,
                )
            }
        };

        // Check for walls; disallow movement if a wall exists
        if self.maze.is_wall_cell_after(check_pos, wall_orientation) {
            return None;
        }

        // Check whether new_pos is visited and send UI updates to render thread
        // It's fine if render thread is closed
        if *self.maze.cell_at(new_pos) == GridCell::VISITED {
            tracing::debug!("[game] Moving to already visited cell at {:?}", new_pos);
            // Mark the current cell as empty path
            let current_grid_coord = self.maze.set(self.current, GridCell::EMPTY);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: current_grid_coord,
                    new: GridCell::EMPTY,
                })
                .ok();

            // Mark the route cell in between as empty path
            let route_grid_coord =
                self.maze
                    .set_path_cell_after(check_pos, path_orientation, Some(PathType::Empty));
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: route_grid_coord,
                    new: GridCell::EMPTY,
                })
                .ok();
        } else {
            tracing::debug!("[game] Moving to new cell at {:?}", new_pos);
            // Mark the current cell as visited,
            let current_grid_coord = self.maze.set(self.current, GridCell::VISITED);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: current_grid_coord,
                    new: GridCell::VISITED,
                })
                .ok();

            // Mark the path cell in between as a route cell
            let route_grid_coord = self
                .maze
                .set_path_cell_after(check_pos, path_orientation, None);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: route_grid_coord,
                    new: GridCell::Path(PathType::Route(path_orientation)),
                })
                .ok();
        }

        // Mark the new position as Pacman
        let new_grid_coord = self.maze.set(new_pos, GridCell::PACMAN);
        self.ui_event_tx
            .send(UiEvent::GridUpdate {
                coord: new_grid_coord,
                new: GridCell::PACMAN,
            })
            .ok(); // Error when render thread is closed, ignore

        // Update current position
        self.current = new_pos;

        Some(self.current)
    }
}
