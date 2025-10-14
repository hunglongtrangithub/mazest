mod generators;
mod maze;
mod solvers;

use std::{
    io::{Read, Write},
    sync::mpsc::Receiver,
    time::Duration,
};

use crossterm::{
    cursor, execute, queue,
    terminal::{self, ClearType},
};

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

pub struct App {
    render_refresh_time: Duration,
}

impl Default for App {
    fn default() -> Self {
        Self {
            render_refresh_time: Duration::from_millis(10),
        }
    }
}

impl App {
    pub fn run(&self) -> std::io::Result<()> {
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

        // Clear the terminal screen
        let mut stdout = std::io::stdout();
        terminal::enable_raw_mode()?;
        execute!(stdout, terminal::EnterAlternateScreen)?;
        crossterm::execute!(stdout, terminal::Clear(ClearType::All), cursor::Hide)?;
        let render_interval = Duration::from_millis(100);
        self.spawn_compute_and_render_threads(width, height, generator, solver, render_interval)?;
        execute!(stdout, terminal::LeaveAlternateScreen)?;
        crossterm::execute!(stdout, cursor::Show)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    fn spawn_compute_and_render_threads(
        &self,
        width: u8,
        height: u8,
        generator: generators::Generator,
        solver: solvers::Solver,
        render_interval: Duration,
    ) -> std::io::Result<()> {
        let (grid_event_tx, grid_event_rx) = std::sync::mpsc::channel::<GridEvent>();
        let mut maze = maze::Maze::new(width, height, Some(grid_event_tx));

        // Spawn a thread to listen for grid updates and render the maze
        let render_refresh_time = self.render_refresh_time;
        let render_thread_handle = std::thread::spawn(move || {
            App::render_loop(render_interval, grid_event_rx, render_refresh_time)
        });

        // Generate the maze using the selected algorithm
        generators::generate_maze(&mut maze, generator, None);

        // Solve the maze using the selected algorithm
        let goal_reached = solvers::solve_maze(&mut maze, solver);

        drop(maze); // Ensure maze is dropped and sender is closed

        // Wait for render thread to finish - ignore any errors
        render_thread_handle.join().ok();

        let mut stdout = std::io::stdout();
        if goal_reached {
            print!("Maze solved! Goal reached.\r\n");
        } else {
            print!("No path found to the goal.\r\n");
        }
        print!("Press Enter to exit...\r\n");
        stdout.flush()?;
        let mut buf = [0u8; 1];
        while std::io::stdin().read(&mut buf)? == 1 {
            if buf[0] == b'\r' {
                break;
            }
        }
        Ok(())
    }

    fn process_events(
        event_buffer: &mut Vec<GridEvent>,
        stdout: &mut std::io::Stdout,
        grid_dims: &mut Option<(u16, u16)>,
        render_refresh_time: Duration,
    ) -> std::io::Result<()> {
        let resize_msg = "Terminal size is too small for the maze dimensions to display. Please resize the terminal.";
        for event in event_buffer.drain(..) {
            // print!("Last rendered event: {:?}\r\n", event);
            match event {
                GridEvent::Initial {
                    cell,
                    width,
                    height,
                } => {
                    *grid_dims = Some((width, height));
                    // Check if terminal height and width are sufficient
                    let (term_width, term_height) = terminal::size()?;
                    if term_width < width * GridCell::CELL_WIDTH || term_height < height {
                        print!("{}\r\n", resize_msg);
                        stdout.flush()?;
                        return Ok(());
                    }
                    // Clear screen
                    // Move to top-left corner
                    // Print the whole grid with the specified cell

                    queue!(stdout, crossterm::cursor::MoveTo(0, 0))?;
                    for _y in 0..height {
                        for _x in 0..width {
                            queue!(stdout, crossterm::style::Print(cell))?;
                        }
                        queue!(stdout, crossterm::style::Print("\r\n"))?;
                    }
                    stdout.flush()?;
                }
                GridEvent::Update {
                    coord,
                    old: _old,
                    new,
                } => match grid_dims {
                    Some((width, height)) => {
                        // Move the cursor to the specified coordinate and print the
                        // new cell using the grid dimensions

                        let (term_width, term_height) = terminal::size()?;
                        if term_width < *width * GridCell::CELL_WIDTH || term_height < *height {
                            print!("{}\r\n", resize_msg);
                            stdout.flush()?;
                            return Ok(());
                        }
                        queue!(
                            stdout,
                            crossterm::cursor::MoveTo(coord.0 * GridCell::CELL_WIDTH, coord.1),
                            crossterm::style::Print(new),
                        )?;
                        stdout.flush()?;
                    }
                    // Skip if width and height are not set
                    None => continue,
                },
            }
            // Sleep a bit to simulate rendering time
            std::thread::sleep(render_refresh_time);
        }
        Ok(())
    }

    fn render_loop(
        render_interval: Duration,
        receiver: Receiver<GridEvent>,
        render_refresh_time: Duration,
    ) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        let mut event_buffer = Vec::new();
        let mut last_render = std::time::Instant::now();
        let mut grid_dims = None;

        loop {
            // Block and wait for the next event
            match receiver.recv() {
                Err(_e) => {
                    // Channel disconnected, render the remaining buffer and exit
                    App::process_events(
                        &mut event_buffer,
                        &mut stdout,
                        &mut grid_dims,
                        render_refresh_time,
                    )?;
                    break;
                }
                Ok(event) => {
                    event_buffer.push(event);
                    if last_render.elapsed() >= render_interval {
                        // Reset the timer
                        last_render = std::time::Instant::now();
                        // Render all buffered events
                        App::process_events(
                            &mut event_buffer,
                            &mut stdout,
                            &mut grid_dims,
                            render_refresh_time,
                        )?;
                    }
                }
            }
        }
        // Move cursor below the maze after exiting
        if let Some((_, height)) = grid_dims {
            queue!(stdout, crossterm::cursor::MoveTo(0, height))?;
            stdout.flush()?;
        }
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let app = App::default();
    app.run()
}
