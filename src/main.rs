use mazest::app::App;

fn main() -> std::io::Result<()> {
    // Initialize logging only in debug mode. Guard is kept alive for the duration of the program.
    #[cfg(debug_assertions)]
    let _guard = {
        // Initialize tracing subscriber for logging
        let writer = tracing_appender::rolling::never("logs", "app.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(writer);
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(non_blocking)
            .with_line_number(true)
            .init();
        tracing::info!("Logging initialized.");
        guard
    };
    let mut app = App::default();
    app.run()
}
