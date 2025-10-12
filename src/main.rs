mod generators;
mod maze;
mod solvers;

use crate::{generators::generate_maze, maze::GridCell};

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

    let mut maze = maze::Maze::new(width, height);

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
    generate_maze(&mut maze, generator, None);

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
    Ok(())
}
