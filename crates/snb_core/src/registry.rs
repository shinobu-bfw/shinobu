//! Auto-registration registry backed by [`inventory`].
//!
//! The `#[command]` / `#[hook]` / `#[message_handler]` / `#[adapter]` macros emit
//! an `inventory::submit!` for each annotated item. [`register_all`](snb_core::context::register_all)
//! then iterates these at `on_load` time and registers every collected item under
//! the plugin's name — so authors no longer hand-write `register(...)` calls.
//!
//! Because each plugin is a separate `cdylib` with its own statically-linked copy
//! of `snb_core`, `inventory::iter` inside a plugin sees only that plugin's own
//! submissions.

use std::sync::Arc;

use crate::adapter::Adapter;
use crate::command::CommandHandler;
use crate::database::DatabaseDriver;
use crate::hook::Hook;
use crate::message_handler::MessageHandler;

/// A command handler discovered via `#[command]`.
pub struct CommandRegistration {
    pub factory: fn() -> Arc<dyn CommandHandler>,
}
inventory::collect!(CommandRegistration);

/// A hook discovered via `#[hook]`.
pub struct HookRegistration {
    pub factory: fn() -> Arc<dyn Hook>,
}
inventory::collect!(HookRegistration);

/// A message handler discovered via `#[message_handler]`.
pub struct MessageHandlerRegistration {
    pub factory: fn() -> Arc<dyn MessageHandler>,
}
inventory::collect!(MessageHandlerRegistration);

/// An adapter discovered via `#[adapter]`.
pub struct AdapterRegistration {
    pub factory: fn() -> Arc<dyn Adapter>,
}
inventory::collect!(AdapterRegistration);

/// A database driver discovered via `#[database]`.
pub struct DatabaseRegistration {
    pub factory: fn() -> Arc<dyn DatabaseDriver>,
}
inventory::collect!(DatabaseRegistration);

// Re-export for macro-generated `inventory::submit!` calls so plugins don't need
// a direct `inventory` dependency.
pub use inventory::submit;
