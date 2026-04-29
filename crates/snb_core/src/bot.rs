/// Identity snapshot of the running bot.
///
/// Returned by [`crate::context::BotContext::get_me`] so plugins can inspect the bot's
/// display name (and potentially other metadata in the future).
#[derive(Clone)]
pub struct BotInfo {
    pub name: String,
}
