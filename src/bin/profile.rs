use mazest::{app::App, generators::Generator, solvers::Solver};

fn main() -> std::io::Result<()> {
    let app = App::default();

    let mut args = std::env::args();
    args.next(); // Skip executable name
    let num_iters = args.next().and_then(|s| s.parse::<usize>().ok());
    app.profile(u8::MAX, u8::MAX, Solver::Bfs, Generator::Prim, num_iters)?;
    Ok(())
}
