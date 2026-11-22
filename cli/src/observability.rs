use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initializes the tracing subscriber for observability.
///
/// Verbosity flags:
/// - default: INFO
/// - `-v`: DEBUG
/// - `-vv`: TRACE
pub fn init_tracing(verbose: u8) {
    let filter = match verbose {
        0 => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        1 => EnvFilter::new("debug"),
        _ => EnvFilter::new("trace"),
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).compact())
        .try_init();
}
