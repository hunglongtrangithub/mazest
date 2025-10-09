mod generators;
mod maze;

fn main() -> std::io::Result<()> {
    let mut input = String::new();
    println!("Enter maze dimensions (width height): ");
    std::io::stdin().read_line(&mut input)?;

    // Parse the input dimensions
    let dims: Vec<u16> = input
        .split_whitespace()
        .take(2)
        .filter_map(|s| s.parse::<u16>().ok())
        .collect();

    if dims.len() != 2 {
        eprintln!("Please enter two valid numbers for width and height.");
        return Ok(());
    }

    let (width, height) = (dims[0], dims[1]);

    let mut maze = maze::Maze::new(width, height);
    generators::prim::randomized_prim(&mut maze);
    Ok(())
}
