//! Core traits, types, and the global bot context for the Shinobu plugin framework.
//!
//! This crate defines the plugin-facing API surface: [`context::BotContext`],
//! [`plugin::SnbPlugin`], [`adapter::Adapter`], [`command::CommandHandler`],
//! [`hook::Hook`], [`message_handler::MessageHandler`], [`database::DatabaseDriver`],
//! and the [`event::Event`] type that flows through the dispatch loop.
//!
//! Plugins depend on `snb_core` + `snb_macros` and compile to `cdylib` shared
//! libraries that are loaded at runtime by `snb_runtime`.

pub mod adapter;
pub mod bot;
pub mod command;
pub mod context;
pub mod database;
pub mod error;
pub mod event;
pub mod hook;
pub mod log_bridge;
pub mod logger;
pub mod message_handler;
pub mod plugin;
pub mod session;
