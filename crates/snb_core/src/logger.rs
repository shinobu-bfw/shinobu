/// Severity level for log messages.
///
/// Ordered from least to most severe: `Debug < Info < Warn < Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(format!(
                "unknown log level: {s:?} (expected debug/info/warn/error)"
            )),
        }
    }
}

/// Logger abstraction used by the bot and plugins.
///
/// After [`crate::context::set_bot`] is called, the standard `log` crate macros
/// (`log::info!`, `log::debug!`, etc.) are automatically routed through this
/// logger via the log bridge.
pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, source: &str, message: &str);

    fn debug(&self, source: &str, msg: &str) {
        self.log(LogLevel::Debug, source, msg);
    }
    fn info(&self, source: &str, msg: &str) {
        self.log(LogLevel::Info, source, msg);
    }
    fn warn(&self, source: &str, msg: &str) {
        self.log(LogLevel::Warn, source, msg);
    }
    fn error(&self, source: &str, msg: &str) {
        self.log(LogLevel::Error, source, msg);
    }
}
