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
