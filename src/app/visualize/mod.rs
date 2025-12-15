mod history;
mod renderer;

use std::{
    io::{Stdout, Write},
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

use crossterm::{
    ExecutableCommand,
    event::{self, KeyCode},
    queue,
    style::{self, Attribute, Color, Stylize},
};
use rand::Rng;

use crate::{
    app::{
        self,
        visualize::renderer::{Renderer, RendererStatus},
    },
    generators::{Generator, generate_maze},
    maze::{Maze, grid::GridEvent},
    solvers::{Solver, solve_maze},
};

enum UserInputEvent {
    KeyPress(event::KeyEvent),
    Resize,
}

#[derive(Debug)]
pub enum UserActionEvent {
    /// Pause the animation
    Pause,
    /// Resume the animation
    Resume,
    /// Step forward in history or to the future when paused
    Forward,
    /// Step backward in history when paused
    Backward,
    /// Terminal resize
    Resize,
    /// Increase animation speed
    SpeedUp,
    /// Decrease animation speed
    SlowDown,
    /// Cancel rendering
    Cancel,
}

/// Maximum number of grid events to buffer in the channel between compute and render threads
const MAX_EVENTS_IN_CHANNEL_BUFFER: usize = 1000;
/// Timeout for receiving input events, a.k.a. how often to check for render done/cancel flags
const INPUT_RECV_TIMEOUT: Duration = Duration::from_millis(100);
/// Timeout for polling input events in the input thread, a.k.a.
/// how often to check for render done/cancel flags
const USER_INPUT_EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(100);
/// Maximum number of grid events to keep for history browsing when paused or grid state
/// recovery
const MAX_HISTORY_GRID_EVENTS: usize = 100;

/// Entry point of the visualizer app
pub fn run(stdout: &mut Stdout) -> std::io::Result<()> {
    queue!(
        stdout,
        style::SetAttribute(Attribute::Reverse),
        style::PrintStyledContent("Visualization Mode\r\n".with(Color::Yellow)),
        style::SetAttribute(Attribute::NoReverse),
    )?;
    stdout.flush()?;

    // Ask user for maze dimensions
    let (width, height) = match app::ask_maze_dimensions(stdout)? {
        Some(dims) => dims,
        None => {
            return Ok(());
        }
    };

    // Ask user for maze generation algorithm
    let mut generator = match app::select_from_menu(
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

    // Ask user for maze solving algorithm
    let mut solver = match app::select_from_menu(
        stdout,
        "Select maze solving algorithm (use arrow keys and Enter, or Esc to exit):",
        &app::SOLVERS,
    )? {
        Some(solver) => {
            stdout.execute(style::PrintStyledContent(
                format!("Selected solver: {}\r\n", solver)
                    .with(Color::Green)
                    .attribute(Attribute::Bold),
            ))?;
            solver
        }
        None => {
            return Ok(());
        }
    };

    queue!(
        stdout,
        style::PrintStyledContent(
            "Controls:\r\n"
                .with(Color::Yellow)
                .attribute(Attribute::Bold)
        ),
        style::PrintStyledContent("  Enter: Pause/Resume animation\r\n".with(Color::Cyan)),
        style::PrintStyledContent("  ←/→: Step backward/forward when paused\r\n".with(Color::Cyan)),
        style::PrintStyledContent("  ↑/↓: Speed up/slow down animation\r\n".with(Color::Cyan)),
        style::PrintStyledContent("  Esc: Exit\r\n\r\n".with(Color::Cyan)),
    )?;

    stdout.flush()?;
    // Ask if user wants to loop generation and solving
    let loop_animation = match app::select_from_menu(
        stdout,
        "Loop maze generation and solving? Will randomize generator & solver combination. (use arrow keys and Enter, or Esc to exit):",
        &["Yes", "No"],
    )? {
        Some(choice) => choice == "Yes",
        None => {
            return Ok(());
        }
    };

    // Flag to indicate other threads should stop. Set to true by the main thread on Esc key event.
    let should_stop = Arc::new(AtomicBool::new(false));

    let (user_input_event_tx, user_input_event_rx) = std::sync::mpsc::channel::<UserInputEvent>();
    let should_stop_for_input = should_stop.clone();
    // Spawn a thread to listen for user input
    let input_thread_handle = std::thread::spawn(move || -> std::io::Result<()> {
        listen_to_user_input(
            user_input_event_tx,
            USER_INPUT_EVENT_POLL_TIMEOUT,
            &should_stop_for_input,
        )
    });

    let (grid_event_tx, grid_event_rx) =
        std::sync::mpsc::sync_channel::<GridEvent>(MAX_EVENTS_IN_CHANNEL_BUFFER);
    let (user_action_event_tx, user_action_event_rx) =
        std::sync::mpsc::channel::<UserActionEvent>();

    // Spawn a thread to listen for grid updates and render the maze
    let render_cancel_for_render = should_stop.clone();
    let render_thread_handle = std::thread::spawn(move || {
        Renderer::new(MAX_HISTORY_GRID_EVENTS, Some((width, height))).render(
            grid_event_rx,
            user_action_event_rx,
            &render_cancel_for_render,
        )
    });

    // Spawn a thread to generate maze and solve it
    let render_cancel_for_compute = should_stop.clone();
    let compute_thread_handle = std::thread::spawn(move || -> bool {
        if !loop_animation {
            return compute(width, height, grid_event_tx, generator, solver);
        }
        // Looping mode: randomly select generator and solver each iteration
        let mut rng = rand::rng();
        loop {
            let goal_reached = compute(width, height, grid_event_tx.clone(), generator, solver);
            // Check if rendering was cancelled
            if render_cancel_for_compute.load(std::sync::atomic::Ordering::Acquire) {
                tracing::info!("Compute thread detected render cancel, exiting loop");
                return goal_reached;
            }
            // Randomly select new generator and solver combination for next iteration
            (generator, solver) = app::COMBOS[rng.random_range(0..app::COMBOS.len())];
        }
    });

    // Main thread loop to listen for user input events during rendering
    let completed = app_loop(
        user_input_event_rx,
        user_action_event_tx,
        INPUT_RECV_TIMEOUT,
        render_thread_handle,
        should_stop,
    )?;

    // Wait for input thread to finish
    input_thread_handle.join().expect("Input thread panicked")?;

    // Wait for compute thread to finish
    let goal_reached = compute_thread_handle
        .join()
        .expect("Compute thread panicked");

    if let RendererStatus::Cancelled = completed {
        tracing::info!("Rendering was cancelled by user.");
        return Ok(());
    }

    let msg = if goal_reached {
        "Path found! "
    } else {
        "No path found. "
    };
    stdout.execute(style::PrintStyledContent(
        msg.with(Color::Green).attribute(Attribute::Bold),
    ))?;

    stdout.execute(style::PrintStyledContent(
        "Press Esc to exit...\r"
            .with(Color::Blue)
            .attribute(Attribute::Bold),
    ))?;

    // Wait for user to press Esc
    app::wait_for_keypress(KeyCode::Esc)?;
    Ok(())
}

/// App loop after starting input and render threads
fn app_loop(
    user_input_event_rx: Receiver<UserInputEvent>,
    user_action_event_tx: Sender<UserActionEvent>,
    input_recv_timeout: Duration,
    render_thread_handle: std::thread::JoinHandle<Result<RendererStatus, std::io::Error>>,
    should_stop: Arc<AtomicBool>,
) -> std::io::Result<RendererStatus> {
    tracing::info!("Started main app loop");
    // Flag to indicate if the animation is currently paused
    let mut is_paused = false;
    loop {
        // Check if render is done
        if render_thread_handle.is_finished() {
            // Signal threads to stop
            should_stop.store(true, std::sync::atomic::Ordering::Release);
            break;
        }

        let event = match user_input_event_rx.recv_timeout(input_recv_timeout) {
            Err(e) => {
                match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        // Skip to next iteration to check render_done again
                        continue;
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        // Input thread has exited, break the loop
                        break;
                    }
                }
            }
            Ok(event) => match event {
                UserInputEvent::KeyPress(key_event) => {
                    match key_event.code {
                        // Exit on Esc key
                        KeyCode::Esc => {
                            tracing::debug!("[app loop] Esc key pressed, notifying renderer");
                            // Error only happens if user_input_event_rx is dropped, which
                            // means render thread has exited already
                            user_action_event_tx.send(UserActionEvent::Cancel).ok();
                            should_stop.store(true, std::sync::atomic::Ordering::Release);
                            break;
                        }
                        KeyCode::Enter => {
                            // Toggle pause/resume on Enter key
                            let event = if is_paused {
                                UserActionEvent::Resume
                            } else {
                                UserActionEvent::Pause
                            };
                            // Toggle pause state
                            is_paused = !is_paused;
                            Some(event)
                        }
                        KeyCode::Left if is_paused => {
                            // Step backward when paused
                            Some(UserActionEvent::Backward)
                        }
                        KeyCode::Right if is_paused => {
                            // Step forward when paused
                            Some(UserActionEvent::Forward)
                        }
                        KeyCode::Up => {
                            // Speed up animation
                            Some(UserActionEvent::SpeedUp)
                        }
                        KeyCode::Down => {
                            // Slow down animation
                            Some(UserActionEvent::SlowDown)
                        }
                        _ => None, // Ignore other keys
                    }
                }
                UserInputEvent::Resize => Some(UserActionEvent::Resize),
            },
        };

        // Send the user action event to the render thread
        if let Some(event) = event
            && user_action_event_tx.send(event).is_err()
        {
            // Render thread has exited
            break;
        }
    }
    // The user_input_event_rx and user_action_event_tx are dropped here
    tracing::info!("Exiting main app loop");

    // Wait for render thread to finish and get its status
    render_thread_handle.join().expect("Render thread panicked")
}

/// Listen for user input events (key presses and resize)
/// This function runs in a separate thread, and is the only place where user input is read
fn listen_to_user_input(
    user_input_event_tx: Sender<UserInputEvent>,
    event_poll_timeout: Duration,
    should_stop: &AtomicBool,
) -> std::io::Result<()> {
    loop {
        // Check if this thread should exit
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

/// Generate and solve the maze
/// Returns whether the goal was reached
fn compute(
    width: u8,
    height: u8,
    grid_event_tx: std::sync::mpsc::SyncSender<GridEvent>,
    generator: Generator,
    solver: Solver,
) -> bool {
    let mut maze = Maze::new(width, height, Some(grid_event_tx));
    // Generate the maze using the selected algorithm
    generate_maze(&mut maze, generator, None);

    // Solve the maze using the selected algorithm
    solve_maze(&mut maze, solver)
    // Maze is dropped here, as well as the grid_event_tx sender
}
