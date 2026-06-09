use super::*;

fn test_sm() -> InMemorySessionManager {
    InMemorySessionManager::new(5, std::time::Duration::from_secs(3600))
}

#[test]
fn append_and_get_recent() {
    let sm = test_sm();
    let key = SessionKey::private("chat1", "user1");

    sm.append_message(&key, SessionMessage::user("hello"));
    sm.append_message(&key, SessionMessage::assistant("hi"));
    sm.append_message(&key, SessionMessage::user("bye"));

    let recent = sm.get_recent_messages(&key, 10);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].content, "hello");
    assert_eq!(recent[2].content, "bye");
}

#[test]
fn max_messages_eviction() {
    let sm = test_sm(); // max_messages = 5
    let key = SessionKey::group("chat1");

    for i in 0..10 {
        sm.append_message(&key, SessionMessage::user(format!("msg {i}")));
    }

    let recent = sm.get_recent_messages(&key, 100);
    assert_eq!(recent.len(), 5);
    assert_eq!(recent[0].content, "msg 5");
    assert_eq!(recent[4].content, "msg 9");
}

#[test]
fn session_state() {
    let sm = test_sm();
    let key = SessionKey::private("chat1", "user1");

    assert_eq!(sm.get_state(&key), SessionState::Active);

    sm.set_state(&key, SessionState::WaitingForInput);
    assert_eq!(sm.get_state(&key), SessionState::WaitingForInput);
}

#[test]
fn clear_session() {
    let sm = test_sm();
    let key = SessionKey::group("chat1");

    sm.append_message(&key, SessionMessage::user("test"));
    assert_eq!(sm.message_count(&key), 1);

    sm.clear_session(&key);
    assert_eq!(sm.message_count(&key), 0);
    assert_eq!(sm.get_state(&key), SessionState::Active);
}

#[test]
fn get_all_messages() {
    let sm = test_sm();
    let key = SessionKey::group("g");

    sm.append_message(&key, SessionMessage::user("a"));
    sm.append_message(&key, SessionMessage::assistant("b"));

    let all = sm.get_all_messages(&key);
    assert_eq!(all.len(), 2);
}
