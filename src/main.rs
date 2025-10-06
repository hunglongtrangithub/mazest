mod generators;
mod maze;

fn main() {
    // TODO: make the app loop. Take maze dimensions as input and print the generated maze.
    // Pad the maze with walls.
    let mut maze = maze::Maze::new(21, 21);
    generators::randomized_prim(&mut maze);
}
