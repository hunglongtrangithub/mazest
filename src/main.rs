mod generators;
mod maze;
mod solvers;

use crate::maze::GridCell;

#[derive(Debug)]
enum GridEvent {
    Initial {
        cell: GridCell,
        width: u16,
        height: u16,
    },
    Update {
        coord: (u16, u16),
        old: GridCell,
        new: GridCell,
    },
}

fn main() -> std::io::Result<()> {
    let mut input = String::new();
    println!("Enter maze dimensions (width height). Maximum size is 255x255:");
    std::io::stdin().read_line(&mut input)?;

    // Parse the input dimensions
    let dims = input
        .split_whitespace()
        .take(2)
        .filter_map(|s| s.parse::<u8>().ok())
        .collect::<Vec<_>>();

    if dims.len() != 2 {
        eprintln!("Please enter two valid numbers for width and height.");
        return Ok(());
    }

    let (width, height) = (dims[0], dims[1]);
    if width < 2 || height < 2 {
        eprintln!("Width and height must be at least 2.");
        return Ok(());
    }

    let (sender, receiver) = std::sync::mpsc::channel::<GridEvent>();
    let mut maze = maze::Maze::new(width, height, Some(sender));

    // Spawn a thread to listen for grid updates and render the maze
    let render_thread_handle = std::thread::spawn(move || {
        let mut event_buffer = Vec::new();
        let mut last_render = std::time::Instant::now();
        let render_interval = std::time::Duration::from_millis(100);
        loop {
            match receiver.recv() {
                Err(e) => {
                    // Channel disconnected, render the remaining buffer and exit
                    for event in event_buffer.drain(..) {
                        println!("Last rendered event: {:?}", event);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    eprintln!("Renderer thread exiting: {}", e);
                    break;
                }
                Ok(event) => {
                    // TODO: Render the maze based on the event variant
                    // For simplicity, we just print the event for now.
                    event_buffer.push(event);
                    if last_render.elapsed() >= render_interval {
                        // Reset the timer
                        last_render = std::time::Instant::now();
                        // Render all buffered events
                        for event in event_buffer.drain(..) {
                            println!("Rendered event: {:?}", event);
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                }
            }
        }
    });

    // Let user select the algorithm
    println!("Select maze generation algorithm:");
    println!("1. {}", generators::Generator::RecurBacktrack);
    println!("2. {}", generators::Generator::Prim);
    println!("3. {}", generators::Generator::RecurDiv);
    println!("4. {}", generators::Generator::Kruskal);
    input.clear();
    std::io::stdin().read_line(&mut input)?;
    let generator = match input.trim() {
        "1" => generators::Generator::RecurBacktrack,
        "2" => generators::Generator::Prim,
        "3" => generators::Generator::RecurDiv,
        "4" => generators::Generator::Kruskal,
        _ => {
            eprintln!("Invalid selection.");
            return Ok(());
        }
    };
    // Generate the maze using the selected algorithm
    generators::generate_maze(&mut maze, generator, None);

    println!("Select maze solving algorithm:");
    println!("1. {}", solvers::Solver::Dfs);
    println!("2. {}", solvers::Solver::Bfs);
    println!("3. {}", solvers::Solver::Dijkstra);
    println!("4. {}", solvers::Solver::AStar);
    input.clear();
    std::io::stdin().read_line(&mut input)?;
    let solver = match input.trim() {
        "1" => solvers::Solver::Dfs,
        "2" => solvers::Solver::Bfs,
        "3" => solvers::Solver::Dijkstra,
        "4" => solvers::Solver::AStar,
        _ => {
            eprintln!("Invalid selection.");
            return Ok(());
        }
    };

    // Solve the maze using the selected algorithm
    let goal_reached = solvers::solve_maze(&mut maze, solver);
    if goal_reached {
        println!("Maze solved! Goal reached.");
    } else {
        println!("No path found to the goal.");
    }
    print!("Press Enter to exit...");
    drop(maze); // Ensure maze is dropped and sender is closed
    // Wait for render thread to finish - ignore any errors
    render_thread_handle.join().ok();

    input.clear();
    std::io::stdin().read_line(&mut input)?;
    Ok(())
}
