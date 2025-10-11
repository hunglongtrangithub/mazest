mod generators;
mod maze;
mod solvers;

use crate::generators::generate_maze;

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
    input.clear();
    println!("Select maze generation algorithm:");
    println!("1. {}", generators::Generator::Dfs);
    println!("2. {}", generators::Generator::Prim);
    println!("3. {}", generators::Generator::RecurDiv);
    println!("4. {}", generators::Generator::Kruskal);
    std::io::stdin().read_line(&mut input)?;
    let generator = match input.trim() {
        "1" => generators::Generator::Dfs,
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

    let start = (0, 0);
    let goal = (maze.width() - 1, maze.height() - 1);
    maze[start] = maze::Cell::Path(maze::PathType::Start);
    maze[goal] = maze::Cell::Path(maze::PathType::Goal);

    println!("Select maze solving algorithm:");
    println!("1. DFS");
    input.clear();
    std::io::stdin().read_line(&mut input)?;
    let goal_reached = match input.trim() {
        "1" => solvers::dfs::solve_dfs(&mut maze, start, goal),
        _ => {
            eprintln!("Invalid selection.");
            return Ok(());
        }
    };
    if goal_reached {
        println!("Maze solved! Goal reached.");
    } else {
        println!("No path found to the goal.");
    }
    Ok(())
}
