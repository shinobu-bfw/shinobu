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
#[path = "../tests/unit/session_tests.rs"]
mod session_tests;
