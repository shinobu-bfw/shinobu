use std::io::{self, BufRead};
use std::sync::Arc;

use snb_core::context::BotContext;
use snb_core::event::Event;
use snb_macros::{adapter, plugin};

/// Built-in stdin adapter.
///
/// Reads lines from stdin and dispatches them as [`snb_core::event::EventType::Message`]
/// events through [`BotContext::emit_event`].
///
/// This also serves as a reference implementation for third-party adapters: the
/// `#[plugin(...)]` form generates the whole `SnbPlugin` impl, and `#[adapter]` /
/// `#[command]` / `#[hook]` / `#[message_handler]` declare and auto-register the
/// plugin's components.
#[plugin(name = "stdin", version = "0.1.0", kind = Adapter)]
pub struct StdinAdapter;

#[adapter]
async fn stdin_reader(bot: Arc<dyn BotContext>) {
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
