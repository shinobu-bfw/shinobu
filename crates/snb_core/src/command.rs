use crate::event::Event;

/// Context passed to a [`CommandHandler::execute`] call.
pub struct CommandContext<'a> {
    pub event: &'a Event,
    /// The portion of the message after the command name.
    /// Convenience accessor for `event.command.as_ref().unwrap().args`.
    pub args: &'a str,
}

/// A handler for a command invocation.
///
/// Registered by plugins during [`crate::plugin::SnbPlugin::on_load`] via
/// [`crate::context::BotContext::register_command`]. The bot resolves incoming
/// [`crate::event::EventType::Command`] events against the registered handler set
/// (matching by canonical name or alias) and dispatches to a single
/// handler per event.
pub trait CommandHandler: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> Vec<&str> {
        vec![]
    }
    fn execute(&self, ctx: &CommandContext) -> anyhow::Result<()>;
}
