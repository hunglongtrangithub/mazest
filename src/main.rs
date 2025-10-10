use crate::generators::generate_maze;

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
    Ok(())
}
