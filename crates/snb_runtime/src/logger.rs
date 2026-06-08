use log::Level;
use snb_core::logger::Logger;

/// Logger that delegates to the standard `log` crate, allowing use of
/// `env_logger` or other log implementations.
///
/// This is the **recommended logger** for production use. It bridges the custom
/// Logger trait to the standard log facade, enabling flexible configuration via
/// `RUST_LOG` environment variable and `env_logger::Builder`.
///
/// # Example
///
/// ```no_run
/// use snb_runtime::logger::EnvLogger;
/// use snb_core::logger::Logger;
/// use std::sync::Arc;
///
/// // Initialize env_logger first
/// env_logger::Builder::from_default_env()
///     .filter_level(log::LevelFilter::Info)
///     .init();
///
/// // Then use EnvLogger
/// let logger: Arc<dyn Logger> = Arc::new(EnvLogger::new());
/// ```
pub struct EnvLogger;

impl Default for EnvLogger {
    fn default() -> Self {
        Self
    }
}

impl EnvLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Logger for EnvLogger {
    fn log(&self, level: u8, source: &str, message: &str) {
        let level = match level {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            5 => Level::Trace,
            _ => return,
        };
        log::log!(target: source, level, "{}", message);
    }
}
