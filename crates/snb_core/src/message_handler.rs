use crate::event::Event;

/// A handler for non-command messages.
///
/// Registered by plugins during [`crate::plugin::SnbPlugin::on_load`] via
/// [`crate::context::BotContext::register_message_handler`].  The bot dispatches
/// [`crate::event::EventType::Message`] events to all registered handlers sorted by
/// [`priority`](MessageHandler::priority) (lower runs first).
///
/// # Example
///
/// ```ignore
/// struct EchoHandler;
///
/// impl MessageHandler for EchoHandler {
///     fn name(&self) -> &str { "echo" }
///     fn handle(&self, event: &Event) -> anyhow::Result<()> {
///         if let Some(msg) = &event.message {
///             snb_core::context::bot().emit_event(Event::message("bot", msg.text()));
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait MessageHandler: Send + Sync {
    fn name(&self) -> &str;

    fn priority(&self) -> u32 {
        0
    }

    fn handle(&self, event: &Event) -> anyhow::Result<()>;
}
