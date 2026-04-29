use anyhow::Result;
use std::any::Any;
use std::ffi::CStr;
use std::sync::Arc;

use crate::bot::Bot;
use snb_core::context::BotContext;
use snb_core::error::PluginError;
use snb_core::plugin::{PluginCell, SnbPlugin, Version, snb_plugin_abi};

/// Loads and unloads plugin shared libraries (`.so` / `.dylib` / `.dll`).
///
/// Validates ABI compatibility before calling the plugin's `on_load`, and
/// ensures safe teardown order on unload (plugin `on_unload` → drop all
/// trait objects → drop the library handle).
pub struct PluginLoader {
    bot: Arc<Bot>,
}

impl PluginLoader {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self { bot }
    }

    pub fn load_plugin(&self, path: std::path::PathBuf) -> Result<()> {
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
                return Err(PluginError::UnsupportedAbi)?;
            }

            let ptr = create_sym();
            let destroy_fn = *destroy_sym; // copy the fn pointer (Copy type)
            (ptr, destroy_fn, abi)
        };
        let keep_alive: Box<dyn Any + Send + Sync> = Box::new(lib);
        let mut cell = unsafe { PluginCell::new(ptr, destroy_fn, keep_alive) };

        if cell.abi_version().major != ffi_abi.major {
            let err = format!(
                "Plugin {} ABI major {} does not match plugin_abi export major {}",
                cell.name(),
                cell.abi_version().major,
                ffi_abi.major
            );
            self.bot.logger().warn("PluginLoader", &err);
            return Err(PluginError::BrokenAbi)?;
        }

        cell.on_load(self.bot.clone());
        self.bot.register_plugin(cell);
        Ok(())
    }

    pub fn unload_plugin(&self, name: &str) -> Result<()> {
        if self.bot.unregister_plugin(name) {
            Ok(())
        } else {
            Err(PluginError::UnloadError)?
        }
    }
}
