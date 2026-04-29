use std::io::{self, BufRead};
use std::sync::Arc;

use snb_core::adapter::Adapter;
use snb_core::context::{BotContext, PluginHelper};
use snb_core::event::Event;
use snb_core::plugin::{PluginType, SnbPlugin, Version};
use snb_macros::plugin;

/// Built-in stdin adapter.
///
/// Reads lines from stdin and dispatches them as [`snb_core::event::EventType::Message`]
/// events through [`BotContext::emit_event`].
///
/// This also serves as a reference implementation for third-party adapters.
/// To build your own, depend on `snb_core` + `snb_macros`, implement
/// [`SnbPlugin`] with [`PluginType::Adapter`] and [`Adapter`].
#[plugin]
pub struct StdinAdapter;
impl SnbPlugin for StdinAdapter {
    fn new() -> Self {
        Self
    }

    fn name(&self) -> &str {
        "stdin"
    }

    fn version(&self) -> Version {
        Version {
            major: 0,
            minor: 1,
            patch: 0,
        }
    }

    fn plugin_type(&self) -> PluginType {
        PluginType::Adapter
    }

    fn on_load(&mut self, ctx: Arc<dyn BotContext>) {
        snb_core::context::set_bot(ctx.clone());
        ctx.register_adapter(self.name(), Arc::new(StdinAdapterRunner));

        let p = PluginHelper::for_plugin(self.name());
        p.info(&format!("v{} loaded!", self.version()));
    }

    fn on_unload(&mut self) {}
}

// -- Adapter runner (stateless) ----------------------------------------------

struct StdinAdapterRunner;

impl Adapter for StdinAdapterRunner {
    fn run(&self, bot: Arc<dyn BotContext>) {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let text = match line {
                Ok(t) if !t.is_empty() => t,
                _ => break,
            };
            let event = if let Some(rest) = text.strip_prefix('/') {
                let mut parts = rest.splitn(2, char::is_whitespace);
                let cmd = parts.next().unwrap_or("");
                let args = parts.next().unwrap_or("").trim_start();
                if cmd.is_empty() {
                    Event::message("stdin", text)
                } else {
                    Event::command("stdin", cmd, args)
                }
            } else {
                Event::message("stdin", text)
            };
            bot.emit_event(event);
        }
    }
}
