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

/// Convenience extension for registering components without repeating the
/// plugin name. Call [`PluginHelper::for_plugin`] once, then use the short
/// methods.
///
/// ```ignore
/// let ctx = PluginHelper::for_plugin(self.name);
/// ctx.register(EchoCommand);
/// ctx.register_hook(LogHook);
/// ctx.info("loaded!");
/// ```
pub struct PluginHelper<'a> {
    name: &'a str,
    bot: Arc<dyn BotContext>,
}

impl<'a> PluginHelper<'a> {
    pub fn for_plugin(name: &'a str) -> Self {
        Self { name, bot: bot() }
    }

    /// Register a command under this plugin's name.
    pub fn register<C: CommandHandler + 'static>(&self, command: C) {
        self.bot.register_command(self.name, Arc::new(command));
    }

    /// Register a hook under this plugin's name.
    pub fn register_hook<H: Hook + 'static>(&self, hook: H) {
        self.bot.register_hook(self.name, Arc::new(hook));
    }

    /// Register an adapter under this plugin's name.
    pub fn register_adapter<A: Adapter + 'static>(&self, adapter: A) {
        self.bot.register_adapter(self.name, Arc::new(adapter));
    }

    /// Register a message handler under this plugin's name.
    pub fn register_message_handler<H: MessageHandler + 'static>(&self, handler: H) {
        self.bot
            .register_message_handler(self.name, Arc::new(handler));
    }

    /// Register a database driver under this plugin's name.
    pub fn register_database<D: DatabaseDriver + 'static>(&self, db: D) {
        self.bot.register_database(self.name, Arc::new(db));
    }

    /// Get a previously registered database driver by plugin name.
    pub fn get_database(&self, plugin_name: &str) -> Option<Arc<dyn DatabaseDriver>> {
        self.bot.get_database(plugin_name)
    }

    /// Returns this plugin's data directory: `./data/<plugin_name>/`.
    pub fn data_dir(&self) -> PathBuf {
        self.bot.data_dir(self.name)
    }

    /// Log an info-level message under this plugin's name.
    pub fn info(&self, msg: &str) {
        self.bot.logger().info(self.name, msg);
    }

    /// Log a debug-level message under this plugin's name.
    pub fn debug(&self, msg: &str) {
        self.bot.logger().debug(self.name, msg);
    }

    /// Log a warn-level message under this plugin's name.
    pub fn warn(&self, msg: &str) {
        self.bot.logger().warn(self.name, msg);
    }

    /// Log an error-level message under this plugin's name.
    pub fn error(&self, msg: &str) {
        self.bot.logger().error(self.name, msg);
    }

    /// Returns a reference to the underlying [`BotContext`].
    pub fn bot(&self) -> &Arc<dyn BotContext> {
        &self.bot
    }

    /// Load a config file from `./configs/<relative_path>`.
    ///
    /// Returns UTF-8 text — parse it however your plugin needs.
    pub fn load_config(&self, relative_path: &Path) -> io::Result<String> {
        self.bot.load_config(relative_path)
    }

    /// Write a config file under this plugin's namespace (`./configs/<name>/`).
    ///
    /// Atomic write (tmp + rename). Returns `PermissionDenied` if the path
    /// escapes the plugin's directory.
    pub fn write_config(&self, relative_path: &Path, contents: &str) -> io::Result<()> {
        self.bot.write_config(self.name, relative_path, contents)
    }

    /// Get the built-in session manager.
    pub fn get_session_manager(&self) -> Arc<dyn SessionManager> {
        self.bot.get_session_manager()
    }
}
