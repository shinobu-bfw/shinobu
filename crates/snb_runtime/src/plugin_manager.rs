use anyhow::Result;
use std::sync::Arc;

use crate::bot::Bot;
use snb_core::context::BotContext;

/// Loads and unloads plugin shared libraries (`.so` / `.dylib` / `.dll`).
///
/// This is a small runtime-facing wrapper around [`BotContext`]'s plugin
/// lifecycle API. Keeping the actual logic on `Bot` lets management plugins use
/// the same ABI checks, duplicate checks, and rollback path as the CLI startup
/// loader.
pub struct PluginLoader {
    bot: Arc<Bot>,
}

impl PluginLoader {
    pub fn new(bot: Arc<Bot>) -> Self {
        // Ensure the bot context is set so `log::*!` macros work throughout
        // plugin loading. The runtime should have already called `set_bot`,
        // but we check to avoid overwriting it if already present.
        if !snb_core::context::bot_is_set() {
            snb_core::context::set_bot(bot.clone());
        }
        Self { bot }
    }

    pub fn load_plugin(&self, path: std::path::PathBuf) -> Result<()> {
        self.bot.clone().load_plugin(&path)
    }

    pub fn unload_plugin(&self, name: &str) -> Result<()> {
        self.bot.clone().unload_plugin(name)
    }
}
