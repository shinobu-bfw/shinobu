//! Proc macros for the Shinobu plugin framework.
//!
//! - `#[plugin]` — generates the FFI boilerplate for dynamically loaded plugins
//!   (`create_plugin`, `destroy_plugin`, `plugin_abi` exports).
//! - `#[command(...)]`, `#[hook(...)]`, `#[message_handler(...)]`, `#[adapter]` —
//!   generate the corresponding trait `impl` from an inherent `impl` block, so
//!   authors only write the core method and declare metadata as attributes.

mod adapter;
mod command;
mod common;
mod database;
mod handler;
mod hook;
mod plugin;

use proc_macro::TokenStream;

/// Generate the FFI exports for a Shinobu plugin struct, and optionally the
/// whole `SnbPlugin` impl.
///
/// Bare `#[plugin]` emits only `create_plugin` / `destroy_plugin` / `plugin_abi`
/// — pair it with a hand-written `SnbPlugin` impl (custom `on_load`/`on_event`).
///
/// With metadata it also generates the `SnbPlugin` impl, folding in `set_bot`
/// and `register_all` (requires a unit struct):
///
/// ```ignore
/// #[plugin(name = "stdin", version = "0.1.0", kind = Adapter)]
/// pub struct StdinAdapter;
/// ```
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, input: TokenStream) -> TokenStream {
    plugin::expand(attr.into(), input.into()).into()
}

/// Generate a `CommandHandler` from a free function and auto-register it.
///
/// ```ignore
/// #[command(name = "echo", aliases = ["say"])]
/// fn echo(ctx: &CommandContext) -> anyhow::Result<()> { ... }
/// ```
#[proc_macro_attribute]
pub fn command(attr: TokenStream, input: TokenStream) -> TokenStream {
    command::expand(attr.into(), input.into()).into()
}

/// Generate a `Hook` from a free function and auto-register it.
///
/// ```ignore
/// #[hook(name = "log_hook", kind = HookType::All)]
/// fn log_hook(event: &mut Event) -> anyhow::Result<()> { ... }
/// ```
#[proc_macro_attribute]
pub fn hook(attr: TokenStream, input: TokenStream) -> TokenStream {
    hook::expand(attr.into(), input.into()).into()
}

/// Generate a `MessageHandler` from a free function and auto-register it.
///
/// ```ignore
/// #[message_handler(name = "echo_handler")]
/// fn echo_handler(event: &Event) -> anyhow::Result<()> { ... }
/// ```
#[proc_macro_attribute]
pub fn message_handler(attr: TokenStream, input: TokenStream) -> TokenStream {
    handler::expand(attr.into(), input.into()).into()
}

/// Generate an `Adapter` from a free `async fn` and auto-register it.
///
/// The async body is driven by `run_async`, creating a tokio runtime inside
/// the plugin's own cdylib.
///
/// ```ignore
/// #[adapter]
/// async fn demo(bot: Arc<dyn BotContext>) { ... }
/// ```
#[proc_macro_attribute]
pub fn adapter(attr: TokenStream, input: TokenStream) -> TokenStream {
    adapter::expand(attr.into(), input.into()).into()
}

/// Register a `DatabaseDriver` built by a free function and auto-register it.
///
/// ```ignore
/// #[database]
/// fn sqlite() -> SqliteDatabase {
///     let path = context::bot().data_dir("sqlite").join("data.db");
///     SqliteDatabase::new("sqlite", path)
/// }
/// ```
#[proc_macro_attribute]
pub fn database(attr: TokenStream, input: TokenStream) -> TokenStream {
    database::expand(attr.into(), input.into()).into()
}
