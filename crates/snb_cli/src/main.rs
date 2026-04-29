use std::path::Path;
use std::sync::Arc;

use snb_core::bot::BotInfo;
use snb_core::context::{self, BotContext};
use snb_core::logger::{LogLevel, Logger};
use snb_runtime::bot::Bot;
use snb_runtime::logger::StdoutLogger;
use snb_runtime::plugin_manager::PluginLoader;

/// Load the log level from `configs/bot.toml`, defaulting to `Info`.
fn load_log_level(config_dir: &Path) -> LogLevel {
    let path = config_dir.join("bot.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return LogLevel::Info;
    };
    let Ok(table) = toml::from_str::<toml::Table>(&text) else {
        return LogLevel::Info;
    };
    table
        .get("log_level")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(LogLevel::Info)
}

#[tokio::main]
async fn main() {
    let cwd = std::env::current_dir().unwrap();
    let config_dir = cwd.join("configs");
    let data_root = cwd.join("data");
    let log_level = load_log_level(&config_dir);
    let logger: Arc<dyn Logger> = Arc::new(StdoutLogger::new(log_level));
    logger.info("shinobu", "Starting...");

    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "Shinobu".into(),
        },
        logger,
        config_dir,
        data_root,
    ));

    context::set_bot(bot.clone());

    // Load adapters / plugins from target/debug
    let loader = PluginLoader::new(bot.clone());
    let lib_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    for entry in std::fs::read_dir(&lib_dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        // Match adapter .so/.dylib files (e.g. libsnb_adapter_stdin.so)
        if (name.starts_with("libsnb_") || name.starts_with("snb_"))
            && (name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll"))
        {
            match loader.load_plugin(path.clone()) {
                Ok(_) => log::info!("loaded {}", name),
                Err(e) => log::info!("skip {}: {}", name, e),
            }
        }
    }

    log::info!("Bot '{}' ready", bot.bot_info.name);

    // Start adapters and wait for signal
    bot.run(bot.clone());
    tokio::signal::ctrl_c().await.ok();

    // Graceful shutdown: unload all plugins
    log::info!("Shutting down...");
    for name in bot.list_plugins() {
        log::info!("Unloading plugin: {}", &name);
        bot.unregister_plugin(&name);
    }
    log::info!("Goodbye.");
}
