//! Privacy settings IQ specification.
//!
//! Fetches, caches, sets, and handles push notifications for the user's
//! privacy settings. Also supports setting the default disappearing-message
//! timer.
//!
//! ## Wire Format
//! ```xml
//! <!-- Fetch (get) -->
//! <iq xmlns="privacy" type="get" to="s.whatsapp.net" id="...">
//!   <privacy/>
//! </iq>
//!
//! <!-- Fetch response -->
//! <iq from="s.whatsapp.net" id="..." type="result">
//!   <privacy>
//!     <category name="last" value="all"/>
//!     <category name="online" value="all"/>
//!     <category name="profile" value="contacts"/>
//!     <category name="status" value="contacts"/>
//!     <category name="groupadd" value="contacts"/>
//!     <category name="readreceipts" value="all"/>
//!     <category name="calladd" value="all"/>
//!     ...
//!   </privacy>
//! </iq>
//!
//! <!-- Set a single setting -->
//! <iq xmlns="privacy" type="set" to="s.whatsapp.net" id="...">
//!   <privacy>
//!     <category name="last" value="contacts"/>
//!   </privacy>
//! </iq>
//!
//! <!-- Set default disappearing timer -->
//! <iq xmlns="disappearing_mode" type="set" to="s.whatsapp.net" id="...">
//!   <disappearing_mode duration="86400"/>
//! </iq>
//! ```
//!
//! Verified against WhatsApp Web JS (WAWebQueryPrivacy) and whatsmeow
//! `privacysettings.go`.

use std::time::Duration;

use crate::iq::spec::IqSpec;
use crate::request::InfoQuery;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};

/// IQ namespace for privacy settings.
pub const PRIVACY_NAMESPACE: &str = "privacy";

/// IQ namespace for disappearing mode.
pub const DISAPPEARING_MODE_NAMESPACE: &str = "disappearing_mode";

// ── Privacy category ────────────────────────────────────────────────────────

/// Privacy setting category name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrivacyCategory {
    /// Last seen visibility
    Last,
    /// Online status visibility
    Online,
    /// Profile photo visibility
    Profile,
    /// Status/about visibility
    Status,
    /// Group add permissions
    GroupAdd,
    /// Read receipts
    ReadReceipts,
    /// Call add permissions
    CallAdd,
    /// Default disappearing timer (duration in seconds, "0" = off)
    Disappearing,
    /// Who can send messages (all / contacts)
    Messages,
    /// Advanced privacy defense mode (on_standard / off)
    Defense,
    /// Sticker privacy (contacts / contact_allowlist / none)
    Stickers,
    /// Other/unknown category
    Other(String),
}

impl PrivacyCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Last => "last",
            Self::Online => "online",
            Self::Profile => "profile",
            Self::Status => "status",
            Self::GroupAdd => "groupadd",
            Self::ReadReceipts => "readreceipts",
            Self::CallAdd => "calladd",
            Self::Disappearing => "disappearing",
            Self::Messages => "messages",
            Self::Defense => "defense",
            Self::Stickers => "stickers",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl From<&str> for PrivacyCategory {
    fn from(s: &str) -> Self {
        match s {
            "last" => Self::Last,
            "online" => Self::Online,
            "profile" => Self::Profile,
            "status" => Self::Status,
            "groupadd" => Self::GroupAdd,
            "readreceipts" => Self::ReadReceipts,
            "calladd" => Self::CallAdd,
            "disappearing" => Self::Disappearing,
            "messages" => Self::Messages,
            "defense" => Self::Defense,
            "stickers" => Self::Stickers,
            other => Self::Other(other.to_string()),
        }
    }
}

// ── Privacy value ───────────────────────────────────────────────────────────

/// Privacy setting value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyValue {
    /// Visible to everyone
    All,
    /// Visible only to contacts
    Contacts,
    /// Not visible to anyone
    None,
    /// Visible to contacts except specific list
    ContactBlacklist,
    /// Visible to contacts on an allow list (for stickers)
    ContactAllowlist,
    /// Match their settings (for online/last)
    MatchLastSeen,
    /// Known contacts only (for calladd)
    Known,
    /// Defense mode enabled at standard level
    OnStandard,
    /// Setting explicitly disabled (for defense)
    Off,
    /// Other/unknown value
    Other(String),
}

impl PrivacyValue {
    pub fn as_str(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Contacts => "contacts",
            Self::None => "none",
            Self::ContactBlacklist => "contact_blacklist",
            Self::ContactAllowlist => "contact_allowlist",
            Self::MatchLastSeen => "match_last_seen",
            Self::Known => "known",
            Self::OnStandard => "on_standard",
            Self::Off => "off",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl From<&str> for PrivacyValue {
    fn from(s: &str) -> Self {
        match s {
            "all" => Self::All,
            "contacts" => Self::Contacts,
            "none" => Self::None,
            "contact_blacklist" => Self::ContactBlacklist,
            "contact_allowlist" => Self::ContactAllowlist,
            "match_last_seen" => Self::MatchLastSeen,
            "known" => Self::Known,
            "on_standard" => Self::OnStandard,
            "off" => Self::Off,
            other => Self::Other(other.to_string()),
        }
    }
}

// ── Single privacy setting ──────────────────────────────────────────────────

/// A single privacy setting (category + value pair).
#[derive(Debug, Clone)]
pub struct PrivacySetting {
    /// The category name (e.g., "last", "profile", etc.)
    pub category: PrivacyCategory,
    /// The privacy value (e.g., "all", "contacts", "none")
    pub value: PrivacyValue,
}

// ── Cached privacy settings struct ──────────────────────────────────────────

/// All privacy settings for a user, suitable for caching.
///
/// Mirrors whatsmeow `types.PrivacySettings`. Each field holds the current
/// value for the corresponding category. Fields default to `PrivacyValue::None`
/// which means "not yet fetched" -- callers should use [`FetchPrivacySettingsSpec`]
/// to populate this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacySettings {
    /// Who can add the user to groups (all / contacts / contact_blacklist).
    pub group_add: PrivacyValue,
    /// Last seen visibility (all / contacts / contact_blacklist / none).
    pub last_seen: PrivacyValue,
    /// Status/about visibility (all / contacts / contact_blacklist / none).
    pub status: PrivacyValue,
    /// Profile photo visibility (all / contacts / contact_blacklist / none).
    pub profile: PrivacyValue,
    /// Read receipts (all / none).
    pub read_receipts: PrivacyValue,
    /// Online status visibility (all / match_last_seen).
    pub online: PrivacyValue,
    /// Who can call the user (all / known).
    pub call_add: PrivacyValue,
    /// Default disappearing timer as a string value (seconds, "0" = off).
    pub disappearing: PrivacyValue,
    /// Who can send messages (all / contacts).
    pub messages: PrivacyValue,
    /// Advanced privacy defense mode (on_standard / off).
    pub defense: PrivacyValue,
    /// Sticker privacy (contacts / contact_allowlist / none).
    pub stickers: PrivacyValue,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            group_add: PrivacyValue::None,
            last_seen: PrivacyValue::None,
            status: PrivacyValue::None,
            profile: PrivacyValue::None,
            read_receipts: PrivacyValue::None,
            online: PrivacyValue::None,
            call_add: PrivacyValue::None,
            disappearing: PrivacyValue::None,
            messages: PrivacyValue::None,
            defense: PrivacyValue::None,
            stickers: PrivacyValue::None,
        }
    }
}

impl PrivacySettings {
    /// Get the value for a given category.
    pub fn get(&self, category: &PrivacyCategory) -> &PrivacyValue {
        match category {
            PrivacyCategory::GroupAdd => &self.group_add,
            PrivacyCategory::Last => &self.last_seen,
            PrivacyCategory::Status => &self.status,
            PrivacyCategory::Profile => &self.profile,
            PrivacyCategory::ReadReceipts => &self.read_receipts,
            PrivacyCategory::Online => &self.online,
            PrivacyCategory::CallAdd => &self.call_add,
            PrivacyCategory::Disappearing => &self.disappearing,
            PrivacyCategory::Messages => &self.messages,
            PrivacyCategory::Defense => &self.defense,
            PrivacyCategory::Stickers => &self.stickers,
            PrivacyCategory::Other(_) => &PrivacyValue::None,
        }
    }

    /// Set the value for a given category. Returns `true` if the category was
    /// recognized and updated, `false` for unknown categories.
    pub fn set(&mut self, category: &PrivacyCategory, value: PrivacyValue) -> bool {
        match category {
            PrivacyCategory::GroupAdd => self.group_add = value,
            PrivacyCategory::Last => self.last_seen = value,
            PrivacyCategory::Status => self.status = value,
            PrivacyCategory::Profile => self.profile = value,
            PrivacyCategory::ReadReceipts => self.read_receipts = value,
            PrivacyCategory::Online => self.online = value,
            PrivacyCategory::CallAdd => self.call_add = value,
            PrivacyCategory::Disappearing => self.disappearing = value,
            PrivacyCategory::Messages => self.messages = value,
            PrivacyCategory::Defense => self.defense = value,
            PrivacyCategory::Stickers => self.stickers = value,
            PrivacyCategory::Other(_) => return false,
        }
        true
    }
}

// ── Privacy settings change event ───────────────────────────────────────────

/// Indicates which privacy settings changed in a notification.
///
/// Mirrors whatsmeow `events.PrivacySettings`. Returned by
/// [`parse_privacy_settings`] so callers can dispatch targeted events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacySettingsChangedEvent {
    pub group_add_changed: bool,
    pub last_seen_changed: bool,
    pub status_changed: bool,
    pub profile_changed: bool,
    pub read_receipts_changed: bool,
    pub online_changed: bool,
    pub call_add_changed: bool,
    pub disappearing_changed: bool,
    pub messages_changed: bool,
    pub defense_changed: bool,
    pub stickers_changed: bool,
}

impl PrivacySettingsChangedEvent {
    /// Returns `true` if at least one setting changed.
    pub fn any_changed(&self) -> bool {
        self.group_add_changed
            || self.last_seen_changed
            || self.status_changed
            || self.profile_changed
            || self.read_receipts_changed
            || self.online_changed
            || self.call_add_changed
            || self.disappearing_changed
            || self.messages_changed
            || self.defense_changed
            || self.stickers_changed
    }
}

// ── Response type (list of settings) ────────────────────────────────────────

/// Response from privacy settings query (raw list form).
#[derive(Debug, Clone, Default)]
pub struct PrivacySettingsResponse {
    /// The list of privacy settings.
    pub settings: Vec<PrivacySetting>,
}

impl PrivacySettingsResponse {
    /// Get a privacy setting by category.
    pub fn get(&self, category: &PrivacyCategory) -> Option<&PrivacySetting> {
        self.settings.iter().find(|s| &s.category == category)
    }

    /// Get the value for a category.
    pub fn get_value(&self, category: &PrivacyCategory) -> Option<&PrivacyValue> {
        self.get(category).map(|s| &s.value)
    }

    /// Convert this response into a [`PrivacySettings`] cache struct.
    pub fn into_cached(&self) -> PrivacySettings {
        let mut cached = PrivacySettings::default();
        for setting in &self.settings {
            cached.set(&setting.category, setting.value.clone());
        }
        cached
    }
}

// ── Parsing helper ──────────────────────────────────────────────────────────

/// Parse privacy settings from a `<privacy>` node, updating `settings` in
/// place and returning a change event indicating which fields were modified.
///
/// Mirrors whatsmeow `parsePrivacySettings`. This is used both for full
/// fetch responses and incremental push notifications.
pub fn parse_privacy_settings(
    privacy_node: &Node,
    settings: &mut PrivacySettings,
) -> PrivacySettingsChangedEvent {
    let mut evt = PrivacySettingsChangedEvent::default();

    for child in privacy_node.get_children_by_tag("category") {
        let name = match child.attrs().optional_string("name") {
            Some(n) => n,
            Option::None => continue,
        };
        let value = match child.attrs().optional_string("value") {
            Some(v) => v,
            Option::None => continue,
        };

        let category = PrivacyCategory::from(name);
        let privacy_value = PrivacyValue::from(value);

        match &category {
            PrivacyCategory::GroupAdd => evt.group_add_changed = true,
            PrivacyCategory::Last => evt.last_seen_changed = true,
            PrivacyCategory::Status => evt.status_changed = true,
            PrivacyCategory::Profile => evt.profile_changed = true,
            PrivacyCategory::ReadReceipts => evt.read_receipts_changed = true,
            PrivacyCategory::Online => evt.online_changed = true,
            PrivacyCategory::CallAdd => evt.call_add_changed = true,
            PrivacyCategory::Disappearing => evt.disappearing_changed = true,
            PrivacyCategory::Messages => evt.messages_changed = true,
            PrivacyCategory::Defense => evt.defense_changed = true,
            PrivacyCategory::Stickers => evt.stickers_changed = true,
            PrivacyCategory::Other(_) => {}
        }

        settings.set(&category, privacy_value);
    }

    evt
}

// ── IQ specs ────────────────────────────────────────────────────────────────

/// Fetches privacy settings from the server (GET).
///
/// Corresponds to whatsmeow `TryFetchPrivacySettings`. The caller is
/// responsible for caching the result; this spec only handles the wire
/// format.
#[derive(Debug, Clone, Default)]
pub struct FetchPrivacySettingsSpec;

impl FetchPrivacySettingsSpec {
    pub fn new() -> Self {
        Self
    }
}

impl IqSpec for FetchPrivacySettingsSpec {
    type Response = PrivacySettings;

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::get(
            PRIVACY_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(
                vec![NodeBuilder::new("privacy").build()],
            )),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        use crate::iq::node::required_child;

        let privacy_node = required_child(response, "privacy")?;
        let mut settings = PrivacySettings::default();
        parse_privacy_settings(privacy_node, &mut settings);
        Ok(settings)
    }
}

/// Sets a single privacy setting on the server (SET).
///
/// Corresponds to whatsmeow `SetPrivacySetting`. After a successful set,
/// the caller should update their cached [`PrivacySettings`] with the new
/// value.
#[derive(Debug, Clone)]
pub struct SetPrivacySettingSpec {
    pub category: PrivacyCategory,
    pub value: PrivacyValue,
}

impl SetPrivacySettingSpec {
    pub fn new(category: PrivacyCategory, value: PrivacyValue) -> Self {
        Self { category, value }
    }
}

impl IqSpec for SetPrivacySettingSpec {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::set(
            PRIVACY_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![NodeBuilder::new("privacy")
                .children([NodeBuilder::new("category")
                    .attr("name", self.category.as_str())
                    .attr("value", self.value.as_str())
                    .build()])
                .build()])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response, anyhow::Error> {
        // Set operations only need a successful result ack.
        Ok(())
    }
}

/// Sets the default disappearing-message timer on the server.
///
/// Corresponds to whatsmeow `SetDefaultDisappearingTimer`. A duration of
/// zero disables disappearing messages by default.
#[derive(Debug, Clone)]
pub struct SetDefaultDisappearingTimerSpec {
    pub duration: Duration,
}

impl SetDefaultDisappearingTimerSpec {
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }

    /// Convenience: disable disappearing messages (duration = 0).
    pub fn disabled() -> Self {
        Self {
            duration: Duration::ZERO,
        }
    }
}

impl IqSpec for SetDefaultDisappearingTimerSpec {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::set(
            DISAPPEARING_MODE_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![NodeBuilder::new(
                "disappearing_mode",
            )
            .attr("duration", self.duration.as_secs().to_string())
            .build()])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response, anyhow::Error> {
        Ok(())
    }
}

// ── Legacy compat alias ─────────────────────────────────────────────────────

/// Legacy alias for [`FetchPrivacySettingsSpec`].
///
/// Earlier code used this name; kept for backward compatibility. New code
/// should prefer [`FetchPrivacySettingsSpec`].
pub type PrivacySettingsSpec = FetchPrivacySettingsSpec;

/// Alias following the `Iq` suffix convention used in other modules (e.g.,
/// `GroupQueryIq`, `SetGroupSubjectIq`). Equivalent to [`SetPrivacySettingSpec`].
pub type SetPrivacySettingIq = SetPrivacySettingSpec;

// ── Notification handler ────────────────────────────────────────────────────

/// Handle a server push notification about changed privacy settings.
///
/// Mirrors whatsmeow `handlePrivacySettingsNotification`. Parses the
/// `<privacy>` child from the notification node and applies the changes to
/// the given settings struct. Returns the change event.
///
/// The caller is responsible for dispatching the event and storing the
/// updated settings.
pub fn handle_privacy_settings_notification(
    notification_node: &Node,
    settings: &mut PrivacySettings,
) -> Result<PrivacySettingsChangedEvent, anyhow::Error> {
    use crate::iq::node::required_child;

    let privacy_node = required_child(notification_node, "privacy")?;
    let evt = parse_privacy_settings(privacy_node, settings);
    Ok(evt)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── PrivacyCategory ─────────────────────────────────────────────────

    #[test]
    fn test_privacy_category_from_str() {
        assert_eq!(PrivacyCategory::from("last"), PrivacyCategory::Last);
        assert_eq!(PrivacyCategory::from("online"), PrivacyCategory::Online);
        assert_eq!(PrivacyCategory::from("profile"), PrivacyCategory::Profile);
        assert_eq!(PrivacyCategory::from("status"), PrivacyCategory::Status);
        assert_eq!(PrivacyCategory::from("groupadd"), PrivacyCategory::GroupAdd);
        assert_eq!(
            PrivacyCategory::from("readreceipts"),
            PrivacyCategory::ReadReceipts
        );
        assert_eq!(PrivacyCategory::from("calladd"), PrivacyCategory::CallAdd);
        assert_eq!(
            PrivacyCategory::from("disappearing"),
            PrivacyCategory::Disappearing
        );
        assert_eq!(
            PrivacyCategory::from("messages"),
            PrivacyCategory::Messages
        );
        assert_eq!(PrivacyCategory::from("defense"), PrivacyCategory::Defense);
        assert_eq!(
            PrivacyCategory::from("stickers"),
            PrivacyCategory::Stickers
        );
        assert_eq!(
            PrivacyCategory::from("unknown"),
            PrivacyCategory::Other("unknown".to_string())
        );
    }

    #[test]
    fn test_privacy_category_as_str_roundtrip() {
        let categories = [
            PrivacyCategory::Last,
            PrivacyCategory::Online,
            PrivacyCategory::Profile,
            PrivacyCategory::Status,
            PrivacyCategory::GroupAdd,
            PrivacyCategory::ReadReceipts,
            PrivacyCategory::CallAdd,
            PrivacyCategory::Disappearing,
            PrivacyCategory::Messages,
            PrivacyCategory::Defense,
            PrivacyCategory::Stickers,
        ];
        for cat in &categories {
            assert_eq!(&PrivacyCategory::from(cat.as_str()), cat);
        }
    }

    // ── PrivacyValue ────────────────────────────────────────────────────

    #[test]
    fn test_privacy_value_from_str() {
        assert_eq!(PrivacyValue::from("all"), PrivacyValue::All);
        assert_eq!(PrivacyValue::from("contacts"), PrivacyValue::Contacts);
        assert_eq!(PrivacyValue::from("none"), PrivacyValue::None);
        assert_eq!(
            PrivacyValue::from("contact_blacklist"),
            PrivacyValue::ContactBlacklist
        );
        assert_eq!(
            PrivacyValue::from("contact_allowlist"),
            PrivacyValue::ContactAllowlist
        );
        assert_eq!(
            PrivacyValue::from("match_last_seen"),
            PrivacyValue::MatchLastSeen
        );
        assert_eq!(PrivacyValue::from("known"), PrivacyValue::Known);
        assert_eq!(
            PrivacyValue::from("on_standard"),
            PrivacyValue::OnStandard
        );
        assert_eq!(PrivacyValue::from("off"), PrivacyValue::Off);
        assert_eq!(
            PrivacyValue::from("unknown"),
            PrivacyValue::Other("unknown".to_string())
        );
    }

    #[test]
    fn test_privacy_value_as_str_roundtrip() {
        let values = [
            PrivacyValue::All,
            PrivacyValue::Contacts,
            PrivacyValue::None,
            PrivacyValue::ContactBlacklist,
            PrivacyValue::ContactAllowlist,
            PrivacyValue::MatchLastSeen,
            PrivacyValue::Known,
            PrivacyValue::OnStandard,
            PrivacyValue::Off,
        ];
        for val in &values {
            assert_eq!(&PrivacyValue::from(val.as_str()), val);
        }
    }

    // ── PrivacySettings (cached struct) ─────────────────────────────────

    #[test]
    fn test_privacy_settings_default() {
        let settings = PrivacySettings::default();
        assert_eq!(settings.group_add, PrivacyValue::None);
        assert_eq!(settings.last_seen, PrivacyValue::None);
        assert_eq!(settings.online, PrivacyValue::None);
        assert_eq!(settings.call_add, PrivacyValue::None);
        assert_eq!(settings.messages, PrivacyValue::None);
        assert_eq!(settings.defense, PrivacyValue::None);
        assert_eq!(settings.stickers, PrivacyValue::None);
    }

    #[test]
    fn test_privacy_settings_get_set() {
        let mut settings = PrivacySettings::default();

        assert!(settings.set(&PrivacyCategory::Last, PrivacyValue::All));
        assert_eq!(settings.get(&PrivacyCategory::Last), &PrivacyValue::All);

        assert!(settings.set(&PrivacyCategory::GroupAdd, PrivacyValue::Contacts));
        assert_eq!(
            settings.get(&PrivacyCategory::GroupAdd),
            &PrivacyValue::Contacts
        );

        assert!(settings.set(&PrivacyCategory::Online, PrivacyValue::MatchLastSeen));
        assert_eq!(
            settings.get(&PrivacyCategory::Online),
            &PrivacyValue::MatchLastSeen
        );

        assert!(settings.set(&PrivacyCategory::CallAdd, PrivacyValue::Known));
        assert_eq!(
            settings.get(&PrivacyCategory::CallAdd),
            &PrivacyValue::Known
        );

        assert!(settings.set(&PrivacyCategory::Messages, PrivacyValue::Contacts));
        assert_eq!(
            settings.get(&PrivacyCategory::Messages),
            &PrivacyValue::Contacts
        );

        assert!(settings.set(&PrivacyCategory::Defense, PrivacyValue::OnStandard));
        assert_eq!(
            settings.get(&PrivacyCategory::Defense),
            &PrivacyValue::OnStandard
        );

        assert!(settings.set(&PrivacyCategory::Stickers, PrivacyValue::ContactAllowlist));
        assert_eq!(
            settings.get(&PrivacyCategory::Stickers),
            &PrivacyValue::ContactAllowlist
        );

        // Unknown categories return false and read as None.
        assert!(!settings.set(&PrivacyCategory::Other("mystery".into()), PrivacyValue::All));
        assert_eq!(
            settings.get(&PrivacyCategory::Other("mystery".into())),
            &PrivacyValue::None
        );
    }

    // ── parse_privacy_settings ──────────────────────────────────────────

    #[test]
    fn test_parse_privacy_settings_full() {
        let privacy_node = NodeBuilder::new("privacy")
            .children([
                NodeBuilder::new("category")
                    .attr("name", "last")
                    .attr("value", "contacts")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "online")
                    .attr("value", "match_last_seen")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "profile")
                    .attr("value", "all")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "status")
                    .attr("value", "contact_blacklist")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "groupadd")
                    .attr("value", "contacts")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "readreceipts")
                    .attr("value", "all")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "calladd")
                    .attr("value", "known")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "messages")
                    .attr("value", "contacts")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "defense")
                    .attr("value", "on_standard")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "stickers")
                    .attr("value", "contact_allowlist")
                    .build(),
            ])
            .build();

        let mut settings = PrivacySettings::default();
        let evt = parse_privacy_settings(&privacy_node, &mut settings);

        assert_eq!(settings.last_seen, PrivacyValue::Contacts);
        assert_eq!(settings.online, PrivacyValue::MatchLastSeen);
        assert_eq!(settings.profile, PrivacyValue::All);
        assert_eq!(settings.status, PrivacyValue::ContactBlacklist);
        assert_eq!(settings.group_add, PrivacyValue::Contacts);
        assert_eq!(settings.read_receipts, PrivacyValue::All);
        assert_eq!(settings.call_add, PrivacyValue::Known);
        assert_eq!(settings.messages, PrivacyValue::Contacts);
        assert_eq!(settings.defense, PrivacyValue::OnStandard);
        assert_eq!(settings.stickers, PrivacyValue::ContactAllowlist);

        assert!(evt.last_seen_changed);
        assert!(evt.online_changed);
        assert!(evt.profile_changed);
        assert!(evt.status_changed);
        assert!(evt.group_add_changed);
        assert!(evt.read_receipts_changed);
        assert!(evt.call_add_changed);
        assert!(!evt.disappearing_changed);
        assert!(evt.messages_changed);
        assert!(evt.defense_changed);
        assert!(evt.stickers_changed);
        assert!(evt.any_changed());
    }

    #[test]
    fn test_parse_privacy_settings_partial_update() {
        let mut settings = PrivacySettings {
            group_add: PrivacyValue::All,
            last_seen: PrivacyValue::All,
            status: PrivacyValue::All,
            profile: PrivacyValue::All,
            read_receipts: PrivacyValue::All,
            online: PrivacyValue::All,
            call_add: PrivacyValue::All,
            disappearing: PrivacyValue::None,
            messages: PrivacyValue::All,
            defense: PrivacyValue::Off,
            stickers: PrivacyValue::Contacts,
        };

        // Only update last seen.
        let notification_node = NodeBuilder::new("privacy")
            .children([NodeBuilder::new("category")
                .attr("name", "last")
                .attr("value", "none")
                .build()])
            .build();

        let evt = parse_privacy_settings(&notification_node, &mut settings);

        assert_eq!(settings.last_seen, PrivacyValue::None);
        // Others unchanged.
        assert_eq!(settings.group_add, PrivacyValue::All);
        assert_eq!(settings.profile, PrivacyValue::All);

        assert!(evt.last_seen_changed);
        assert!(!evt.group_add_changed);
        assert!(!evt.profile_changed);
        assert!(evt.any_changed());
    }

    #[test]
    fn test_parse_privacy_settings_skips_malformed() {
        let privacy_node = NodeBuilder::new("privacy")
            .children([
                // Missing value attribute -- should be skipped.
                NodeBuilder::new("category").attr("name", "last").build(),
                // Valid entry.
                NodeBuilder::new("category")
                    .attr("name", "profile")
                    .attr("value", "contacts")
                    .build(),
                // Not a category node -- should be skipped by get_children_by_tag.
                NodeBuilder::new("something_else")
                    .attr("name", "status")
                    .attr("value", "all")
                    .build(),
            ])
            .build();

        let mut settings = PrivacySettings::default();
        let evt = parse_privacy_settings(&privacy_node, &mut settings);

        // "last" was skipped (missing value).
        assert_eq!(settings.last_seen, PrivacyValue::None);
        // "profile" was parsed.
        assert_eq!(settings.profile, PrivacyValue::Contacts);
        // "something_else" was filtered out.
        assert_eq!(settings.status, PrivacyValue::None);

        assert!(!evt.last_seen_changed);
        assert!(evt.profile_changed);
        assert!(!evt.status_changed);
    }

    #[test]
    fn test_parse_empty_privacy_node() {
        let privacy_node = NodeBuilder::new("privacy").build();
        let mut settings = PrivacySettings::default();
        let evt = parse_privacy_settings(&privacy_node, &mut settings);

        assert!(!evt.any_changed());
        assert_eq!(settings, PrivacySettings::default());
    }

    // ── PrivacySettingsResponse ─────────────────────────────────────────

    #[test]
    fn test_privacy_settings_response_get() {
        let response = PrivacySettingsResponse {
            settings: vec![
                PrivacySetting {
                    category: PrivacyCategory::Last,
                    value: PrivacyValue::All,
                },
                PrivacySetting {
                    category: PrivacyCategory::Profile,
                    value: PrivacyValue::Contacts,
                },
            ],
        };

        assert_eq!(
            response.get_value(&PrivacyCategory::Last),
            Some(&PrivacyValue::All)
        );
        assert_eq!(
            response.get_value(&PrivacyCategory::Profile),
            Some(&PrivacyValue::Contacts)
        );
        assert_eq!(response.get_value(&PrivacyCategory::Online), Option::None);
    }

    #[test]
    fn test_privacy_settings_response_into_cached() {
        let response = PrivacySettingsResponse {
            settings: vec![
                PrivacySetting {
                    category: PrivacyCategory::Last,
                    value: PrivacyValue::All,
                },
                PrivacySetting {
                    category: PrivacyCategory::GroupAdd,
                    value: PrivacyValue::Contacts,
                },
                PrivacySetting {
                    category: PrivacyCategory::CallAdd,
                    value: PrivacyValue::Known,
                },
            ],
        };

        let cached = response.into_cached();
        assert_eq!(cached.last_seen, PrivacyValue::All);
        assert_eq!(cached.group_add, PrivacyValue::Contacts);
        assert_eq!(cached.call_add, PrivacyValue::Known);
        // Unset fields remain default.
        assert_eq!(cached.online, PrivacyValue::None);
        assert_eq!(cached.profile, PrivacyValue::None);
    }

    // ── FetchPrivacySettingsSpec (GET) ───────────────────────────────────

    #[test]
    fn test_fetch_privacy_settings_spec_build_iq() {
        let spec = FetchPrivacySettingsSpec::new();
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, PRIVACY_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Get);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "privacy");
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_fetch_privacy_settings_spec_parse_response() {
        let spec = FetchPrivacySettingsSpec::new();
        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("privacy")
                .children([
                    NodeBuilder::new("category")
                        .attr("name", "last")
                        .attr("value", "all")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "profile")
                        .attr("value", "contacts")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "status")
                        .attr("value", "none")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "calladd")
                        .attr("value", "known")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "online")
                        .attr("value", "match_last_seen")
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.last_seen, PrivacyValue::All);
        assert_eq!(result.profile, PrivacyValue::Contacts);
        assert_eq!(result.status, PrivacyValue::None);
        assert_eq!(result.call_add, PrivacyValue::Known);
        assert_eq!(result.online, PrivacyValue::MatchLastSeen);
        // Not in response, should be default.
        assert_eq!(result.group_add, PrivacyValue::None);
    }

    #[test]
    fn test_fetch_privacy_settings_spec_missing_privacy_node() {
        let spec = FetchPrivacySettingsSpec::new();
        let response = NodeBuilder::new("iq").attr("type", "result").build();

        let result = spec.parse_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("privacy"));
    }

    // ── Legacy alias ────────────────────────────────────────────────────

    #[test]
    fn test_privacy_settings_spec_alias() {
        // PrivacySettingsSpec should be usable as FetchPrivacySettingsSpec.
        let spec = PrivacySettingsSpec::new();
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, PRIVACY_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Get);
    }

    // ── SetPrivacySettingSpec (SET) ─────────────────────────────────────

    #[test]
    fn test_set_privacy_setting_spec_build_iq() {
        let spec = SetPrivacySettingSpec::new(PrivacyCategory::Last, PrivacyValue::Contacts);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, PRIVACY_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "privacy");

            let children: Vec<&Node> = nodes[0].get_children_by_tag("category").collect();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].attrs().optional_string("name"), Some("last"));
            assert_eq!(
                children[0].attrs().optional_string("value"),
                Some("contacts")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_set_privacy_setting_spec_all_categories() {
        let test_cases = [
            (
                PrivacyCategory::GroupAdd,
                PrivacyValue::All,
                "groupadd",
                "all",
            ),
            (PrivacyCategory::Last, PrivacyValue::None, "last", "none"),
            (
                PrivacyCategory::Status,
                PrivacyValue::ContactBlacklist,
                "status",
                "contact_blacklist",
            ),
            (
                PrivacyCategory::Profile,
                PrivacyValue::Contacts,
                "profile",
                "contacts",
            ),
            (
                PrivacyCategory::ReadReceipts,
                PrivacyValue::None,
                "readreceipts",
                "none",
            ),
            (
                PrivacyCategory::Online,
                PrivacyValue::MatchLastSeen,
                "online",
                "match_last_seen",
            ),
            (
                PrivacyCategory::CallAdd,
                PrivacyValue::Known,
                "calladd",
                "known",
            ),
            (
                PrivacyCategory::Messages,
                PrivacyValue::Contacts,
                "messages",
                "contacts",
            ),
            (
                PrivacyCategory::Defense,
                PrivacyValue::OnStandard,
                "defense",
                "on_standard",
            ),
            (
                PrivacyCategory::Stickers,
                PrivacyValue::ContactAllowlist,
                "stickers",
                "contact_allowlist",
            ),
        ];

        for (category, value, expected_name, expected_value) in test_cases {
            let spec = SetPrivacySettingSpec::new(category, value);
            let iq = spec.build_iq();

            if let Some(NodeContent::Nodes(nodes)) = &iq.content {
                let children: Vec<&Node> = nodes[0].get_children_by_tag("category").collect();
                assert_eq!(
                    children[0].attrs().optional_string("name"),
                    Some(expected_name),
                    "category name mismatch for {expected_name}"
                );
                assert_eq!(
                    children[0].attrs().optional_string("value"),
                    Some(expected_value),
                    "category value mismatch for {expected_name}"
                );
            } else {
                panic!("Expected NodeContent::Nodes for {expected_name}");
            }
        }
    }

    #[test]
    fn test_set_privacy_setting_spec_parse_response() {
        let spec = SetPrivacySettingSpec::new(PrivacyCategory::Last, PrivacyValue::Contacts);
        let response = NodeBuilder::new("iq").attr("type", "result").build();
        assert!(spec.parse_response(&response).is_ok());
    }

    // ── SetDefaultDisappearingTimerSpec ──────────────────────────────────

    #[test]
    fn test_set_disappearing_timer_spec_build_iq() {
        let spec = SetDefaultDisappearingTimerSpec::new(Duration::from_secs(86400));
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, DISAPPEARING_MODE_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "disappearing_mode");
            assert_eq!(nodes[0].attrs().optional_string("duration"), Some("86400"));
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_set_disappearing_timer_spec_disabled() {
        let spec = SetDefaultDisappearingTimerSpec::disabled();
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].attrs().optional_string("duration"), Some("0"));
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_set_disappearing_timer_spec_7_days() {
        let spec = SetDefaultDisappearingTimerSpec::new(Duration::from_secs(7 * 86400));
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].attrs().optional_string("duration"), Some("604800"));
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_set_disappearing_timer_spec_parse_response() {
        let spec = SetDefaultDisappearingTimerSpec::new(Duration::from_secs(86400));
        let response = NodeBuilder::new("iq").attr("type", "result").build();
        assert!(spec.parse_response(&response).is_ok());
    }

    // ── handle_privacy_settings_notification ────────────────────────────

    #[test]
    fn test_handle_privacy_settings_notification() {
        let notification = NodeBuilder::new("notification")
            .attr("type", "privacy")
            .children([NodeBuilder::new("privacy")
                .children([
                    NodeBuilder::new("category")
                        .attr("name", "last")
                        .attr("value", "none")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "readreceipts")
                        .attr("value", "none")
                        .build(),
                ])
                .build()])
            .build();

        let mut settings = PrivacySettings {
            group_add: PrivacyValue::All,
            last_seen: PrivacyValue::All,
            status: PrivacyValue::All,
            profile: PrivacyValue::All,
            read_receipts: PrivacyValue::All,
            online: PrivacyValue::All,
            call_add: PrivacyValue::All,
            disappearing: PrivacyValue::None,
            messages: PrivacyValue::All,
            defense: PrivacyValue::Off,
            stickers: PrivacyValue::Contacts,
        };

        let evt = handle_privacy_settings_notification(&notification, &mut settings).unwrap();

        // Changed fields.
        assert_eq!(settings.last_seen, PrivacyValue::None);
        assert_eq!(settings.read_receipts, PrivacyValue::None);
        assert!(evt.last_seen_changed);
        assert!(evt.read_receipts_changed);

        // Unchanged fields.
        assert_eq!(settings.group_add, PrivacyValue::All);
        assert_eq!(settings.profile, PrivacyValue::All);
        assert_eq!(settings.messages, PrivacyValue::All);
        assert_eq!(settings.defense, PrivacyValue::Off);
        assert_eq!(settings.stickers, PrivacyValue::Contacts);
        assert!(!evt.group_add_changed);
        assert!(!evt.profile_changed);
        assert!(!evt.messages_changed);
        assert!(!evt.defense_changed);
        assert!(!evt.stickers_changed);
    }

    #[test]
    fn test_handle_privacy_settings_notification_missing_privacy() {
        let notification = NodeBuilder::new("notification")
            .attr("type", "privacy")
            .build();

        let mut settings = PrivacySettings::default();
        let result = handle_privacy_settings_notification(&notification, &mut settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("privacy"));
    }

    // ── PrivacySettingsChangedEvent ─────────────────────────────────────

    #[test]
    fn test_changed_event_default_is_empty() {
        let evt = PrivacySettingsChangedEvent::default();
        assert!(!evt.any_changed());
    }

    #[test]
    fn test_changed_event_single_change() {
        let evt = PrivacySettingsChangedEvent {
            call_add_changed: true,
            ..Default::default()
        };
        assert!(evt.any_changed());
    }

    #[test]
    fn test_changed_event_new_categories() {
        let evt_messages = PrivacySettingsChangedEvent {
            messages_changed: true,
            ..Default::default()
        };
        assert!(evt_messages.any_changed());

        let evt_defense = PrivacySettingsChangedEvent {
            defense_changed: true,
            ..Default::default()
        };
        assert!(evt_defense.any_changed());

        let evt_stickers = PrivacySettingsChangedEvent {
            stickers_changed: true,
            ..Default::default()
        };
        assert!(evt_stickers.any_changed());
    }

    // ── SetPrivacySettingIq alias ──────────────────────────────────────

    #[test]
    fn test_set_privacy_setting_iq_alias() {
        // SetPrivacySettingIq should be usable as SetPrivacySettingSpec.
        let spec = SetPrivacySettingIq::new(PrivacyCategory::Defense, PrivacyValue::Off);
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, PRIVACY_NAMESPACE);
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let children: Vec<&Node> = nodes[0].get_children_by_tag("category").collect();
            assert_eq!(children[0].attrs().optional_string("name"), Some("defense"));
            assert_eq!(children[0].attrs().optional_string("value"), Some("off"));
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    // ── New categories in parse + notification ─────────────────────────

    #[test]
    fn test_parse_privacy_settings_new_categories_only() {
        let privacy_node = NodeBuilder::new("privacy")
            .children([
                NodeBuilder::new("category")
                    .attr("name", "messages")
                    .attr("value", "all")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "defense")
                    .attr("value", "on_standard")
                    .build(),
                NodeBuilder::new("category")
                    .attr("name", "stickers")
                    .attr("value", "none")
                    .build(),
            ])
            .build();

        let mut settings = PrivacySettings::default();
        let evt = parse_privacy_settings(&privacy_node, &mut settings);

        assert_eq!(settings.messages, PrivacyValue::All);
        assert_eq!(settings.defense, PrivacyValue::OnStandard);
        assert_eq!(settings.stickers, PrivacyValue::None);

        assert!(evt.messages_changed);
        assert!(evt.defense_changed);
        assert!(evt.stickers_changed);
        // Original categories unchanged.
        assert!(!evt.last_seen_changed);
        assert!(!evt.group_add_changed);
        assert!(!evt.call_add_changed);
    }

    #[test]
    fn test_handle_notification_with_new_categories() {
        let notification = NodeBuilder::new("notification")
            .attr("type", "privacy")
            .children([NodeBuilder::new("privacy")
                .children([
                    NodeBuilder::new("category")
                        .attr("name", "defense")
                        .attr("value", "off")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "stickers")
                        .attr("value", "contact_allowlist")
                        .build(),
                ])
                .build()])
            .build();

        let mut settings = PrivacySettings {
            defense: PrivacyValue::OnStandard,
            stickers: PrivacyValue::None,
            ..Default::default()
        };

        let evt = handle_privacy_settings_notification(&notification, &mut settings).unwrap();

        assert_eq!(settings.defense, PrivacyValue::Off);
        assert_eq!(settings.stickers, PrivacyValue::ContactAllowlist);
        assert!(evt.defense_changed);
        assert!(evt.stickers_changed);
        assert!(!evt.messages_changed);
    }

    #[test]
    fn test_set_privacy_setting_spec_new_values() {
        // defense = off
        let spec = SetPrivacySettingSpec::new(PrivacyCategory::Defense, PrivacyValue::Off);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let children: Vec<&Node> = nodes[0].get_children_by_tag("category").collect();
            assert_eq!(children[0].attrs().optional_string("name"), Some("defense"));
            assert_eq!(children[0].attrs().optional_string("value"), Some("off"));
        } else {
            panic!("Expected NodeContent::Nodes");
        }

        // stickers = contact_allowlist
        let spec =
            SetPrivacySettingSpec::new(PrivacyCategory::Stickers, PrivacyValue::ContactAllowlist);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let children: Vec<&Node> = nodes[0].get_children_by_tag("category").collect();
            assert_eq!(
                children[0].attrs().optional_string("name"),
                Some("stickers")
            );
            assert_eq!(
                children[0].attrs().optional_string("value"),
                Some("contact_allowlist")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_privacy_settings_response_into_cached_with_new_fields() {
        let response = PrivacySettingsResponse {
            settings: vec![
                PrivacySetting {
                    category: PrivacyCategory::Messages,
                    value: PrivacyValue::Contacts,
                },
                PrivacySetting {
                    category: PrivacyCategory::Defense,
                    value: PrivacyValue::OnStandard,
                },
                PrivacySetting {
                    category: PrivacyCategory::Stickers,
                    value: PrivacyValue::ContactAllowlist,
                },
            ],
        };

        let cached = response.into_cached();
        assert_eq!(cached.messages, PrivacyValue::Contacts);
        assert_eq!(cached.defense, PrivacyValue::OnStandard);
        assert_eq!(cached.stickers, PrivacyValue::ContactAllowlist);
        // Unset fields remain default.
        assert_eq!(cached.last_seen, PrivacyValue::None);
        assert_eq!(cached.group_add, PrivacyValue::None);
    }

    #[test]
    fn test_fetch_spec_parse_response_with_new_categories() {
        let spec = FetchPrivacySettingsSpec::new();
        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("privacy")
                .children([
                    NodeBuilder::new("category")
                        .attr("name", "messages")
                        .attr("value", "all")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "defense")
                        .attr("value", "off")
                        .build(),
                    NodeBuilder::new("category")
                        .attr("name", "stickers")
                        .attr("value", "contacts")
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.messages, PrivacyValue::All);
        assert_eq!(result.defense, PrivacyValue::Off);
        assert_eq!(result.stickers, PrivacyValue::Contacts);
    }
}
