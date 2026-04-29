use crate::event::{Event, EventType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookType {
    /// Run before any command.
    BeforeCommand,
    /// Run after any command.
    AfterCommand,
    /// Run before only the named command.
    BeforeNamedCommand(String),
    /// Run after only the named command.
    AfterNamedCommand(String),
    /// Run on events whose [`EventType`] equals the inner value.
    Event(EventType),
    /// Run on every dispatched event, regardless of type.
    All,
}

/// An event interceptor registered by a plugin.
///
/// Hooks are sorted by [`priority`](Hook::priority) (lower runs first) and
/// executed by the bot when matching events are dispatched.
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn hook_type(&self) -> HookType;
    fn priority(&self) -> u32 {
        0
    }
    fn execute(&self, event: &mut Event) -> anyhow::Result<()>;
}
