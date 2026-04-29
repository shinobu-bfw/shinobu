use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::Instant;

use snb_core::session::{SessionKey, SessionManager, SessionMessage, SessionState};

struct SessionData {
    messages: VecDeque<SessionMessage>,
    state: SessionState,
    last_active: Instant,
}

/// A thread-safe, TTL-based in-memory session manager.
///
/// Sessions are keyed by [`SessionKey`] and automatically evicted after
/// `ttl` of inactivity. Each session holds at most `max_messages` entries
/// (oldest messages are dropped first).
pub struct InMemorySessionManager {
    sessions: RwLock<HashMap<String, SessionData>>,
    max_messages: usize,
    ttl: std::time::Duration,
}

impl InMemorySessionManager {
    pub fn new(max_messages: usize, ttl: std::time::Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_messages,
            ttl,
        }
    }

    fn evict_expired(&self) {
        self.sessions
            .write()
            .unwrap()
            .retain(|_, data| data.last_active.elapsed() < self.ttl);
    }
}

impl SessionManager for InMemorySessionManager {
    fn append_message(&self, key: &SessionKey, msg: SessionMessage) {
        self.evict_expired();
        let mut sessions = self.sessions.write().unwrap();
        let skey = key.to_string_key();
        let data = sessions.entry(skey).or_insert_with(|| SessionData {
            messages: VecDeque::new(),
            state: SessionState::Active,
            last_active: Instant::now(),
        });
        data.messages.push_back(msg);
        while data.messages.len() > self.max_messages {
            data.messages.pop_front();
        }
        data.last_active = Instant::now();
    }

    fn get_recent_messages(&self, key: &SessionKey, n: usize) -> Vec<SessionMessage> {
        let sessions = self.sessions.read().unwrap();
        let skey = key.to_string_key();
        match sessions.get(&skey) {
            Some(data) => {
                let skip = data.messages.len().saturating_sub(n);
                data.messages.iter().skip(skip).cloned().collect()
            }
            None => Vec::new(),
        }
    }

    fn get_all_messages(&self, key: &SessionKey) -> Vec<SessionMessage> {
        let sessions = self.sessions.read().unwrap();
        let skey = key.to_string_key();
        match sessions.get(&skey) {
            Some(data) => data.messages.iter().cloned().collect(),
            None => Vec::new(),
        }
    }

    fn set_state(&self, key: &SessionKey, state: SessionState) {
        let mut sessions = self.sessions.write().unwrap();
        let skey = key.to_string_key();
        let data = sessions.entry(skey).or_insert_with(|| SessionData {
            messages: VecDeque::new(),
            state: SessionState::Active,
            last_active: Instant::now(),
        });
        data.state = state;
    }

    fn get_state(&self, key: &SessionKey) -> SessionState {
        let sessions = self.sessions.read().unwrap();
        let skey = key.to_string_key();
        match sessions.get(&skey) {
            Some(data) => data.state,
            None => SessionState::Active,
        }
    }

    fn clear_session(&self, key: &SessionKey) {
        let skey = key.to_string_key();
        self.sessions.write().unwrap().remove(&skey);
    }

    fn message_count(&self, key: &SessionKey) -> usize {
        let sessions = self.sessions.read().unwrap();
        let skey = key.to_string_key();
        match sessions.get(&skey) {
            Some(data) => data.messages.len(),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
