use super::*;

struct DefaultsCommand;

impl CommandHandler for DefaultsCommand {
    fn name(&self) -> &str {
        "defaults"
    }
    fn execute(&self, _ctx: &CommandContext) -> anyhow::Result<()> {
        Ok(())
    }
}

struct AdminCommand;

impl CommandHandler for AdminCommand {
    fn name(&self) -> &str {
        "admin"
    }
    fn description(&self) -> &str {
        "an admin command"
    }
    fn visibility(&self) -> CommandVisibility {
        CommandVisibility::Admin
    }
    fn execute(&self, _ctx: &CommandContext) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn command_handler_defaults_are_public_with_empty_description() {
    let cmd = DefaultsCommand;
    assert_eq!(cmd.description(), "");
    assert_eq!(cmd.visibility(), CommandVisibility::Public);
}

#[test]
fn command_handler_can_declare_admin_visibility() {
    let cmd = AdminCommand;
    assert_eq!(cmd.description(), "an admin command");
    assert_eq!(cmd.visibility(), CommandVisibility::Admin);
}

#[test]
fn command_visibility_default_is_public() {
    assert_eq!(CommandVisibility::default(), CommandVisibility::Public);
}
