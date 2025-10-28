use mazest::app::App;

fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber for logging
    let writer = tracing_appender::rolling::never("logs", "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(writer);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(non_blocking)
        .with_line_number(true)
        .init();

    // Set up terminal and run the application
    let mut stdout = std::io::stdout();
    App::setup_terminal(&mut stdout)?;
    let app = App::default();
    let res = app.run(&mut stdout);
    App::restore_terminal(&mut stdout)?;
    res
}
