use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};

use crate::adapter::Adapter;
use crate::bot::BotInfo;
use crate::command::CommandHandler;
use crate::database::DatabaseDriver;
use crate::event::Event;
use crate::hook::Hook;
use crate::logger::Logger;
use crate::message_handler::MessageHandler;
use crate::plugin::{PluginCell, PluginInfo};
use crate::session::SessionManager;

// -- Global bot context -------------------------------------------------------

/// Each compilation unit (host binary and each dynamically loaded `.so`)
/// gets its own copy of this static. The host calls [`set_bot`] at startup;
/// each plugin calls [`set_bot`] in its [`SnbPlugin::on_load`].
static BOT: LazyLock<RwLock<Option<Arc<dyn BotContext>>>> = LazyLock::new(|| RwLock::new(None));

/// Set the global bot instance for the current compilation unit.
///
/// Also installs the `log` crate bridge so that third-party libraries using
/// `log::info!()` / `log::debug!()` / etc. are forwarded through the bot's
/// logger. The bridge is initialized at most once per compilation unit.
pub fn set_bot(ctx: Arc<dyn BotContext>) {
    crate::log_bridge::try_init(ctx.logger());
    *BOT.write().unwrap() = Some(ctx);
}

/// Returns the global bot instance.
///
/// # Panics
///
/// Panics if [`set_bot`] has not been called in the current compilation unit.
pub fn bot() -> Arc<dyn BotContext> {
    BOT.read()
        .unwrap()
        .as_ref()
        .expect("bot not initialized — call set_bot() in on_load")
        .clone()
}

// -- BotContext trait ---------------------------------------------------------

/// The bidirectional channel between the bot runtime and plugins.
///
/// Plugins receive an `Arc<dyn BotContext>` in [`crate::plugin::SnbPlugin::on_load`] and use
/// it to register commands, hooks, emit events, and access other plugins.
pub trait BotContext: Send + Sync {
    // -- Logger --

    /// Returns the bot's logger.
    ///
    /// Plugins and adapters should route all output through this logger
    /// instead of using `println!` or the `log` crate directly.
    fn logger(&self) -> Arc<dyn Logger>;
    // -- Bot identity --

    fn get_me(&self) -> BotInfo;

    // -- Event dispatch --

    /// Push an event into the bot's dispatch loop.
    ///
    /// This is a synchronous dispatch — hooks and commands run inline.
    fn emit_event(&self, event: Event);

    // -- Plugin management --

    /// Register a loaded plugin cell.
    fn register_plugin(&self, plugin: PluginCell);
    fn unregister_plugin(&self, name: &str) -> bool;
    fn list_plugins(&self) -> Vec<String>;

    /// Returns a snapshot of the named plugin's identity.
    ///
    /// ```ignore
    /// let info = bot.get_plugin("echo")?;
    /// println!("{} v{}", info.name, info.version);
    /// ```
    fn get_plugin(&self, name: &str) -> Option<PluginInfo>;

    // -- Component registration (called by plugins during on_load) --

    fn register_command(&self, plugin_name: &str, command: Arc<dyn CommandHandler>);
    fn register_hook(&self, plugin_name: &str, hook: Arc<dyn Hook>);

    /// Register an adapter that runs on a dedicated OS thread.
    fn register_adapter(&self, plugin_name: &str, adapter: Arc<dyn Adapter>);

    /// Register a handler for non-command messages.
    fn register_message_handler(&self, plugin_name: &str, handler: Arc<dyn MessageHandler>);

    /// Register a database driver under this plugin's name.
    fn register_database(&self, plugin_name: &str, db: Arc<dyn DatabaseDriver>);

    /// Get a previously registered database driver by plugin name.
    fn get_database(&self, plugin_name: &str) -> Option<Arc<dyn DatabaseDriver>>;

    /// Returns the data directory for the given plugin: `./data/<plugin_name>/`.
    ///
    /// Creates the directory if it doesn't exist. Each plugin can only
    /// access its own data directory.
    fn data_dir(&self, plugin_name: &str) -> PathBuf;

    // -- Config --

    /// Load a config file from the `./configs/` directory.
    ///
    /// `relative_path` is resolved relative to the bot's config directory
    /// (typically `./configs/`). Returns the file contents as UTF-8 text so
    /// that each plugin can parse the format it expects (TOML, JSON, YAML, …).
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the file does not exist or cannot be read,
    /// or if the file is not valid UTF-8.
    fn load_config(&self, relative_path: &Path) -> io::Result<String>;

    /// Write a config file under `./configs/<plugin_name>/`.
    ///
    /// The file is written atomically (tmp + rename) and the path is
    /// confined to the calling plugin's config namespace. Writing outside
    /// `configs/<plugin_name>/` (e.g. `../other_plugin/...`) returns
    /// `PermissionDenied`.
    ///
    /// On Unix the file is created with mode `0o644`.
    ///
    /// # Errors
    ///
    /// Returns `PermissionDenied` if the resolved path escapes the plugin's
    /// config directory, or an `io::Error` on I/O failure.
    fn write_config(
        &self,
        plugin_name: &str,
        relative_path: &Path,
        contents: &str,
    ) -> io::Result<()>;

    // -- Session management --

    /// Get the built-in session manager for temporary in-memory sessions.
    fn get_session_manager(&self) -> Arc<dyn SessionManager>;
}

/// Register every command, hook, message handler, adapter, and database driver
/// that this plugin declared via the `#[command]` / `#[hook]` /
/// `#[message_handler]` / `#[adapter]` / `#[database]` macros, under
/// `plugin_name`. Call once in `on_load`, after [`set_bot`].
///
/// Each macro emits an `inventory::submit!`; this walks those collections via
/// the global [`bot`] and registers each item. This must live in `snb_core`
/// (which is statically linked into every plugin `cdylib`) so `inventory::iter`
/// reads *this* plugin's submissions — moving it behind a `dyn BotContext` call
/// would iterate the host's (empty) registry instead.
///
/// ```ignore
/// fn on_load(&mut self, ctx: Arc<dyn BotContext>) {
///     snb_core::context::set_bot(ctx);
///     snb_core::context::register_all(self.name());
///     log::info!("loaded!");
/// }
/// ```
pub fn register_all(plugin_name: &str) {
    let bot = bot();
    for reg in inventory::iter::<crate::registry::CommandRegistration> {
        bot.register_command(plugin_name, (reg.factory)());
    }
    for reg in inventory::iter::<crate::registry::HookRegistration> {
        bot.register_hook(plugin_name, (reg.factory)());
    }
    for reg in inventory::iter::<crate::registry::MessageHandlerRegistration> {
        bot.register_message_handler(plugin_name, (reg.factory)());
    }
    for reg in inventory::iter::<crate::registry::AdapterRegistration> {
        bot.register_adapter(plugin_name, (reg.factory)());
    }
    for reg in inventory::iter::<crate::registry::DatabaseRegistration> {
        bot.register_database(plugin_name, (reg.factory)());
    }
}
