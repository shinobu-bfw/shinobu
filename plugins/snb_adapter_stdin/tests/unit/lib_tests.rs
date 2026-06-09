use super::*;

#[test]
fn stdin_message_defaults_to_admin() {
    let event = with_admin_context(Event::message("stdin", "hello"), "hello");
    let message = event.message.unwrap();

    assert_eq!(message.from.as_deref(), Some("stdin"));
    assert_eq!(message.to.as_deref(), Some("stdin"));
    assert_eq!(message.chat_type, Some(ChatType::Private));
    assert!(message.is_admin);
}

#[test]
fn stdin_command_carries_admin_message() {
    let event = with_admin_context(Event::command("stdin", "ping", ""), "/ping");
    let message = event.message.unwrap();

    assert_eq!(message.text(), "/ping");
    assert!(message.is_admin);
}
