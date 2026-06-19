use super::*;

#[test]
fn event_command_construction() {
    let event = Event::command("test_adapter", "help", "me");
    assert_eq!(event.event_type, EventType::Command);
    assert_eq!(event.source, "test_adapter");
    assert!(event.command.is_some());
    let cmd = event.command.unwrap();
    assert_eq!(cmd.cmd, "help");
    assert_eq!(cmd.args, "me");
}

#[test]
fn event_message_construction() {
    let event = Event::message("telegram", "hello world");
    assert_eq!(event.event_type, EventType::Message);
    assert!(event.message.is_some());
    let msg = event.message.unwrap();
    assert_eq!(msg.text(), "hello world");
    assert!(msg.has_text());
    assert!(!msg.is_admin);
}

#[test]
fn event_formatted_message() {
    let event = Event::formatted_message("discord", "**bold**", TextFormat::Markdown);
    let msg = event.message.unwrap();
    assert_eq!(msg.content.len(), 1);
    match &msg.content[0] {
        ContentItem::Text { text, format } => {
            assert_eq!(text, "**bold**");
            assert_eq!(*format, Some(TextFormat::Markdown));
        }
        _ => panic!("Expected Text content"),
    }
}

#[test]
fn message_text_concatenation() {
    let msg = Message {
        content: vec![
            ContentItem::text("hello "),
            ContentItem::text("world"),
            ContentItem::File {
                source: FileSource::Path("test.txt".into()),
                file_name: Some("test.txt".into()),
                file_id: None,
            },
        ],
        ..Default::default()
    };
    assert_eq!(msg.text(), "hello world");
    assert!(msg.has_text());
}

#[test]
fn event_with_reply_target_plugins() {
    let event = Event::command("test", "ping", "")
        .with_reply_plugin("plugin_a")
        .with_target_plugin("plugin_b");
    assert_eq!(event.reply_plugin, Some("plugin_a".to_string()));
    assert_eq!(event.target_plugin, Some("plugin_b".to_string()));
}

#[test]
fn content_item_builders() {
    let text = ContentItem::text("plain");
    match text {
        ContentItem::Text { text, format } => {
            assert_eq!(text, "plain");
            assert_eq!(format, None);
        }
        _ => panic!("Expected Text"),
    }

    let formatted = ContentItem::formatted_text("bold", TextFormat::Html);
    match formatted {
        ContentItem::Text { text, format } => {
            assert_eq!(text, "bold");
            assert_eq!(format, Some(TextFormat::Html));
        }
        _ => panic!("Expected Text"),
    }
}

#[test]
fn sender_and_chat_constructors_default_metadata() {
    let s = Sender::new("42");
    assert_eq!(s.id, "42");
    assert_eq!(s.username, None);
    assert_eq!(s.display_name, None);
    assert_eq!(s.first_name, None);
    assert!(!s.is_bot);
    assert!(s.extra.is_empty());

    let c = Chat::new("100");
    assert_eq!(c.id, "100");
    assert_eq!(c.kind, None);
    assert_eq!(c.title, None);
    assert!(c.extra.is_empty());
}

#[test]
fn message_accessors_read_sender_and_chat() {
    let mut msg = Message::default();
    assert_eq!(msg.sender_id(), None);
    assert_eq!(msg.chat_id(), ""); // default Chat has an empty id

    msg.sender = Some(Sender::new("u7"));
    msg.chat = Chat::new("c42");
    assert_eq!(msg.sender_id(), Some("u7"));
    assert_eq!(msg.chat_id(), "c42");
}

#[test]
fn sender_extra_holds_platform_specific_metadata() {
    let mut s = Sender::new("9");
    s.extra.insert("is_premium".to_string(), "true".to_string());
    assert_eq!(s.extra.get("is_premium").map(String::as_str), Some("true"));
}
