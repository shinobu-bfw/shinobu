use crate::event::Event;

/// Context passed to a [`CommandHandler::execute`] call.
pub struct CommandContext<'a> {
    pub event: &'a Event,
    /// The portion of the message after the command name.
    /// Convenience accessor for `event.command.as_ref().unwrap().args`.
    pub args: &'a str,
}

/// How a command is exposed to users in clients that support a command menu
/// (e.g. Telegram's `setMyCommands`) and how the runtime gates its dispatch.
///
/// - `Public`: visible to everyone; dispatched to anyone.
/// - `Admin`: visible only to configured admins; the runtime silently ignores
///   invocations from non-admins (see `Bot::dispatch_command`).
/// - `Hidden`: never advertised in a command menu; still dispatchable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CommandVisibility {
    #[default]
    Public,
    Admin,
    Hidden,
}

/// A snapshot of a registered command's metadata, returned by
/// [`crate::context::BotContext::commands`]. Adapters use this to populate a
/// platform command menu without reaching into the live handler.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub visibility: CommandVisibility,
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

    /// Human-readable one-line description shown in a client command menu.
    /// Defaults to empty (Telegram permits an empty description).
    fn description(&self) -> &str {
        ""
    }

    /// Visibility / gating classification for this command. Defaults to
    /// [`CommandVisibility::Public`].
    fn visibility(&self) -> CommandVisibility {
        CommandVisibility::Public
    }

    fn execute(&self, ctx: &CommandContext) -> anyhow::Result<()>;
}

#[cfg(test)]
#[path = "../tests/unit/command_tests.rs"]
mod command_tests;
