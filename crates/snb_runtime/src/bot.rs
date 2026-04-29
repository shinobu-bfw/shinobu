use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::session::InMemorySessionManager;
use snb_core::adapter::Adapter;
use snb_core::bot::BotInfo;
use snb_core::command::{CommandContext, CommandHandler};
use snb_core::context::BotContext;
use snb_core::database::DatabaseDriver;
use snb_core::event::*;
use snb_core::hook::{Hook, HookType};
use snb_core::logger::Logger;
use snb_core::message_handler::MessageHandler;
use snb_core::plugin::{PluginCell, PluginInfo};
use snb_core::session::SessionManager;

struct CommandEntry {
    plugin_name: String,
    command: Arc<dyn CommandHandler>,
}

struct HookEntry {
    plugin_name: String,
    hook: Arc<dyn Hook>,
}

struct MessageHandlerEntry {
    plugin_name: String,
    handler: Arc<dyn MessageHandler>,
}

struct AdapterEntry {
    plugin_name: String,
    adapter: Arc<dyn Adapter>,
}

#[allow(dead_code)]
struct DatabaseDriverEntry {
    plugin_name: String,
    db: Arc<dyn DatabaseDriver>,
}

/// Phase indicator passed to [`Bot::run_hooks`].
///
/// Distinct from [`HookType`] because dispatch needs to ask "what phase
/// are we in?" without enumerating every possible hook variant. Only
/// these three phases occur during dispatch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HookPhase {
    BeforeCommand,
    AfterCommand,
    Main,
}

/// The concrete bot runtime.
///
/// Implements [`BotContext`] and owns all registered plugins, commands, hooks,
/// adapters, message handlers, and database drivers. Created once at startup
/// and shared as `Arc<Bot>`.
pub struct Bot {
    pub bot_info: BotInfo,
    logger: Arc<dyn Logger>,
    config_dir: PathBuf,
    data_root: PathBuf,
    plugins: RwLock<HashMap<String, PluginCell>>,
    plugin_infos: RwLock<HashMap<String, PluginInfo>>,
    commands: RwLock<HashMap<String, CommandEntry>>,
    /// alias -> canonical command name
    aliases: RwLock<HashMap<String, String>>,
    hooks: RwLock<Vec<HookEntry>>,
    message_handlers: RwLock<Vec<MessageHandlerEntry>>,
    adapters: Mutex<Vec<AdapterEntry>>,
    databases: RwLock<HashMap<String, DatabaseDriverEntry>>,
    session_manager: Arc<dyn SessionManager>,
}

impl Bot {
    pub fn new(
        bot_info: BotInfo,
        logger: Arc<dyn Logger>,
        config_dir: PathBuf,
        data_root: PathBuf,
    ) -> Self {
        Self {
            bot_info,
            logger,
            config_dir,
            data_root,
            plugins: RwLock::new(HashMap::new()),
            plugin_infos: RwLock::new(HashMap::new()),
            commands: RwLock::new(HashMap::new()),
            aliases: RwLock::new(HashMap::new()),
            hooks: RwLock::new(Vec::new()),
            message_handlers: RwLock::new(Vec::new()),
            adapters: Mutex::new(Vec::new()),
            databases: RwLock::new(HashMap::new()),
            session_manager: Arc::new(InMemorySessionManager::new(
                100,
                std::time::Duration::from_secs(1800),
            )),
        }
    }

    /// Resolve `relative_path` under `config_dir` and ensure it stays inside.
    ///
    /// Returns `PermissionDenied` if the canonical path escapes the config root.
    fn safe_config_path(&self, relative_path: &Path) -> io::Result<PathBuf> {
        self.safe_path_under(&self.config_dir, relative_path)
    }

    /// Resolve `relative_path` under `root` and ensure the result is inside `root`.
    ///
    /// Rejects `..` components and checks the canonicalized path stays within
    /// `root`. The `root` directory is created if it doesn't exist.
    fn safe_path_under(&self, root: &Path, relative_path: &Path) -> io::Result<PathBuf> {
        // Reject any ".." or non-normal components up front.
        for comp in relative_path.components() {
            match comp {
                std::path::Component::Normal(_) => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "path traversal: {} contains disallowed component {comp:?}",
                            relative_path.display()
                        ),
                    ));
                }
            }
        }

        // Ensure root exists so we can canonicalize it.
        std::fs::create_dir_all(root)?;
        let root_canonical = root.canonicalize().map_err(|e| {
            io::Error::other(format!("cannot resolve root {}: {e}", root.display()))
        })?;

        let full_path = root.join(relative_path);

        // Canonicalize the final path (or parent if the file doesn't exist).
        let canonical = if full_path.exists() {
            full_path.canonicalize().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("cannot resolve {}: {e}", full_path.display()),
                )
            })?
        } else {
            let parent = full_path.parent().unwrap_or(&full_path);
            let parent_canon = parent.canonicalize().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("cannot resolve parent {}: {e}", parent.display()),
                )
            })?;
            parent_canon.join(full_path.file_name().unwrap_or_default())
        };

        if !canonical.starts_with(&root_canonical) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "path traversal: {} escapes {}",
                    relative_path.display(),
                    root.display()
                ),
            ));
        }
        Ok(canonical)
    }

    /// Start all registered adapters on dedicated OS threads.
    ///
    /// Each adapter receives a clone of `bot_ctx` so it can call
    /// [`BotContext::emit_event`] directly.
    pub fn run(&self, bot_ctx: Arc<dyn BotContext>) {
        let adapters = self.adapters.lock().unwrap().drain(..).collect::<Vec<_>>();
        for entry in adapters {
            let name = entry.plugin_name.clone();
            let ctx = bot_ctx.clone();
            self.logger
                .info("Bot", &format!("starting adapter '{}'", name));
            std::thread::Builder::new()
                .name(format!("adapter-{}", name))
                .spawn(move || {
                    entry.adapter.run(ctx);
                })
                .expect("failed to spawn adapter thread");
        }
    }

    fn dispatch_command(&self, event: &mut Event) {
        let Some(parsed) = event.command.clone() else {
            self.logger
                .warn("Bot", "Command event missing parsed command payload");
            return;
        };

        let cmd = match self.lookup_command(&parsed.cmd) {
            Some(c) => c,
            None => {
                self.logger.debug(
                    "Bot",
                    &format!("no command registered for '{}'", parsed.cmd),
                );
                return;
            }
        };

        self.run_hooks(HookPhase::BeforeCommand, event);

        // Re-read args after BeforeCommand hooks (they may have rewritten them).
        let args = event
            .command
            .as_ref()
            .map(|c| c.args.clone())
            .unwrap_or_default();
        let ctx = CommandContext { event, args: &args };
        if let Err(e) = cmd.execute(&ctx) {
            self.logger.error(&parsed.cmd, &format!("{:#}", e));
        }

        self.run_hooks(HookPhase::AfterCommand, event);
    }

    fn lookup_command(&self, name: &str) -> Option<Arc<dyn CommandHandler>> {
        let cmds = self.commands.read().unwrap();
        if let Some(entry) = cmds.get(name) {
            return Some(entry.command.clone());
        }
        // Hold cmds lock through alias lookup to avoid re-acquiring.
        let aliases = self.aliases.read().unwrap();
        let canonical = aliases.get(name)?;
        cmds.get(canonical).map(|e| e.command.clone())
    }

    fn run_message_handlers(&self, event: &Event) {
        let handlers = self.message_handlers.read().unwrap();
        for entry in handlers.iter() {
            if let Err(e) = entry.handler.handle(event) {
                self.logger.error(entry.handler.name(), &format!("{:#}", e));
            }
        }
    }

    fn run_hooks(&self, phase: HookPhase, event: &mut Event) {
        let cmd_name = event.command.as_ref().map(|c| c.cmd.as_str());
        let hooks: Vec<Arc<dyn Hook>> = {
            let hooks = self.hooks.read().unwrap();
            hooks
                .iter()
                .filter(|e| match (e.hook.hook_type(), phase) {
                    (HookType::BeforeCommand, HookPhase::BeforeCommand) => true,
                    (HookType::AfterCommand, HookPhase::AfterCommand) => true,
                    (HookType::BeforeNamedCommand(n), HookPhase::BeforeCommand) => {
                        Some(n.as_str()) == cmd_name
                    }
                    (HookType::AfterNamedCommand(n), HookPhase::AfterCommand) => {
                        Some(n.as_str()) == cmd_name
                    }
                    (HookType::Event(et), HookPhase::Main) => et == event.event_type,
                    (HookType::All, HookPhase::Main) => true,
                    _ => false,
                })
                .map(|e| e.hook.clone())
                .collect()
        };
        for hook in hooks {
            if let Err(e) = hook.execute(event) {
                self.logger.error(hook.name(), &format!("{:#}", e));
            }
        }
    }
}

impl BotContext for Bot {
    fn logger(&self) -> Arc<dyn Logger> {
        self.logger.clone()
    }

    fn get_me(&self) -> BotInfo {
        self.bot_info.clone()
    }

    fn get_plugin(&self, name: &str) -> Option<PluginInfo> {
        self.plugin_infos.read().unwrap().get(name).cloned()
    }

    fn list_plugins(&self) -> Vec<String> {
        self.plugin_infos.read().unwrap().keys().cloned().collect()
    }

    fn emit_event(&self, mut event: Event) {
        // Every event goes through the Main hook phase exactly once.
        self.run_hooks(HookPhase::Main, &mut event);

        match event.event_type {
            EventType::Command => {
                self.dispatch_command(&mut event);
            }
            EventType::Message => {
                self.run_message_handlers(&event);
            }
            _ => {}
        }

        // Dispatch to plugin on_event handlers.
        // Use read lock — on_event takes &self so no mutable access needed.
        let plugins = self.plugins.read().unwrap();
        match &event.receiver {
            Some(target) => {
                if let Some(cell) = plugins.get(target.as_str()) {
                    cell.on_event(&event);
                }
            }
            None => {
                for cell in plugins.values() {
                    cell.on_event(&event);
                }
            }
        }
    }

    fn register_plugin(&self, plugin: PluginCell) {
        let info = PluginInfo::from_plugin(&*plugin);
        let plugin_name = info.name.clone();

        self.plugin_infos
            .write()
            .unwrap()
            .insert(plugin_name.clone(), info);
        self.plugins
            .write()
            .unwrap()
            .insert(plugin_name.clone(), plugin);

        // Emit after releasing locks to avoid deadlock if a handler
        // tries to register another plugin.
        self.emit_event(Event::typed(EventType::PluginLoaded, "Bot", plugin_name));
    }

    fn unregister_plugin(&self, name: &str) -> bool {
        // Take the cell out of the map but DON'T drop it yet — every
        // trait object registered by this plugin has its vtable in the
        // plugin's `.so`, and dropping `cell` will eventually `dlclose`
        // it. We must drop those Arcs first while the dylib is still
        // mapped.
        let mut cell = match self.plugins.write().unwrap().remove(name) {
            Some(c) => c,
            None => {
                self.logger
                    .warn("Bot", &format!("plugin '{}' not found for unloading", name));
                return false;
            }
        };

        // User's on_unload runs while the dylib is still mapped.
        cell.on_unload();

        // Drop everything that points into the dylib's vtables.
        let removed_canonicals: Vec<String> = {
            let mut cmds = self.commands.write().unwrap();
            let to_remove: Vec<String> = cmds
                .iter()
                .filter(|(_, e)| e.plugin_name == name)
                .map(|(k, _)| k.clone())
                .collect();
            for k in &to_remove {
                cmds.remove(k);
            }
            to_remove
        };
        if !removed_canonicals.is_empty() {
            self.aliases
                .write()
                .unwrap()
                .retain(|_, canonical| !removed_canonicals.contains(canonical));
        }

        self.hooks
            .write()
            .unwrap()
            .retain(|e| e.plugin_name != name);
        self.message_handlers
            .write()
            .unwrap()
            .retain(|e| e.plugin_name != name);
        self.adapters
            .lock()
            .unwrap()
            .retain(|e| e.plugin_name != name);
        self.databases.write().unwrap().remove(name);

        self.plugin_infos.write().unwrap().remove(name);

        // Now safe: every Arc<dyn …> from this plugin has been dropped, so
        // the dylib can unload (PluginCell::drop runs destroy_plugin and
        // then `_keep_alive: Library` drops, calling dlclose).
        drop(cell);

        self.logger
            .info("Bot", &format!("plugin '{}' unloaded", name));
        // Emit after dropping cell and releasing all locks.
        self.emit_event(Event::typed(EventType::PluginUnloaded, "Bot", name));
        true
    }

    fn register_command(&self, plugin_name: &str, command: Arc<dyn CommandHandler>) {
        let cmd_name = command.name().to_string();
        let alias_list: Vec<String> = command.aliases().iter().map(|s| s.to_string()).collect();
        self.logger
            .info(plugin_name, &format!("registered command '{}'", cmd_name));

        if !alias_list.is_empty() {
            let mut aliases = self.aliases.write().unwrap();
            for a in &alias_list {
                aliases.insert(a.clone(), cmd_name.clone());
            }
        }
        self.commands.write().unwrap().insert(
            cmd_name,
            CommandEntry {
                plugin_name: plugin_name.to_string(),
                command,
            },
        );
    }

    fn register_hook(&self, plugin_name: &str, hook: Arc<dyn Hook>) {
        self.logger
            .info(plugin_name, &format!("registered hook '{}'", hook.name()));
        let mut hooks = self.hooks.write().unwrap();
        hooks.push(HookEntry {
            plugin_name: plugin_name.to_string(),
            hook,
        });
        hooks.sort_by_key(|e| e.hook.priority());
    }

    fn register_adapter(&self, plugin_name: &str, adapter: Arc<dyn Adapter>) {
        self.logger.info(plugin_name, "registered adapter");
        self.adapters.lock().unwrap().push(AdapterEntry {
            plugin_name: plugin_name.to_string(),
            adapter,
        });
    }

    fn register_message_handler(&self, plugin_name: &str, handler: Arc<dyn MessageHandler>) {
        self.logger.info(
            plugin_name,
            &format!("registered message handler '{}'", handler.name()),
        );
        let mut handlers = self.message_handlers.write().unwrap();
        handlers.push(MessageHandlerEntry {
            plugin_name: plugin_name.to_string(),
            handler,
        });
        handlers.sort_by_key(|e| e.handler.priority());
    }

    fn register_database(&self, plugin_name: &str, db: Arc<dyn DatabaseDriver>) {
        self.logger
            .info(plugin_name, &format!("registered database '{}'", db.name()));
        self.databases.write().unwrap().insert(
            plugin_name.to_string(),
            DatabaseDriverEntry {
                plugin_name: plugin_name.to_string(),
                db,
            },
        );
    }

    fn get_database(&self, plugin_name: &str) -> Option<Arc<dyn DatabaseDriver>> {
        self.databases
            .read()
            .unwrap()
            .get(plugin_name)
            .map(|e| e.db.clone())
    }

    fn data_dir(&self, plugin_name: &str) -> PathBuf {
        let dir = self.data_root.join(plugin_name);
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn load_config(&self, relative_path: &Path) -> io::Result<String> {
        let full_path = self.safe_config_path(relative_path)?;
        std::fs::read_to_string(&full_path)
    }

    fn write_config(
        &self,
        plugin_name: &str,
        relative_path: &Path,
        contents: &str,
    ) -> io::Result<()> {
        let plugin_root = self.config_dir.join(plugin_name);
        let full_path = self.safe_path_under(&plugin_root, relative_path)?;
        let tmp_path = full_path.with_extension("tmp");
        std::fs::write(&tmp_path, contents)?;
        std::fs::rename(&tmp_path, &full_path)?;
        Ok(())
    }

    fn get_session_manager(&self) -> Arc<dyn SessionManager> {
        self.session_manager.clone()
    }
}
