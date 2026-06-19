use std::collections::BTreeMap;
use std::time::Duration;

/// Discriminant for [`Event`]. The structured payload (parsed command,
/// message text, etc.) lives in the corresponding `Option` field on the
/// event itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    PluginLoaded,
    PluginUnloaded,
    Command,
    Message,
    MessageSent,
    MessageEdit,
    MessageDelete,
    Other(String),
}

/// A parsed command invocation.
///
/// `cmd` is the bare command name (without any leading prefix like `/`),
/// `args` is everything that followed it on the same line.
#[derive(Debug, Clone)]
pub struct Command {
    pub cmd: String,
    pub args: String,
}

/// Source of an image attachment.
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Remote URL.
    Url(String),
    /// Local image path.
    Path(String),
    /// Platform-assigned file ID.
    Id(String),
    /// Base64-encoded image data.
    Base64(String),
}

/// Source of a file attachment.
#[derive(Debug, Clone)]
pub enum FileSource {
    /// Remote URL.
    Url(String),
    /// Local file path.
    Path(String),
    /// Platform-assigned file ID.
    Id(String),
}

/// Text formatting mode for platforms that support parsed markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Markdown,
    MarkdownV2,
    Html,
}

/// A single content item within a message.
///
/// A message can contain multiple content items (text + image + file, etc.).
#[derive(Debug, Clone)]
pub enum ContentItem {
    Text {
        text: String,
        format: Option<TextFormat>,
    },
    Image {
        source: ImageSource,
        /// Optional platform-assigned file ID.
        file_id: Option<String>,
        /// Optional image caption.
        caption: Option<String>,
    },
    File {
        source: FileSource,
        file_name: Option<String>,
        file_id: Option<String>,
    },
    /// Platform-specific custom content type.
    Other { kind: String, data: String },
}

impl ContentItem {
    /// Build a plain text content item.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            format: None,
        }
    }

    /// Build a formatted text content item.
    pub fn formatted_text(text: impl Into<String>, format: TextFormat) -> Self {
        Self::Text {
            text: text.into(),
            format: Some(format),
        }
    }
}

/// Chat context type. Platform-neutral; adapters record the exact platform
/// kind (e.g. Telegram "supergroup") in `Chat::extra["raw_kind"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatType {
    /// Private / one-on-one chat.
    Private,
    /// Group chat (Telegram group or supergroup).
    Group,
    /// Broadcast channel (Telegram channel, etc.).
    Channel,
    /// Platform-specific custom type.
    Other(String),
}

/// A message sender's identity and metadata. `id` is the platform user id used
/// for routing/auth; the rest is best-effort display metadata. Platform-specific
/// fields (e.g. Telegram `is_premium`) go in `extra` rather than bloating the
/// neutral type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sender {
    /// Platform-assigned user id (canonical; used for auth and session keys).
    pub id: String,
    /// Public @username, if the user has one.
    pub username: Option<String>,
    /// Human-readable display name (e.g. first + last name).
    pub display_name: Option<String>,
    /// Given name, if the platform exposes name parts.
    pub first_name: Option<String>,
    /// Family name, if the platform exposes name parts.
    pub last_name: Option<String>,
    /// Whether the sender is a bot.
    pub is_bot: bool,
    /// BCP-47 / ISO-639-1 language code the sender's client reports.
    pub language: Option<String>,
    /// Platform-specific / not-yet-typed metadata. Neutral adapters leave this
    /// empty. Promote a key to a typed field once a second platform needs it.
    pub extra: BTreeMap<String, String>,
}

impl Sender {
    /// Build a sender carrying only its id; metadata defaults to absent.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }
}

/// A chat / conversation's identity and metadata. `id` is the platform chat id
/// used for routing; the rest is best-effort.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chat {
    /// Platform-assigned chat id (canonical; the outgoing send target).
    pub id: String,
    /// Chat context type (private/group/channel/other). `None` when unknown.
    pub kind: Option<ChatType>,
    /// Group/channel title (absent for private chats).
    pub title: Option<String>,
    /// Public @username/handle of the chat, if any.
    pub username: Option<String>,
    /// Platform-specific / not-yet-typed metadata (e.g. `raw_kind="supergroup"`).
    pub extra: BTreeMap<String, String>,
}

impl Chat {
    /// Build a chat carrying only its id; metadata defaults to absent.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }
}

/// A non-command message.
#[derive(Debug, Clone, Default)]
pub struct Message {
    /// Message ID assigned by the adapter, used for reply references.
    pub id: Option<String>,
    /// ID of the message being replied to.
    pub reply_to: Option<String>,
    /// Content items (text, images, files, etc.).
    pub content: Vec<ContentItem>,
    /// Sender identity + metadata. `None` for channel posts / outgoing messages
    /// with no human sender.
    pub sender: Option<Sender>,
    /// Chat identity + metadata. Carries the routing id (inbound source /
    /// outgoing target) and, inbound, display metadata.
    pub chat: Chat,
    /// Users mentioned / @-ed in this message.
    pub at: Vec<String>,
    /// Whether the sender is an administrator in this chat/context.
    ///
    /// Adapters should set this when the platform exposes administrator
    /// membership/permission information. It defaults to `false` when unknown.
    pub is_admin: bool,
    /// Delete this outgoing message after the given duration, if supported by the adapter.
    pub delete_after: Option<Duration>,
}

impl Message {
    /// Concatenate all text content items into a single string.
    #[must_use]
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns `true` if this message contains any text content.
    #[must_use]
    pub fn has_text(&self) -> bool {
        self.content
            .iter()
            .any(|c| matches!(c, ContentItem::Text { .. }))
    }

    /// The sender's platform id, if a sender is present.
    #[must_use]
    pub fn sender_id(&self) -> Option<&str> {
        self.sender.as_ref().map(|s| s.id.as_str())
    }

    /// The chat's platform id. Empty string on a default/uninitialised chat.
    #[must_use]
    pub fn chat_id(&self) -> &str {
        &self.chat.id
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub source: String,
    /// Free-form metadata. Used by `PluginLoaded`/`PluginUnloaded` to carry
    /// the plugin name; otherwise typically empty.
    pub data: String,
    /// `Some` iff `event_type == EventType::Command`.
    pub command: Option<Command>,
    /// `Some` iff this event carries message-shaped data.
    pub message: Option<Message>,
    /// Originating plugin name. `Some` indicates the origin expects a response
    /// routed back to it. (Renamed from `sender`; distinct from `source`, the
    /// adapter name, and from `Message::sender`, the human sender.)
    pub reply_plugin: Option<String>,
    /// Target plugin name for directed routing; `None` broadcasts to all plugins.
    /// (Renamed from `receiver`.)
    pub target_plugin: Option<String>,
}

impl Event {
    /// Build an [`EventType::Command`] event.
    pub fn command(
        source: impl Into<String>,
        cmd: impl Into<String>,
        args: impl Into<String>,
    ) -> Self {
        Self {
            event_type: EventType::Command,
            source: source.into(),
            data: String::new(),
            command: Some(Command {
                cmd: cmd.into(),
                args: args.into(),
            }),
            message: None,
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::Message`] event with a text content item.
    pub fn message(source: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            event_type: EventType::Message,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                content: vec![ContentItem::text(text)],
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::Message`] event with formatted text content.
    pub fn formatted_message(
        source: impl Into<String>,
        text: impl Into<String>,
        format: TextFormat,
    ) -> Self {
        Self {
            event_type: EventType::Message,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                content: vec![ContentItem::formatted_text(text, format)],
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::Message`] event with a file content item.
    pub fn file_message(
        source: impl Into<String>,
        file: FileSource,
        file_name: Option<String>,
    ) -> Self {
        Self {
            event_type: EventType::Message,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                content: vec![ContentItem::File {
                    source: file,
                    file_name,
                    file_id: None,
                }],
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::MessageSent`] event.
    ///
    /// `platform_message_id` is the native adapter message id. `request_id`, when
    /// present, is the outgoing framework message id supplied by the sender.
    pub fn message_sent(
        source: impl Into<String>,
        platform_message_id: impl Into<String>,
        request_id: Option<String>,
    ) -> Self {
        Self {
            event_type: EventType::MessageSent,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                id: Some(platform_message_id.into()),
                reply_to: request_id,
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::MessageEdit`] event that replaces the text of an
    /// already-sent message identified by its native adapter id.
    pub fn message_edit(
        source: impl Into<String>,
        platform_message_id: impl Into<String>,
        text: impl Into<String>,
        format: Option<TextFormat>,
    ) -> Self {
        Self {
            event_type: EventType::MessageEdit,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                id: Some(platform_message_id.into()),
                content: vec![ContentItem::Text {
                    text: text.into(),
                    format,
                }],
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build an [`EventType::MessageDelete`] event.
    pub fn message_delete(
        source: impl Into<String>,
        platform_message_id: impl Into<String>,
    ) -> Self {
        Self {
            event_type: EventType::MessageDelete,
            source: source.into(),
            data: String::new(),
            command: None,
            message: Some(Message {
                id: Some(platform_message_id.into()),
                ..Default::default()
            }),
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Build a typed event with no structured payload (plugin lifecycle,
    /// `Other`, etc.). `data` carries free-form context such as a plugin
    /// name.
    pub fn typed(
        event_type: EventType,
        source: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            source: source.into(),
            data: data.into(),
            command: None,
            message: None,
            reply_plugin: None,
            target_plugin: None,
        }
    }

    /// Set the originating plugin name (signals that a response is expected).
    pub fn with_reply_plugin(mut self, plugin: impl Into<String>) -> Self {
        self.reply_plugin = Some(plugin.into());
        self
    }

    /// Set the target plugin name (directed routing, no broadcast).
    pub fn with_target_plugin(mut self, plugin: impl Into<String>) -> Self {
        self.target_plugin = Some(plugin.into());
        self
    }
}

#[cfg(test)]
#[path = "../tests/unit/event_tests.rs"]
mod event_tests;
