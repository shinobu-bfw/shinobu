use std::sync::Arc;

use crate::context::BotContext;
use crate::event::Event;

/// A plugin component that continuously receives external events, running on
/// a dedicated OS thread spawned by the bot.
///
/// Adapters are written with the [`#[adapter]`](snb_macros::adapter) attribute
/// macro: author an inherent `async fn run(&self, bot: Arc<dyn BotContext>)` and
/// the macro generates this trait impl, wrapping the body in [`run_async`] so the
/// tokio runtime is created inside the plugin's own cdylib (independent from the
/// host's, avoiding issues with dynamically loaded plugins that carry their own
/// copies of tokio's statics).
///
/// ```ignore
/// use snb_macros::adapter;
///
/// struct MyAdapter;
///
/// #[adapter]
/// impl MyAdapter {
///     async fn run(&self, bot: Arc<dyn BotContext>) {
///         bot.emit_event(Event::message("my", "hello"));
///     }
/// }
/// ```
pub trait Adapter: Send + Sync {
    fn run(&self, bot: Arc<dyn BotContext>);

    /// Send an outgoing event through this adapter.
    ///
    /// Adapters that support platform output should inspect the event's message
    /// content and deliver supported items (text, files, images, etc.). The
    /// default implementation keeps existing receive-only adapters working.
    fn send(&self, _event: &Event) -> anyhow::Result<()> {
        anyhow::bail!("adapter does not support outgoing messages")
    }
}

/// Run an async closure as an adapter body, creating a dedicated single-threaded
/// tokio runtime on the current OS thread.
///
/// Used by the [`#[adapter]`](snb_macros::adapter) macro to bridge the authored
/// `async fn run` to the synchronous [`Adapter::run`]. Adapters should prefer the
/// macro over calling this directly.
///
/// A panic inside `future` is caught here rather than allowed to propagate.
/// Adapters run on an OS thread the host spawned, driving a tokio runtime
/// created *inside this plugin's cdylib*. If the unwind escaped `run` it would
/// cross the cdylib → host boundary — a foreign frame the Rust runtime cannot
/// unwind through, aborting the whole process ("Rust cannot catch foreign
/// exceptions"). Catching it here, still inside the cdylib, lets a faulty
/// adapter stop only itself while the host and other adapters keep running.
pub fn run_async<F: std::future::Future<Output = ()> + Send>(future: F) {
    let body = std::panic::AssertUnwindSafe(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("run_async: failed to create tokio runtime");
        rt.block_on(future);
    });
    if let Err(panic) = std::panic::catch_unwind(body) {
        let msg = panic
            .downcast_ref::<&str>()
            .map(std::string::ToString::to_string)
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        log::error!("adapter panicked and was contained; this adapter has stopped: {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use std::sync::Arc;

    struct TestAdapter;

    impl Adapter for TestAdapter {
        fn run(&self, _bot: Arc<dyn BotContext>) {
            // Test adapter that does nothing
        }

        fn send(&self, _event: &Event) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoSendAdapter;

    impl Adapter for NoSendAdapter {
        fn run(&self, _bot: Arc<dyn BotContext>) {}
    }

    #[test]
    fn adapter_default_send_returns_error() {
        let adapter = NoSendAdapter;
        let event = Event::message("test", "hello");
        let result = adapter.send(&event);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "adapter does not support outgoing messages"
        );
    }

    #[test]
    fn adapter_custom_send_succeeds() {
        let adapter = TestAdapter;
        let event = Event::message("test", "hello");
        assert!(adapter.send(&event).is_ok());
    }

    #[test]
    fn run_async_executes_future() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        run_async(async move {
            flag_clone.store(true, Ordering::SeqCst);
        });

        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn run_async_catches_panic() {
        run_async(async {
            panic!("test panic");
        });
    }
}
