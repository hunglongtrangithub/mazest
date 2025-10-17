mod app;
mod generators;
mod maze;
mod solvers;

use crate::app::App;

fn main() -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    App::setup_terminal(&mut stdout)?;
    let app = App::default();
    let res = app.run(&mut stdout);
    App::restore_terminal(&mut stdout)?;
    res
}
