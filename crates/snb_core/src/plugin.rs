use crate::context::BotContext;
use crate::event::Event;
use std::any::Any;
use std::ops::{Deref, DerefMut};

/// Classification of a plugin's role in the framework.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginType {
    /// An input adapter that receives external events (e.g., stdin, Telegram).
    Adapter,
    /// A general-purpose plugin (commands, hooks, message handlers).
    Plugin,
    /// A database driver plugin (e.g., `SQLite`).
    DatabaseDriver,
}

/// ABI version — read from `[package.metadata.snb].abi_version` in Cargo.toml.
///
/// Bump on any plugin-facing ABI break (`SnbPlugin` / `BotContext` / `Adapter`
/// trait or `Event` / `PluginCell` layout changes). In `0.x` the **minor** is the
/// breaking position (semver-zero); from `1.x` on, bump the **major**.
///
/// The host rejects a `major` mismatch, a `minor` newer than its own, and — while
/// `major == 0` — any `minor` mismatch. A `1.x+` older minor loads (additive);
/// `patch` differences only warn.
#[must_use]
pub fn snb_plugin_abi() -> Version {
    env!("SNB_ABI_VERSION")
        .parse()
        .expect("invalid SNB_ABI_VERSION")
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::str::FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("expected major.minor.patch, got `{s}`"));
        }
        let major = parts[0].parse().map_err(|e| format!("bad major: {e}"))?;
        let minor = parts[1].parse().map_err(|e| format!("bad minor: {e}"))?;
        let patch = parts[2].parse().map_err(|e| format!("bad patch: {e}"))?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

pub trait SnbPlugin: Send + Sync {
    fn new() -> Self
    where
        Self: Sized;
    fn name(&self) -> &str;
    fn version(&self) -> Version;
    fn plugin_type(&self) -> PluginType;
    fn abi_version(&self) -> Version {
        snb_plugin_abi()
    }
    fn on_load(&mut self, ctx: std::sync::Arc<dyn BotContext>);
    fn on_unload(&mut self);

    /// Called for every dispatched event (after hooks and command/message dispatch).
    ///
    /// Use `ctx.emit_event(...)` to send response events back into the bot's
    /// dispatch loop. Default is a no-op; override as needed.
    fn on_event(&self, event: &Event) {
        let _ = event;
    }
}

/// A snapshot of a plugin's identity, returned by [`BotContext::get_plugin`].
///
/// Cached at register time so callers don't need to reach into the live
/// plugin object (which would otherwise require synchronisation).
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: Version,
    pub plugin_type: PluginType,
    pub abi_version: Version,
}

impl PluginInfo {
    pub fn from_plugin(p: &dyn SnbPlugin) -> Self {
        Self {
            name: p.name().to_string(),
            version: p.version(),
            plugin_type: p.plugin_type(),
            abi_version: p.abi_version(),
        }
    }
}

// ---------------------------------------------------------------------------
// FFI infrastructure — keeps dynamically loaded plugins alive and safe
// ---------------------------------------------------------------------------

/// A handle to a loaded plugin that ensures correct deallocation order.
///
/// Wraps a raw pointer to the plugin trait object together with the
/// `destroy_plugin` export and a keep-alive handle (the `Library`) so the
/// dylib stays loaded as long as the cell exists.
///
/// Implements [`Deref`] and [`DerefMut`] to [`dyn SnbPlugin`], so a shared
/// reference to `PluginCell` gives direct access to the plugin's methods.
pub struct PluginCell {
    ptr: *mut Box<dyn SnbPlugin>,
    destroy: unsafe extern "C" fn(*mut Box<dyn SnbPlugin>),
    _keep_alive: Box<dyn Any + Send + Sync>,
}

// SAFETY: SnbPlugin requires Send + Sync. The bot owns each PluginCell
// behind a Mutex; nothing else aliases the raw pointer.
unsafe impl Send for PluginCell {}
unsafe impl Sync for PluginCell {}

impl PluginCell {
    /// # Safety
    ///
    /// - `ptr` must have been returned by the plugin's `create_plugin` export.
    /// - `destroy` must be the plugin's `destroy_plugin` export.
    /// - `keep_alive` must prevent the dylib from being unloaded (e.g.
    ///   `Box::new(libloading::Library)`).
    pub unsafe fn new(
        ptr: *mut Box<dyn SnbPlugin>,
        destroy: unsafe extern "C" fn(*mut Box<dyn SnbPlugin>),
        keep_alive: Box<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            ptr,
            destroy,
            _keep_alive: keep_alive,
        }
    }
}

impl Deref for PluginCell {
    type Target = dyn SnbPlugin;

    fn deref(&self) -> &Self::Target {
        unsafe { &**self.ptr }
    }
}

impl DerefMut for PluginCell {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut **self.ptr }
    }
}

impl Drop for PluginCell {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            (self.destroy)(self.ptr);
        }));
    }
}
