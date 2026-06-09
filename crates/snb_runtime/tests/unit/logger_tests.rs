use super::*;

#[test]
fn env_logger_new() {
    let logger = EnvLogger::new();
    logger.log(3, "test", "hello");
}

#[test]
fn env_logger_default() {
    let logger = EnvLogger;
    logger.log(3, "test", "hello");
}

#[test]
fn env_logger_helper_methods() {
    let logger = EnvLogger::new();
    logger.debug("test", "debug message");
    logger.info("test", "info message");
    logger.warn("test", "warn message");
    logger.error("test", "error message");
}

#[test]
fn env_logger_invalid_level() {
    let logger = EnvLogger::new();
    logger.log(99, "test", "should be ignored");
}
