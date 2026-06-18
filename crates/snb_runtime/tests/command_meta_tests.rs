use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use snb_core::bot::BotInfo;
use snb_core::command::{CommandContext, CommandHandler, CommandVisibility};
use snb_core::context::BotContext;
use snb_runtime::bot::Bot;
use snb_runtime::logger::EnvLogger;

struct FakeCommand {
    executed: Arc<AtomicBool>,
    visibility: CommandVisibility,
}

impl CommandHandler for FakeCommand {
    fn name(&self) -> &str {
        "secret"
    }
    fn description(&self) -> &str {
        "a secret command"
    }
    fn visibility(&self) -> CommandVisibility {
        self.visibility
    }
    fn execute(&self, _ctx: &CommandContext) -> anyhow::Result<()> {
        self.executed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn test_bot() -> Arc<Bot> {
    let tmp = std::env::temp_dir().join(format!("snb_cmdmeta_{}", std::process::id()));
    Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(EnvLogger::new()),
        tmp.join("configs"),
        tmp.join("data"),
    ))
}

#[test]
fn commands_accessor_returns_registered_specs() {
    let bot = test_bot();
    bot.register_command(
        "test_plugin",
        Arc::new(FakeCommand {
            executed: Arc::new(AtomicBool::new(false)),
            visibility: CommandVisibility::Admin,
        }),
    );

    let specs = bot.commands();
    let spec = specs
        .iter()
        .find(|s| s.name == "secret")
        .expect("secret command present in specs");
    assert_eq!(spec.description, "a secret command");
    assert_eq!(spec.visibility, CommandVisibility::Admin);
}

use snb_core::event::{Event, Message};

fn command_event(cmd: &str, is_admin: bool) -> Event {
    let mut event = Event::command("test", cmd, "");
    event.message = Some(Message {
        id: None,
        reply_to: None,
        content: Vec::new(),
        from: Some("1".to_string()),
        to: Some("1".to_string()),
        at: Vec::new(),
        chat_type: None,
        is_admin,
        delete_after: None,
    });
    event
}

fn register_fake(bot: &Arc<Bot>, visibility: CommandVisibility) -> Arc<AtomicBool> {
    let executed = Arc::new(AtomicBool::new(false));
    bot.register_command(
        "test_plugin",
        Arc::new(FakeCommand {
            executed: executed.clone(),
            visibility,
        }),
    );
    executed
}

#[test]
fn admin_command_is_skipped_for_non_admin() {
    let bot = test_bot();
    let executed = register_fake(&bot, CommandVisibility::Admin);
    bot.emit_event(command_event("secret", false));
    assert!(
        !executed.load(Ordering::SeqCst),
        "admin command must not run for a non-admin"
    );
}

#[test]
fn admin_command_runs_for_admin() {
    let bot = test_bot();
    let executed = register_fake(&bot, CommandVisibility::Admin);
    bot.emit_event(command_event("secret", true));
    assert!(
        executed.load(Ordering::SeqCst),
        "admin command must run for an admin"
    );
}

#[test]
fn public_command_runs_for_non_admin() {
    let bot = test_bot();
    let executed = register_fake(&bot, CommandVisibility::Public);
    bot.emit_event(command_event("secret", false));
    assert!(
        executed.load(Ordering::SeqCst),
        "public command must run for a non-admin"
    );
}
