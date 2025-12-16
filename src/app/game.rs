use crate::{
    app,
    generators::{Generator, generate_maze},
    maze::{
        Maze, Orientation,
        cell::{GridCell, PathType},
    },
};
use crossterm::{
    ExecutableCommand, cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color, StyledContent, Stylize},
    terminal::{self, ClearType},
};
use std::{
    io::{Stdout, Write},
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{Receiver, Sender},
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
enum UserInputEvent {
    KeyPress(event::KeyEvent),
    Resize,
}

#[derive(Debug, Copy, Clone)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

enum UiEvent {
    GridInit { width: u16, height: u16 },
    GridUpdate { coord: (u16, u16), new: GridCell },
    LogMessage(Option<StyledContent<String>>),
}

#[derive(Debug, PartialEq)]
enum GameRunResult {
    /// Goal is reached before timer runs out
    GoalReached,
    /// Timer runs out before goal is reached
    Timeout,
    /// Game is canceled by user
    Canceled,
}

struct GameState {
    maze: Maze,
    /// Tracks where the player currently is
    current: (u8, u8),
    goal: (u8, u8),
    /// Sender to send UI events of the maze's grid to the render thread
    ui_event_tx: Sender<UiEvent>,
}

/// Timeout for polling input events in the input thread, a.k.a.
/// how often to check for done/cancel flags for the game
const USER_INPUT_EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(100);
/// Timeout for receiving input events, a.k.a. how often to check for render done/cancel flags
const INPUT_RECV_TIMEOUT: Duration = Duration::from_millis(100);
/// Max time for one run
const GAME_RUN_DURATION: Duration = Duration::from_secs(60);
/// Tick duration for the game timer
const GAME_TIMER_TICK_DURATION: Duration = Duration::from_secs(1);

impl GameState {
    /// Set up the initial game state with:
    /// * Maze generation algorithm.
    /// * Maze width & height.
    /// * Start & goal positions. Either randomized (with `random_start_goal = true`) or top left
    ///   for start cell and bottom right for goal cell.
    ///
    /// Panics if either width or height is 0.
    /// Return the initialized [`GameState`].
    fn initialize(
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

    /// Attempt to move the player in the specified direction.
    /// Marks the previous cell as visited and updates current position if move is valid.
    /// Returns the new position if the move is successful, None otherwise.
    fn move_player(&mut self, direction: Direction) -> Option<(u8, u8)> {
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

        // Check whether new_pos is visited
        if *self.maze.cell_at(new_pos) == GridCell::VISITED {
            tracing::debug!("[game] Moving to already visited cell at {:?}", new_pos);
            // Mark the current cell as empty path
            let current_grid_coord = self.maze.set(self.current, GridCell::EMPTY);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: current_grid_coord,
                    new: GridCell::EMPTY,
                })
                .ok(); // Error when render thread is closed, ignore

            // Mark the route cell in between as empty path
            let route_grid_coord =
                self.maze
                    .set_path_cell_after(check_pos, path_orientation, Some(PathType::Empty));
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: route_grid_coord,
                    new: GridCell::EMPTY,
                })
                .ok(); // Error when render thread is closed, ignore
        } else {
            tracing::debug!("[game] Moving to new cell at {:?}", new_pos);
            // Mark the current cell as visited,
            let current_grid_coord = self.maze.set(self.current, GridCell::VISITED);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: current_grid_coord,
                    new: GridCell::VISITED,
                })
                .ok(); // Error when render thread is closed, ignore

            // Mark the path cell in between as a route cell
            let route_grid_coord = self
                .maze
                .set_path_cell_after(check_pos, path_orientation, None);
            self.ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: route_grid_coord,
                    new: GridCell::Path(PathType::Route(path_orientation)),
                })
                .ok(); // Error when render thread is closed, ignore
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

/// Render the current maze to stdout
fn render_ui_events(ui_event_rx: Receiver<UiEvent>) -> std::io::Result<()> {
    // Get a new stdout handle
    let mut stdout = std::io::stdout();
    // Store grid dimensions once received
    let mut grid_dims = None;
    loop {
        // Block and render any new grid events as they come from the sender
        match ui_event_rx.recv() {
            Err(_) => {
                // All senders are dropped. Break from the loop
                tracing::debug!("[render] UI event channel closed, exiting render thread");
                break;
            }
            Ok(event) => match event {
                UiEvent::GridInit { width, height } => {
                    grid_dims = Some((width, height));
                }
                UiEvent::GridUpdate { coord, new } => {
                    match grid_dims {
                        Some(grid_dims) => {
                            if coord.0 >= grid_dims.0 || coord.1 >= grid_dims.1 {
                                // Out of bounds, skip rendering
                                continue;
                            }
                        }
                        // Grid dimensions not yet received, cannot render
                        None => continue,
                    }
                    // Move the cursor to the specified coordinate and print the
                    // new cell using the grid dimensions
                    queue!(
                        stdout,
                        cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1),
                        style::Print(new)
                    )?;
                    stdout.flush()?;
                }
                UiEvent::LogMessage(msg) => {
                    // Log message to terminal below the maze
                    app::log_terminal(
                        &mut stdout,
                        // Use grid height if available, otherwise 0
                        grid_dims.unwrap_or((0, 0)).1,
                        msg,
                    )?;
                }
            },
        }
    }
    Ok(())
}

/// Spawn a run of the game, including user interaction, timer, and maze rendering
fn start_game(
    stdout: &mut Stdout,
    width: u8,
    height: u8,
    generator: Generator,
) -> std::io::Result<GameRunResult> {
    // Clear screen
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    let (ui_event_tx, ui_event_rx) = std::sync::mpsc::channel::<UiEvent>();

    // Spawn render thread to render grid events from the game state
    // Initial maze will be rendered to the terminal
    let render_thread_handle =
        std::thread::spawn(move || -> std::io::Result<()> { render_ui_events(ui_event_rx) });

    // Initialize game state and render initial maze
    let game_state = GameState::initialize(width, height, generator, ui_event_tx.clone());
    // Send grid dimensions to render thread
    if ui_event_tx
        .send(UiEvent::GridInit {
            width: game_state.maze.grid().width(),
            height: game_state.maze.grid().height(),
        })
        .is_err()
    {
        // Just return if render thread has exited
        return Ok(GameRunResult::Canceled);
    }
    // Send initial grid cells to render thread
    for y in 0..game_state.maze.grid().height() {
        for x in 0..game_state.maze.grid().width() {
            if ui_event_tx
                .send(UiEvent::GridUpdate {
                    coord: (x, y),
                    new: game_state.maze.grid()[(x, y)],
                })
                .is_err()
            {
                // Just return if render thread has exited
                return Ok(GameRunResult::Canceled);
            }
        }
    }

    // Flag to let other threads stop. Enabled by the main thread only.
    let should_stop = Arc::new(AtomicBool::new(false));

    let (user_input_event_tx, user_input_event_rx) = std::sync::mpsc::channel::<UserInputEvent>();

    // Spawn user input thread
    let should_stop_for_input = should_stop.clone();
    let input_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
        listen_to_user_input(
            user_input_event_tx,
            USER_INPUT_EVENT_POLL_TIMEOUT,
            &should_stop_for_input,
        )
    });

    // Spawn a thread to start the timer
    let start_time = Instant::now();
    let should_stop_for_timer = should_stop.clone();
    let timer_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
        start_timer(
            start_time,
            GAME_RUN_DURATION,
            GAME_TIMER_TICK_DURATION,
            &should_stop_for_timer,
            ui_event_tx,
        )
    });

    let grid_height = game_state.maze.grid().height();

    // Start game loop in main thread
    let game_result = game_loop(
        game_state,
        user_input_event_rx,
        INPUT_RECV_TIMEOUT,
        &should_stop,
        &timer_thread_handle,
    )?;
    tracing::debug!("[game] Game loop exited with result: {:?}", game_result);

    // At this point, all UI event senders are dropped, so the render thread will exit
    // Wait for render and input threads to finish
    tracing::debug!("[game] Waiting for render and input threads to finish...");
    // TODO: Make render thread and input thread terminate more quickly
    render_thread_handle
        .join()
        .expect("Render thread paniched")?;
    tracing::debug!("[game] Render thread finished");
    input_thread_handle.join().expect("Input thread panicked")?;
    tracing::debug!("[game] Input thread finished");

    app::log_terminal(stdout, grid_height, None::<StyledContent<&str>>)?;
    match game_result {
        GameRunResult::GoalReached => {
            app::log_terminal(
                stdout,
                grid_height,
                Some(
                    "Congratulations! You reached the goal! Press Enter to continue, or Esc to exit."
                        .with(Color::Green)
                        .attribute(Attribute::Bold),
                ),
            )?;
        }
        GameRunResult::Timeout => {
            app::log_terminal(
                stdout,
                grid_height,
                Some(
                    "Time's up! You failed to reach the goal. Press Enter to continue, or Esc to exit."
                        .with(Color::Red)
                        .attribute(Attribute::Bold),
                ),
            )?;
        }
        GameRunResult::Canceled => {
            // Just return immediately
            return Ok(game_result);
        }
    }

    loop {
        if let Event::Key(event::KeyEvent { code, kind, .. }) = event::read()?
            && kind == event::KeyEventKind::Press
        {
            match code {
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Esc => {
                    return Ok(GameRunResult::Canceled);
                }
                _ => {}
            }
        }
    }
    Ok(game_result)
}

/// Main game loop, running in the main thread
/// Polls for user input events and updates game state accordingly
/// Exits when either the goal is reached, time runs out, or user cancels
fn game_loop(
    mut game_state: GameState,
    user_input_event_rx: Receiver<UserInputEvent>,
    input_recv_timeout: Duration,
    should_stop: &AtomicBool,
    timer_thread_handle: &std::thread::JoinHandle<Result<(), std::io::Error>>,
) -> std::io::Result<GameRunResult> {
    loop {
        // Check if render thread is finished
        if timer_thread_handle.is_finished() {
            tracing::info!("[game loop] Timer thread finished, game result is Timeout");
            // Notify all threads to stop
            should_stop.store(true, std::sync::atomic::Ordering::Release);
            return Ok(GameRunResult::Timeout);
        }

        // Check if goal is reached
        if game_state.current == game_state.goal {
            tracing::info!("[game loop] Goal reached!");
            // Notify all threads to stop
            should_stop.store(true, std::sync::atomic::Ordering::Release);
            return Ok(GameRunResult::GoalReached);
        }

        // Poll user input event
        match user_input_event_rx.recv_timeout(input_recv_timeout) {
            Err(e) => {
                match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        // Skip to next iteration to check exit conditions again
                        continue;
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        // Input thread has exited, set should_stop flag to true
                        // to tell other threads to stop
                        should_stop.store(true, std::sync::atomic::Ordering::Release);
                        return Ok(GameRunResult::Canceled);
                    }
                }
            }
            Ok(event) => match event {
                UserInputEvent::KeyPress(key_event) => {
                    match key_event.code {
                        KeyCode::Esc => {
                            // Game should exit on Esc key
                            should_stop.store(true, std::sync::atomic::Ordering::Release);
                            // Set the game as canceled
                            return Ok(GameRunResult::Canceled);
                        }
                        KeyCode::Left => {
                            game_state.move_player(Direction::Left);
                        }
                        KeyCode::Right => {
                            game_state.move_player(Direction::Right);
                        }
                        KeyCode::Up => {
                            game_state.move_player(Direction::Up);
                        }
                        KeyCode::Down => {
                            game_state.move_player(Direction::Down);
                        }
                        _ => {}
                    };
                }
                UserInputEvent::Resize => todo!("Check resize"),
            },
        }
    }
}

/// Start the game timer, logging remaining time to terminal every tick
/// This function will be run in a separate thread
/// # Arguments
/// * `start_time`: The instant when the game started
/// * `game_run_duration`: The total duration of the game run
/// * `tick_duration`: The duration between each tick to log remaining time
/// * `grid_height`: The height of the maze grid, used to position the log correctly
/// * `should_stop`: Flag to check for exiting early (due to user cancel)
fn start_timer(
    start_time: Instant,
    game_run_duration: Duration,
    tick_duration: Duration,
    should_stop: &AtomicBool,
    ui_event_tx: Sender<UiEvent>,
) -> std::io::Result<()> {
    // Get a new stdout handle
    while start_time.elapsed() < game_run_duration {
        // Check if the timer should stop early
        if should_stop.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        // Clear previous log
        if ui_event_tx.send(UiEvent::LogMessage(None)).is_err() {
            // Receiver has been dropped, exit the thread
            return Ok(());
        }
        let remaining_time = GAME_RUN_DURATION - start_time.elapsed();

        // Set message color based on remaining time

        let msg = if remaining_time <= GAME_RUN_DURATION / 4 {
            format!("Time remain: {}", remaining_time.as_secs())
                .with(Color::Red)
                .attribute(Attribute::Bold)
        } else if remaining_time <= GAME_RUN_DURATION / 2 {
            format!("Time remain: {}", remaining_time.as_secs())
                .with(Color::Yellow)
                .attribute(Attribute::Bold)
        } else {
            format!("Time remain: {}", remaining_time.as_secs())
                .with(Color::Green)
                .attribute(Attribute::Bold)
        };
        // Log new remaining time
        if ui_event_tx.send(UiEvent::LogMessage(Some(msg))).is_err() {
            // Receiver has been dropped, exit the thread
            return Ok(());
        }
        // Sleep for 1 second
        std::thread::sleep(tick_duration);
    }
    Ok(())
}

/// Listen for user input events (key presses and resize)
/// This function runs in a separate thread, and is the only place where user input is read
fn listen_to_user_input(
    user_input_event_tx: Sender<UserInputEvent>,
    event_poll_timeout: Duration,
    should_stop: &AtomicBool,
) -> std::io::Result<()> {
    loop {
        // Check if we should stop
        if should_stop.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }

        // Poll for events with a timeout
        if !event::poll(event_poll_timeout)? {
            // No event available, continue loop to check flags again
            continue;
        }

        // Read the next event
        // We only care about key presses events for now
        let input_event = match event::read()? {
            event::Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                UserInputEvent::KeyPress(key_event)
            }
            event::Event::Resize(_, _) => UserInputEvent::Resize,
            _ => continue, // Ignore other events
        };

        // Should exit input thread on Esc key
        let should_exit = matches!(
            input_event,
            UserInputEvent::KeyPress(event::KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        );

        // Send the input event to the main thread
        if user_input_event_tx.send(input_event).is_err() {
            // Receiver has been dropped, exit the thread
            return Ok(());
        }

        if should_exit {
            tracing::debug!("[input loop] Esc key pressed, exiting");
            return Ok(());
        }
    }
}

pub fn run(stdout: &mut Stdout) -> std::io::Result<()> {
    execute!(
        stdout,
        style::SetAttribute(Attribute::Reverse),
        style::PrintStyledContent("Game Mode\r\n".with(Color::Yellow)),
        style::SetAttribute(Attribute::NoReverse),
    )?;

    // Ask user for maze dimensions
    let (width, height) = match app::ask_maze_dimensions(stdout)? {
        Some(dims) => dims,
        None => {
            return Ok(());
        }
    };

    // Ask user for maze generation algorithm
    let generator = match app::select_from_menu(
        stdout,
        "Select maze generation algorithm (use arrow keys and Enter, or Esc to exit):",
        &app::GENERATORS,
    )? {
        Some(generator) => {
            stdout.execute(style::PrintStyledContent(
                format!("Selected generator: {}\r\n", generator)
                    .with(Color::Green)
                    .attribute(Attribute::Bold),
            ))?;
            generator
        }
        None => {
            return Ok(());
        }
    };

    queue!(
            stdout,
            style::PrintStyledContent(
                "Move your Pacman through the maze using arrow keys to its destination before time's over!\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)
            ),
            style::PrintStyledContent(
                "Controls:\r\n"
                    .with(Color::Yellow)
                    .attribute(Attribute::Bold)
            ),
            style::PrintStyledContent(
                "  ←/→/↑/↓: Step up/down/left/right to control Pacman\r\n".with(Color::Cyan)
            ),
            style::PrintStyledContent("  Esc: Exit game\r\n\r\n".with(Color::Cyan)),
        )?;
    stdout.flush()?;

    tracing::info!(
        "[game] Starting game with maze size {}x{} and generator {:?}",
        width,
        height,
        generator
    );

    loop {
        let game_result = start_game(stdout, width, height, generator)?;
        if game_result == GameRunResult::Canceled {
            break;
        }
        tracing::info!("[game] Game result: {:?} Restarting game...", game_result);
    }
    tracing::info!("[game] Game was canceled by user, exiting...");

    Ok(())
}
