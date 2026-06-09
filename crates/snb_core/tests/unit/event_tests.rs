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
        id: None,
        reply_to: None,
        content: vec![
            ContentItem::text("hello "),
            ContentItem::text("world"),
            ContentItem::File {
                source: FileSource::Path("test.txt".into()),
                file_name: Some("test.txt".into()),
                file_id: None,
            },
        ],
        from: None,
        to: None,
        at: Vec::new(),
        chat_type: None,
        is_admin: false,
        delete_after: None,
    };
    assert_eq!(msg.text(), "hello world");
    assert!(msg.has_text());
}

#[test]
fn event_with_sender_receiver() {
    let event = Event::command("test", "ping", "")
        .with_sender("plugin_a")
        .with_receiver("plugin_b");
    assert_eq!(event.sender, Some("plugin_a".to_string()));
    assert_eq!(event.receiver, Some("plugin_b".to_string()));
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
