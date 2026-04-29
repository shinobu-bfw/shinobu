use std::sync::{Arc, OnceLock};

use crate::logger::{LogLevel, Logger};

static BRIDGE_LOGGER: OnceLock<Arc<dyn Logger>> = OnceLock::new();

struct LogBridge;

static LOG_BRIDGE: LogBridge = LogBridge;

impl log::Log for LogBridge {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Filtering is handled by the underlying Logger impl.
        true
    }

    fn log(&self, record: &log::Record) {
        let Some(logger) = BRIDGE_LOGGER.get() else {
            return;
        };
        let level = match record.level() {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Debug,
        };
        let source = record.module_path().unwrap_or("log");
        logger.log(level, source, &record.args().to_string());
    }

    fn flush(&self) {}
}

/// Install a bridge that routes the standard `log` crate's output into the
/// bot's [`Logger`].
///
/// Safe to call multiple times — the bridge is initialized at most once per
/// compilation unit (host binary or plugin `.so`).
pub fn try_init(logger: Arc<dyn Logger>) {
    BRIDGE_LOGGER.get_or_init(|| logger);
    // log::set_logger can only be called once; ignore the error on
    // subsequent calls (which happen in the same compilation unit).
    let _ = log::set_logger(&LOG_BRIDGE);
    log::set_max_level(log::LevelFilter::Debug);
}
