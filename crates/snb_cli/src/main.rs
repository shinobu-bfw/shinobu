use std::path::{Path, PathBuf};
use std::sync::Arc;

use snb_core::bot::BotInfo;
use snb_core::context::{self, BotContext};
use snb_core::logger::Logger;
use snb_runtime::bot::Bot;
use snb_runtime::logger::EnvLogger;
use snb_runtime::plugin_manager::PluginLoader;

/// Load the log level from `configs/bot.toml`, defaulting to `Info`.
fn load_log_level(config_dir: &Path) -> log::LevelFilter {
    let path = config_dir.join("bot.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return log::LevelFilter::Info;
    };
    let Ok(table) = toml::from_str::<toml::Table>(&text) else {
        return log::LevelFilter::Info;
    };
    table
        .get("log_level")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(log::LevelFilter::Info)
}

/// Initialize env_logger with custom format and level filter
fn init_logger(level: log::LevelFilter) {
    env_logger::Builder::from_default_env()
        .filter_level(level)
        .format_timestamp_millis()
        .init();
}

/// True for files that look like a Shinobu plugin shared library
/// (e.g. `libsnb_adapter_stdin.so`, `snb_plugin_example.dll`).
fn is_plugin_library(name: &str) -> bool {
    (name.starts_with("libsnb_") || name.starts_with("snb_"))
        && (name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll"))
}

#[tokio::main]
async fn main() {
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let runtime_root = resolve_runtime_root(&std::env::current_dir().unwrap(), &exe_dir);
    let config_dir = runtime_root.join("configs");
    let data_root = runtime_root.join("data");
    let log_level = load_log_level(&config_dir);

    // Initialize env_logger
    init_logger(log_level);

    log::info!("Starting Shinobu...");

    // Use EnvLogger which delegates to env_logger
    let logger: Arc<dyn Logger> = Arc::new(EnvLogger::new());

    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "Shinobu".into(),
        },
        logger,
        config_dir,
        data_root,
    ));

    context::set_bot(bot.clone());

    // Load adapters / plugins from two locations, in priority order:
    //   1. the directory holding the executable (Cargo's target/<profile>)
    //   2. ./plugins relative to the working directory (prebuilt drop-ins)
    // A library found in (1) shadows a same-named file in (2); duplicate plugin
    // or command names across *different* files are refused by the loader.
    let loader = PluginLoader::new(bot.clone());
    let plugins_dir = runtime_root.join("plugins");

    let mut seen_files = std::collections::HashSet::new();
    for dir in [exe_dir, plugins_dir] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !is_plugin_library(name) {
                continue;
            }
            if !seen_files.insert(name.to_string()) {
                log::info!("skip {name}: shadowed by higher-priority copy");
                continue;
            }
            match loader.load_plugin(path.clone()) {
                Ok(_) => log::info!("loaded {name}"),
                Err(e) => log::warn!("skip {name}: {e}"),
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

fn resolve_runtime_root(current_dir: &Path, exe_dir: &Path) -> PathBuf {
    find_shinobu_project_root(current_dir)
        .or_else(|| find_shinobu_project_root(exe_dir))
        .unwrap_or_else(|| exe_dir.to_path_buf())
}

fn find_shinobu_project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| is_shinobu_project_root(candidate))
        .map(Path::to_path_buf)
}

fn is_shinobu_project_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && path
            .join("crates")
            .join("snb_cli")
            .join("Cargo.toml")
            .is_file()
        && path
            .join("crates")
            .join("snb_core")
            .join("Cargo.toml")
            .is_file()
}

#[cfg(test)]
#[path = "../tests/unit/main_tests.rs"]
mod main_tests;
