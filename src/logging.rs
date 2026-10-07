use anyhow::{Context, Result};
use std::sync::Mutex;
use tracing_subscriber::prelude::*;

pub fn init() -> Result<()> {
    let project = directories::ProjectDirs::from("dev", "castrivo", "Castrivo")
        .context("Could not locate the application data directory for diagnostic logs")?;
    let directory = project.data_local_dir().join("logs");
    std::fs::create_dir_all(&directory).context("Could not create diagnostic log directory")?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("castrivo.log")
        .max_log_files(7)
        .build(&directory)
        .context("Could not create diagnostic logs")?;
    // Synchronous writes keep the last input event available after a native crash.
    let terminal_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "castrivo=info".into());
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(appender))
                .with_filter(tracing_subscriber::EnvFilter::new("castrivo=info")),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr)
                .with_filter(terminal_filter),
        )
        .init();
    tracing::info!(path = %directory.display(), "Persistent diagnostic logs enabled; retain at most seven daily files");
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(location = ?info.location(), backtrace = %std::backtrace::Backtrace::force_capture(),
            "Rust panic; inspect terminal output for the panic message");
        original(info);
    }));
    Ok(())
}
