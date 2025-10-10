mod generators;
mod maze;

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

    let mut maze = maze::Maze::new(width, height);

    // Let user select the algorithm
    input.clear();
    println!("Select maze generation algorithm:");
    println!("1. Randomized Depth-First Search (DFS)");
    println!("2. Prim's Algorithm");
    println!("3. Recursive Division");
    std::io::stdin().read_line(&mut input)?;
    match input.trim() {
        "1" => {
            println!("Generating maze using Randomized DFS...");
            generators::dfs::randomized_dfs(&mut maze);
        }
        "2" => {
            println!("Generating maze using Prim's Algorithm...");
            generators::prim::randomized_prim(&mut maze);
        }
        "3" => {
            println!("Generating maze using Recursive Division...");
            generators::recur_div::recursive_division(&mut maze);
        }
        _ => {
            eprintln!("Invalid selection.");
            return Ok(());
        }
    }
    Ok(())
}
