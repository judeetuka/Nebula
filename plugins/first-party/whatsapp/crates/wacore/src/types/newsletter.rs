use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use wacore_binary::jid::{Jid, MessageId, MessageServerId};
use waproto::whatsapp as wa;

/// The key type used when querying newsletter info via MEX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsletterKeyType {
    /// Look up by JID (e.g. `120363...@newsletter`).
    #[serde(rename = "JID")]
    Jid,
    /// Look up by invite code (the path segment from the channel link).
    #[serde(rename = "INVITE")]
    Invite,
}

/// Prefix for WhatsApp channel invite links.
pub const NEWSLETTER_LINK_PREFIX: &str = "https://whatsapp.com/channel/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewsletterVerificationState {
    Verified,
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewsletterPrivacy {
    Private,
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NewsletterReactionsMode {
    All,
    Basic,
    None,
    Blocklist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewsletterState {
    Active,
    Suspended,
    Geosuspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedNewsletterState {
    pub r#type: NewsletterState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewsletterMuteState {
    On,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewsletterRole {
    Subscriber,
    Guest,
    Admin,
    Owner,
}

/// Top-level newsletter metadata returned by MEX queries.
///
/// The `id` field is serialised as a plain JID string in MEX JSON responses
/// (e.g. `"120363...@newsletter"`), so we use a custom deserialiser that
/// parses the string into a structured [`Jid`].
#[derive(Debug, Clone, Serialize)]
pub struct NewsletterMetadata {
    pub id: Jid,
    pub state: WrappedNewsletterState,
    pub thread_metadata: NewsletterThreadMetadata,
    pub viewer_metadata: Option<NewsletterViewerMetadata>,
}

/// Manual [`Deserialize`] implementation because the `id` field arrives as a
/// plain JID string from MEX JSON, not as a structured `{user, server, ...}`
/// object.
impl<'de> Deserialize<'de> for NewsletterMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            id: String,
            state: WrappedNewsletterState,
            thread_metadata: NewsletterThreadMetadata,
            viewer_metadata: Option<NewsletterViewerMetadata>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let id = Jid::try_from(raw.id.as_str()).map_err(serde::de::Error::custom)?;

        Ok(NewsletterMetadata {
            id,
            state: raw.state,
            thread_metadata: raw.thread_metadata,
            viewer_metadata: raw.viewer_metadata,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterViewerMetadata {
    pub mute: NewsletterMuteState,
    pub role: NewsletterRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterReactionSettings {
    pub value: NewsletterReactionsMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterSettings {
    pub reaction_codes: NewsletterReactionSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterThreadMetadata {
    #[serde(with = "chrono::serde::ts_seconds")]
    pub creation_time: DateTime<Utc>,
    #[serde(rename = "invite")]
    pub invite_code: String,
    pub name: NewsletterText,
    pub description: NewsletterText,
    #[serde(rename = "subscribers_count")]
    pub subscriber_count: i32,
    #[serde(rename = "verification")]
    pub verification_state: NewsletterVerificationState,
    pub picture_url: Option<String>,
    pub preview_url: Option<String>,
    pub settings: NewsletterSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterText {
    pub text: String,
    pub id: String,
    #[serde(with = "chrono::serde::ts_microseconds")]
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewsletterMessage {
    pub message_server_id: MessageServerId,
    pub message_id: MessageId,
    pub r#type: String,
    pub timestamp: DateTime<Utc>,
    pub views_count: i32,
    pub reaction_counts: HashMap<String, i32>,
    pub message: Option<wa::Message>,
}
