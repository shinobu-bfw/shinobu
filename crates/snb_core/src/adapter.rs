use std::sync::Arc;

use crate::context::BotContext;

/// A plugin component that continuously receives external events, running on
/// a dedicated OS thread spawned by the bot.
///
/// Synchronous adapters can block (e.g. `stdin.read_line`) without affecting
/// the tokio runtime. An adapter that needs async I/O can create its own
/// tokio runtime inside `run` — use [`run_async`] as a convenience wrapper.
pub trait Adapter: Send + Sync {
    fn run(&self, bot: Arc<dyn BotContext>);
}

/// Run an async closure as an adapter body, creating a dedicated single-threaded
/// tokio runtime on the current OS thread.
///
/// This is the recommended way to write async adapters: the runtime is
/// independent from the host's, avoiding issues with dynamically loaded
/// plugins that have their own copies of tokio's statics.
///
/// # Example
///
/// ```ignore
/// use snb_core::adapter::{Adapter, run_async};
///
/// struct MyAdapter;
///
/// impl Adapter for MyAdapter {
///     fn run(&self, bot: Arc<dyn BotContext>) {
///         run_async(async move {
///             // async code here
///             bot.emit_event(Event::message("my", "hello"));
///         });
///     }
/// }
/// ```
pub fn run_async<F: std::future::Future<Output = ()> + Send>(future: F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("run_async: failed to create tokio runtime");
    rt.block_on(future);
}
