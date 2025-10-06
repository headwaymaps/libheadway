pub mod map_tiles;
pub mod server;

pub use server::HeadwayServer;

#[cfg(target_os = "ios")]
use oslog::OsLogger;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum Error {
    #[error("Failed to create runtime: {0}")]
    Runtime(String),
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
    #[error("Server error: {0}")]
    Serve(String),
    #[error(transparent)]
    PmTiles(#[from] pmtiles::PmtError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(uniffi::Enum)]
pub enum LogLevel {
    /// A level lower than all log levels.
    Off,
    /// Corresponds to the `Error` log level.
    Error,
    /// Corresponds to the `Warn` log level.
    Warn,
    /// Corresponds to the `Info` log level.
    Info,
    /// Corresponds to the `Debug` log level.
    Debug,
    /// Corresponds to the `Trace` log level.
    Trace,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => Self::Off,
            LogLevel::Error => Self::Error,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Debug => Self::Debug,
            LogLevel::Trace => Self::Trace,
        }
    }
}
/// Initializes the logger for the headway library.
/// This should be called once at application startup.
/// On iOS, this will use OSLog with the specified subsystem and category.
#[uniffi::export]
pub fn enable_logging(subsystem: String, log_level: LogLevel) {
    #[cfg(target_os = "ios")]
    {
        OsLogger::new(&subsystem)
            .level_filter(log_level.into())
            .init()
            .ok(); // Ignore error if already initialized
    }

    #[cfg(not(target_os = "ios"))]
    {
        // Fallback for non-iOS platforms (e.g., simulator or tests)
        let _ = subsystem; // Avoid unused variable warning
        env_logger::Builder::from_default_env()
            .filter_level(log_level.into())
            .try_init()
            .ok();
    }
}

uniffi::setup_scaffolding!();
