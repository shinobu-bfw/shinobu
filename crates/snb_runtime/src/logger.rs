use std::sync::atomic::{AtomicU8, Ordering};

use snb_core::logger::{LogLevel, Logger};

/// Default logger that writes to stdout with level-coloured prefixes.
///
/// Messages below the configured level are dropped.
///
/// ```text
/// [INFO] MyPlugin: plugin loaded
/// [WARN] Bot: plugin not found
/// [ERROR] echo: command failed
/// ```
pub struct StdoutLogger {
    min_level: AtomicU8,
}

impl StdoutLogger {
    pub fn new(level: LogLevel) -> Self {
        Self {
            min_level: AtomicU8::new(level as u8),
        }
    }

    pub fn set_level(&self, level: LogLevel) {
        self.min_level.store(level as u8, Ordering::Relaxed);
    }

    fn min_level(&self) -> LogLevel {
        match self.min_level.load(Ordering::Relaxed) {
            0 => LogLevel::Debug,
            1 => LogLevel::Info,
            2 => LogLevel::Warn,
            3 => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

impl Logger for StdoutLogger {
    fn log(&self, level: LogLevel, source: &str, message: &str) {
        if level < self.min_level() {
            return;
        }
        let prefix = match level {
            LogLevel::Debug => "\x1b[36mDEBUG\x1b[0m", // cyan
            LogLevel::Info => "\x1b[32mINFO \x1b[0m",  // green
            LogLevel::Warn => "\x1b[33mWARN \x1b[0m",  // yellow
            LogLevel::Error => "\x1b[31mERROR\x1b[0m", // red
        };
        println!("[{}] {}: {}", prefix, source, message);
    }
}
