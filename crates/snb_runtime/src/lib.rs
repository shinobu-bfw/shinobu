//! Runtime implementation for the Shinobu bot framework.
//!
//! Provides the concrete [`Bot`](bot::Bot) struct that implements
//! [`BotContext`](snb_core::context::BotContext), along with a
//! [`PluginLoader`](plugin_manager::PluginLoader) for dynamic `.so` loading,
//! a [`StdoutLogger`](logger::StdoutLogger), and an
//! [`InMemorySessionManager`](session::InMemorySessionManager).

pub mod bot;
pub mod logger;
pub mod plugin_manager;
pub mod session;
