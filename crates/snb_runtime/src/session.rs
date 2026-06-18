use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::Instant;

use snb_core::session::{SessionKey, SessionManager, SessionMessage, SessionState};

struct SessionData {
    messages: VecDeque<SessionMessage>,
    state: SessionState,
    last_active: Instant,
}

impl SessionData {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            state: SessionState::Active,
            last_active: Instant::now(),
        }
    }
}

/// A thread-safe, TTL-based in-memory session manager.
///
/// Sessions are keyed by [`SessionKey`] and automatically evicted after
/// `ttl` of inactivity. Each session holds at most `max_messages` entries
/// (oldest messages are dropped first).
pub struct InMemorySessionManager {
    sessions: RwLock<HashMap<SessionKey, SessionData>>,
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

    /// Read a session, returning `default()` when it doesn't exist.
    fn with_session<R>(
        &self,
        key: &SessionKey,
        default: impl FnOnce() -> R,
        f: impl FnOnce(&SessionData) -> R,
    ) -> R {
        match self.sessions.read().unwrap().get(key) {
            Some(data) => f(data),
            None => default(),
        }
    }
}

impl SessionManager for InMemorySessionManager {
    fn append_message(&self, key: &SessionKey, msg: SessionMessage) {
        self.evict_expired();
        let mut sessions = self.sessions.write().unwrap();
        let data = sessions.entry(key.clone()).or_insert_with(SessionData::new);
        data.messages.push_back(msg);
        while data.messages.len() > self.max_messages {
            data.messages.pop_front();
        }
        data.last_active = Instant::now();
    }

    fn get_recent_messages(&self, key: &SessionKey, n: usize) -> Vec<SessionMessage> {
        self.with_session(key, Vec::new, |data| {
            let skip = data.messages.len().saturating_sub(n);
            data.messages.iter().skip(skip).cloned().collect()
        })
    }

    fn get_all_messages(&self, key: &SessionKey) -> Vec<SessionMessage> {
        self.with_session(key, Vec::new, |data| {
            data.messages.iter().cloned().collect()
        })
    }

    fn set_state(&self, key: &SessionKey, state: SessionState) {
        let mut sessions = self.sessions.write().unwrap();
        sessions
            .entry(key.clone())
            .or_insert_with(SessionData::new)
            .state = state;
    }

    fn get_state(&self, key: &SessionKey) -> SessionState {
        self.with_session(key, || SessionState::Active, |data| data.state)
    }

    fn clear_session(&self, key: &SessionKey) {
        self.sessions.write().unwrap().remove(key);
    }

    fn message_count(&self, key: &SessionKey) -> usize {
        self.with_session(key, || 0, |data| data.messages.len())
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_tests.rs"]
mod session_tests;
