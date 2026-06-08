use std::any::Any;
use std::collections::HashMap;
use std::ffi::CStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::session::InMemorySessionManager;
use snb_core::adapter::Adapter;
use snb_core::bot::BotInfo;
use snb_core::command::{CommandContext, CommandHandler};
use snb_core::context::BotContext;
use snb_core::database::DatabaseDriver;
use snb_core::error::PluginError;
use snb_core::event::*;
use snb_core::hook::{Hook, HookType};
use snb_core::logger::Logger;
use snb_core::message_handler::MessageHandler;
use snb_core::plugin::{PluginCell, PluginInfo, SnbPlugin, Version, snb_plugin_abi};
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

#[derive(Clone)]
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
    /// Name conflicts recorded while a plugin's `on_load` runs. The plugin
    /// loader brackets `on_load` with [`Bot::begin_plugin_load`] /
    /// [`Bot::take_plugin_load_conflicts`] to learn whether the plugin tried to
    /// register a component whose name was already taken, and rolls it back if
    /// so. Plugins load sequentially, so a single buffer is sufficient.
    load_conflicts: Mutex<Vec<String>>,
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
            load_conflicts: Mutex::new(Vec::new()),
        }
    }

    /// Reset the per-load conflict buffer. The plugin loader calls this right
    /// before invoking a plugin's `on_load`.
    pub fn begin_plugin_load(&self) {
        self.load_conflicts.lock().unwrap().clear();
    }

    /// Drain and return any name conflicts recorded since the last
    /// [`begin_plugin_load`](Self::begin_plugin_load). A non-empty result means
    /// the plugin must be rejected.
    pub fn take_plugin_load_conflicts(&self) -> Vec<String> {
        std::mem::take(&mut *self.load_conflicts.lock().unwrap())
    }

    /// Remove every component registered under `plugin_name` without touching a
    /// plugin cell. Used by the loader to roll back a plugin that hit a name
    /// conflict mid-`on_load` (the cell is dropped separately by the caller).
    pub fn rollback_plugin_components(&self, plugin_name: &str) {
        self.remove_plugin_components(plugin_name);
    }

    /// Drop every command, hook, message handler, adapter, and database driver
    /// registered under `plugin_name`. Shared by [`unregister_plugin`] (full
    /// teardown) and conflict rollback.
    fn remove_plugin_components(&self, plugin_name: &str) {
        let removed_canonicals: Vec<String> = {
            let mut cmds = self.commands.write().unwrap();
            let to_remove: Vec<String> = cmds
                .iter()
                .filter(|(_, e)| e.plugin_name == plugin_name)
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
            .retain(|e| e.plugin_name != plugin_name);
        self.message_handlers
            .write()
            .unwrap()
            .retain(|e| e.plugin_name != plugin_name);
        self.adapters
            .lock()
            .unwrap()
            .retain(|e| e.plugin_name != plugin_name);
        self.databases.write().unwrap().remove(plugin_name);
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
        let adapters = self.adapters.lock().unwrap().clone();
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

    fn send_to_adapter(&self, adapter_name: &str, event: &Event) -> bool {
        let adapter = {
            let adapters = self.adapters.lock().unwrap();
            adapters
                .iter()
                .find(|entry| entry.plugin_name == adapter_name)
                .map(|entry| entry.adapter.clone())
        };
        let Some(adapter) = adapter else {
            return false;
        };
        if let Err(e) = adapter.send(event) {
            self.logger
                .error(adapter_name, &format!("send failed: {:#}", e));
        }
        true
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

    fn load_plugin(self: Arc<Self>, path: &Path) -> anyhow::Result<()> {
        let current_plugin_abi = snb_plugin_abi();
        let lib = unsafe { libloading::Library::new(path)? };

        let (ptr, destroy_fn, ffi_abi) = unsafe {
            let create_sym: libloading::Symbol<unsafe extern "C" fn() -> *mut Box<dyn SnbPlugin>> =
                lib.get(b"create_plugin")?;
            let destroy_sym: libloading::Symbol<unsafe extern "C" fn(*mut Box<dyn SnbPlugin>)> =
                lib.get(b"destroy_plugin")?;
            let abi_sym: libloading::Symbol<unsafe extern "C" fn() -> *const std::ffi::c_char> =
                lib.get(b"plugin_abi")?;

            let abi_str = CStr::from_ptr(abi_sym()).to_str()?;
            let abi: Version = abi_str.parse().map_err(|_| PluginError::UnsupportedAbi)?;

            if abi.major != current_plugin_abi.major {
                log::error!(
                    "ABI major version mismatch: plugin={}, runtime={} (incompatible)",
                    abi,
                    current_plugin_abi
                );
                return Err(PluginError::UnsupportedAbi)?;
            }

            if abi.minor > current_plugin_abi.minor {
                log::error!(
                    "ABI minor version too new: plugin={}, runtime={} (plugin needs features not available in runtime)",
                    abi,
                    current_plugin_abi
                );
                return Err(PluginError::UnsupportedAbi)?;
            }

            if abi.minor < current_plugin_abi.minor {
                log::warn!(
                    "ABI minor version mismatch: plugin={}, runtime={} (plugin built against older ABI, may miss new features)",
                    abi,
                    current_plugin_abi
                );
            }

            if abi.patch != current_plugin_abi.patch {
                log::warn!(
                    "ABI patch version mismatch: plugin={}, runtime={} (compatible but rebuild recommended)",
                    abi,
                    current_plugin_abi
                );
            }

            let ptr = create_sym();
            let destroy_fn = *destroy_sym;
            (ptr, destroy_fn, abi)
        };
        let keep_alive: Box<dyn Any + Send + Sync> = Box::new(lib);
        let mut cell = unsafe { PluginCell::new(ptr, destroy_fn, keep_alive) };

        if cell.abi_version().major != ffi_abi.major {
            log::warn!(
                "Plugin {} ABI major {} does not match plugin_abi export major {}",
                cell.name(),
                cell.abi_version().major,
                ffi_abi.major
            );
            return Err(PluginError::BrokenAbi)?;
        }

        let name = cell.name().to_string();
        if self.get_plugin(&name).is_some() {
            log::error!("plugin '{}' is already loaded; refusing duplicate", name);
            return Err(PluginError::DuplicatePlugin)?;
        }

        self.begin_plugin_load();
        cell.on_load(self.clone());
        let conflicts = self.take_plugin_load_conflicts();
        if !conflicts.is_empty() {
            log::error!(
                "refusing plugin '{}': {} name conflict(s): {}",
                name,
                conflicts.len(),
                conflicts.join("; ")
            );
            cell.on_unload();
            self.rollback_plugin_components(&name);
            return Err(PluginError::ComponentConflict)?;
        }

        self.register_plugin(cell);
        Ok(())
    }

    fn unload_plugin(self: Arc<Self>, name: &str) -> anyhow::Result<()> {
        if self.unregister_plugin(name) {
            Ok(())
        } else {
            Err(PluginError::UnloadError)?
        }
    }

    fn emit_event(&self, mut event: Event) {
        if let Some(receiver) = &event.receiver
            && self.send_to_adapter(receiver, &event)
        {
            return;
        }

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
        self.remove_plugin_components(name);

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

        // Hold both locks (commands → aliases, the project-wide order) so the
        // conflict check and the insert see a consistent view. Refuse — never
        // overwrite — when a name is already taken, recording the conflict so
        // the loader can roll the whole plugin back.
        let mut cmds = self.commands.write().unwrap();
        let mut aliases = self.aliases.write().unwrap();

        let conflict = if let Some(existing) = cmds.get(&cmd_name) {
            Some(format!(
                "command '{}' already registered by plugin '{}'",
                cmd_name, existing.plugin_name
            ))
        } else if aliases.contains_key(&cmd_name) {
            Some(format!(
                "command '{}' clashes with an existing alias",
                cmd_name
            ))
        } else {
            alias_list.iter().find_map(|a| {
                if cmds.contains_key(a) {
                    Some(format!("alias '{}' clashes with an existing command", a))
                } else if aliases.contains_key(a) {
                    Some(format!("alias '{}' is already registered", a))
                } else {
                    None
                }
            })
        };

        if let Some(msg) = conflict {
            drop(cmds);
            drop(aliases);
            self.logger.error(plugin_name, &msg);
            self.load_conflicts.lock().unwrap().push(msg);
            return;
        }

        for a in &alias_list {
            aliases.insert(a.clone(), cmd_name.clone());
        }
        cmds.insert(
            cmd_name.clone(),
            CommandEntry {
                plugin_name: plugin_name.to_string(),
                command,
            },
        );
        drop(cmds);
        drop(aliases);
        self.logger
            .info(plugin_name, &format!("registered command '{}'", cmd_name));
    }

    fn register_hook(&self, plugin_name: &str, hook: Arc<dyn Hook>) {
        let mut hooks = self.hooks.write().unwrap();
        if let Some(existing) = hooks.iter().find(|e| e.hook.name() == hook.name()) {
            let msg = format!(
                "hook '{}' already registered by plugin '{}'",
                hook.name(),
                existing.plugin_name
            );
            drop(hooks);
            self.logger.error(plugin_name, &msg);
            self.load_conflicts.lock().unwrap().push(msg);
            return;
        }
        self.logger
            .info(plugin_name, &format!("registered hook '{}'", hook.name()));
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
        let mut handlers = self.message_handlers.write().unwrap();
        if let Some(existing) = handlers.iter().find(|e| e.handler.name() == handler.name()) {
            let msg = format!(
                "message handler '{}' already registered by plugin '{}'",
                handler.name(),
                existing.plugin_name
            );
            drop(handlers);
            self.logger.error(plugin_name, &msg);
            self.load_conflicts.lock().unwrap().push(msg);
            return;
        }
        self.logger.info(
            plugin_name,
            &format!("registered message handler '{}'", handler.name()),
        );
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

    fn load_config(&self, plugin_name: &str, relative_path: &Path) -> io::Result<String> {
        let plugin_root = self.config_dir.join(plugin_name);
        let full_path = self.safe_path_under(&plugin_root, relative_path)?;
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
