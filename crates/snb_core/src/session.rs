use std::fmt;

/// Identifies a conversation session.
///
/// A session is scoped to a chat (group/channel/private) and optionally
/// a specific user within that chat. Plugins decide how to construct keys
/// based on their use case (e.g., per-user-in-group vs per-group).
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct SessionKey {
    pub chat_id: String,
    pub user_id: Option<String>,
}

impl SessionKey {
    pub fn new(chat_id: impl Into<String>, user_id: Option<impl Into<String>>) -> Self {
        Self {
            chat_id: chat_id.into(),
            user_id: user_id.map(Into::into),
        }
    }

    /// Key for a group-level session (shared across all users in a chat).
    pub fn group(chat_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            user_id: None,
        }
    }

    /// Key for a private session (user + chat).
    pub fn private(chat_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            user_id: Some(user_id.into()),
        }
    }

    /// Canonical string representation for use as HashMap key.
    pub fn to_string_key(&self) -> String {
        match &self.user_id {
            Some(uid) => format!("{}:{}", self.chat_id, uid),
            None => self.chat_id.clone(),
        }
    }
}

impl fmt::Display for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_key())
    }
}

/// A single message in a conversation session.
#[derive(Debug, Clone)]
pub struct SessionMessage {
    /// Role identifier: "user", "assistant", "bot", "system", etc.
    pub role: String,
    /// Message content (plain text).
    pub content: String,
    /// Unix timestamp (seconds).
    pub timestamp: i64,
    /// Optional reference to the original event message ID.
    pub event_id: Option<String>,
}

impl SessionMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp: now_unix(),
            event_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new("assistant", content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }

    pub fn with_event_id(mut self, id: impl Into<String>) -> Self {
        self.event_id = Some(id.into());
        self
    }
}

/// State of a conversation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Default state — session is active.
    Active,
    /// Waiting for user input (e.g., mid-wizard flow).
    WaitingForInput,
    /// Session is completed/closed.
    Completed,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::WaitingForInput => "waiting",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SessionState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "waiting" => Ok(Self::WaitingForInput),
            "completed" => Ok(Self::Completed),
            _ => Err(format!("unknown session state: {s:?}")),
        }
    }
}

/// Temporary in-memory session management interface.
///
/// Provides short-lived session state for multi-step interactions.
/// Messages are kept in memory only — plugins that need persistence
/// should store their own data via the database driver.
///
/// Obtain via [`crate::context::BotContext::get_session_manager()`].
pub trait SessionManager: Send + Sync {
    /// Append a message to a session. Creates the session if it doesn't exist.
    fn append_message(&self, key: &SessionKey, msg: SessionMessage);

    /// Get up to `n` recent messages (most recent last).
    fn get_recent_messages(&self, key: &SessionKey, n: usize) -> Vec<SessionMessage>;

    /// Get all messages currently in memory for this session.
    fn get_all_messages(&self, key: &SessionKey) -> Vec<SessionMessage>;

    /// Set the state of a session.
    fn set_state(&self, key: &SessionKey, state: SessionState);

    /// Get the current state of a session.
    fn get_state(&self, key: &SessionKey) -> SessionState;

    /// Clear a session from memory.
    fn clear_session(&self, key: &SessionKey);

    /// Get the number of messages in a session.
    fn message_count(&self, key: &SessionKey) -> usize;
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
