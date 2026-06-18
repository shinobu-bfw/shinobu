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
