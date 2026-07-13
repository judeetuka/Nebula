//! Newsletter (WhatsApp Channels) support.
//!
//! Ports whatsmeow/newsletter.go — newsletter subscriptions, metadata queries,
//! message fetching, reactions, and management (follow/unfollow/mute/create).
//!
//! Newsletter IQs use two distinct transports:
//!
//! 1. **MEX (Meta Exchange)** — a GraphQL-over-IQ system for metadata and
//!    subscription management. Each query has a numeric `query_id` and a JSON
//!    variables payload:
//!    ```xml
//!    <iq xmlns="w:mex" type="get" to="s.whatsapp.net" id="...">
//!      <query query_id="6563316087068696">{"variables":{...}}</query>
//!    </iq>
//!    ```
//!
//! 2. **Standard `newsletter` namespace IQs** — for message fetching, live
//!    updates, and view receipts:
//!    ```xml
//!    <iq xmlns="newsletter" type="get" to="s.whatsapp.net" id="...">
//!      <messages type="jid" jid="NL_JID" count="25"/>
//!    </iq>
//!    ```

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use prost::Message as ProstMessage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, MessageId, MessageServerId, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};
use waproto::whatsapp as wa;

use crate::iq::mex::{MexQuerySpec, MexResponse};
use crate::iq::spec::IqSpec;
use crate::request::InfoQuery;
use crate::types::message::EditAttribute;
use crate::types::newsletter::{
    NEWSLETTER_LINK_PREFIX, NewsletterKeyType, NewsletterMessage, NewsletterMetadata,
};

// ── MEX query ID constants ─────────────────────────────────────────────────
//
// Mobile (phone-based) query IDs.

const QUERY_FETCH_NEWSLETTER: &str = "6563316087068696";
const QUERY_FETCH_NEWSLETTER_DEHYDRATED: &str = "7272540469429201";
const QUERY_RECOMMENDED_NEWSLETTERS: &str = "7263823273662354";
const QUERY_NEWSLETTERS_DIRECTORY: &str = "6190824427689257";
const QUERY_SUBSCRIBED_NEWSLETTERS: &str = "6388546374527196";
const QUERY_NEWSLETTER_SUBSCRIBERS: &str = "9800646650009898";

const MUTATION_MUTE_NEWSLETTER: &str = "6274038279359549";
const MUTATION_UNMUTE_NEWSLETTER: &str = "6068417879924485";
const MUTATION_UPDATE_NEWSLETTER: &str = "7150902998257522";
const MUTATION_CREATE_NEWSLETTER: &str = "6234210096708695";
const MUTATION_UNFOLLOW_NEWSLETTER: &str = "6392786840836363";
const MUTATION_FOLLOW_NEWSLETTER: &str = "9926858900719341";

// Desktop (macOS / web) query IDs.

const QUERY_FETCH_NEWSLETTER_DESKTOP: &str = "9779843322044422";
const QUERY_RECOMMENDED_NEWSLETTERS_DESKTOP: &str = "27256776790637714";
const QUERY_SUBSCRIBED_NEWSLETTERS_DESKTOP: &str = "8621797084555037";
const QUERY_NEWSLETTER_SUBSCRIBERS_DESKTOP: &str = "25403502652570342";

const MUTATION_MUTE_NEWSLETTER_DESKTOP: &str = "5971669009605755";
const MUTATION_UNMUTE_NEWSLETTER_DESKTOP: &str = "6104029483058502";
const MUTATION_UPDATE_NEWSLETTER_DESKTOP: &str = "7839742399440946";
const MUTATION_CREATE_NEWSLETTER_DESKTOP: &str = "27527996220149684";
const MUTATION_UNFOLLOW_NEWSLETTER_DESKTOP: &str = "8782612271820087";
const MUTATION_FOLLOW_NEWSLETTER_DESKTOP: &str = "8621797084555037";

// ── Query ID translation ───────────────────────────────────────────────────

/// Convert a mobile MEX query ID to the desktop equivalent when the client
/// is running on a desktop platform (macOS / web).
///
/// If the query ID has no desktop mapping, it is returned unchanged.
pub fn convert_query_id(query_id: &str, is_desktop: bool) -> &str {
    if !is_desktop {
        return query_id;
    }
    match query_id {
        QUERY_FETCH_NEWSLETTER => QUERY_FETCH_NEWSLETTER_DESKTOP,
        QUERY_RECOMMENDED_NEWSLETTERS => QUERY_RECOMMENDED_NEWSLETTERS_DESKTOP,
        QUERY_SUBSCRIBED_NEWSLETTERS => QUERY_SUBSCRIBED_NEWSLETTERS_DESKTOP,
        QUERY_NEWSLETTER_SUBSCRIBERS => QUERY_NEWSLETTER_SUBSCRIBERS_DESKTOP,
        MUTATION_MUTE_NEWSLETTER => MUTATION_MUTE_NEWSLETTER_DESKTOP,
        MUTATION_UNMUTE_NEWSLETTER => MUTATION_UNMUTE_NEWSLETTER_DESKTOP,
        MUTATION_UPDATE_NEWSLETTER => MUTATION_UPDATE_NEWSLETTER_DESKTOP,
        MUTATION_CREATE_NEWSLETTER => MUTATION_CREATE_NEWSLETTER_DESKTOP,
        MUTATION_UNFOLLOW_NEWSLETTER => MUTATION_UNFOLLOW_NEWSLETTER_DESKTOP,
        MUTATION_FOLLOW_NEWSLETTER => MUTATION_FOLLOW_NEWSLETTER_DESKTOP,
        other => other,
    }
}

// ── MEX IQ builders (IqSpec implementations) ───────────────────────────────

/// Build a MEX query spec for a given query ID and variables.
///
/// The caller passes `is_desktop` to select the correct query ID variant.
/// This is a thin wrapper around [`MexQuerySpec`] that performs the
/// desktop/mobile ID translation.
fn build_mex_spec(query_id: &str, variables: Value, is_desktop: bool) -> MexQuerySpec {
    let resolved_id = convert_query_id(query_id, is_desktop);
    MexQuerySpec::new(resolved_id, variables)
}

/// Extract the `data` field from a [`MexResponse`], returning an error if
/// it is absent.
fn mex_data(resp: &MexResponse) -> Result<&Value> {
    resp.data
        .as_ref()
        .ok_or_else(|| anyhow!("MEX response contains no data"))
}

// ── Newsletter subscribe live updates ──────────────────────────────────────

/// IQ spec for subscribing to live newsletter updates.
///
/// Wire format:
/// ```xml
/// <iq xmlns="newsletter" type="set" to="NL_JID" id="...">
///   <live_updates/>
/// </iq>
/// ```
///
/// Response contains `<live_updates duration="SECONDS"/>`.
#[derive(Debug, Clone)]
pub struct NewsletterSubscribeLiveUpdatesSpec {
    pub jid: Jid,
}

/// Duration (in seconds) for which the live-update subscription is valid.
#[derive(Debug, Clone)]
pub struct LiveUpdatesDuration {
    pub duration: Duration,
}

impl IqSpec for NewsletterSubscribeLiveUpdatesSpec {
    type Response = LiveUpdatesDuration;

    fn build_iq(&self) -> InfoQuery<'static> {
        let child = NodeBuilder::new("live_updates").build();
        InfoQuery::set(
            "newsletter",
            self.jid.clone(),
            Some(NodeContent::Nodes(vec![child])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let live_updates = response
            .get_optional_child("live_updates")
            .ok_or_else(|| anyhow!("Missing <live_updates> in response"))?;

        let duration_secs = live_updates
            .attrs()
            .optional_u64("duration")
            .unwrap_or(0);

        Ok(LiveUpdatesDuration {
            duration: Duration::from_secs(duration_secs),
        })
    }
}

// ── Newsletter mark viewed ─────────────────────────────────────────────────

/// Build a `<receipt type="view">` node for marking newsletter messages as
/// viewed.
///
/// This increments the view counter on the newsletter message. It is *not*
/// the same as marking the channel as read on other devices (use the normal
/// `MarkRead` receipt for that).
///
/// Wire format:
/// ```xml
/// <receipt to="NL_JID" type="view" id="REQ_ID">
///   <list>
///     <item server_id="123"/>
///     <item server_id="456"/>
///   </list>
/// </receipt>
/// ```
pub fn build_newsletter_mark_viewed(
    jid: &Jid,
    server_ids: &[MessageServerId],
    request_id: &str,
) -> Node {
    let items: Vec<Node> = server_ids
        .iter()
        .map(|sid| {
            NodeBuilder::new("item")
                .attr("server_id", sid.to_string())
                .build()
        })
        .collect();

    NodeBuilder::new("receipt")
        .attr("to", jid.to_string())
        .attr("type", "view")
        .attr("id", request_id)
        .children([NodeBuilder::new("list").children(items).build()])
        .build()
}

// ── Newsletter send reaction ───────────────────────────────────────────────

/// Build a `<message type="reaction">` node for reacting to a newsletter
/// message.
///
/// To *remove* a previously sent reaction, pass an empty string for
/// `reaction` — the node will include an `edit="7"` (sender-revoke)
/// attribute instead of the reaction code.
///
/// Wire format (add reaction):
/// ```xml
/// <message to="NL_JID" id="MSG_ID" server_id="SRV_ID" type="reaction">
///   <reaction code="👍"/>
/// </message>
/// ```
///
/// Wire format (remove reaction):
/// ```xml
/// <message to="NL_JID" id="MSG_ID" server_id="SRV_ID" type="reaction" edit="7">
///   <reaction/>
/// </message>
/// ```
pub fn build_newsletter_reaction(
    jid: &Jid,
    server_id: MessageServerId,
    reaction: &str,
    message_id: &str,
) -> Node {
    let mut reaction_builder = NodeBuilder::new("reaction");
    let mut message_builder = NodeBuilder::new("message")
        .attr("to", jid.to_string())
        .attr("id", message_id)
        .attr("server_id", server_id.to_string())
        .attr("type", "reaction");

    if reaction.is_empty() {
        // Remove reaction — edit = sender revoke ("7")
        message_builder =
            message_builder.attr("edit", EditAttribute::SenderRevoke.to_string_val());
    } else {
        reaction_builder = reaction_builder.attr("code", reaction);
    }

    message_builder
        .children([reaction_builder.build()])
        .build()
}

// ── MEX-based metadata queries ─────────────────────────────────────────────

/// Serde helper for deserialising the `xwa2_newsletter` field from MEX.
#[derive(Debug, Deserialize)]
struct RespGetNewsletterInfo {
    xwa2_newsletter: Option<NewsletterMetadata>,
}

/// Serde helper for deserialising the `xwa2_newsletter_subscribed` field.
#[derive(Debug, Deserialize)]
struct RespGetSubscribedNewsletters {
    xwa2_newsletter_subscribed: Option<Vec<NewsletterMetadata>>,
}

/// Serde helper for deserialising the `xwa2_newsletter_create` field.
#[derive(Debug, Deserialize)]
struct RespCreateNewsletter {
    xwa2_newsletter_create: Option<NewsletterMetadata>,
}

/// Serde helper for deserialising newsletter subscriber edges.
#[derive(Debug, Deserialize)]
struct RespGetNewsletterSubscribers {
    xwa2_newsletter_subscribers: Option<NewsletterSubscribersData>,
}

#[derive(Debug, Deserialize)]
struct NewsletterSubscribersData {
    subscribers: Option<NewsletterSubscriberEdges>,
}

#[derive(Debug, Deserialize)]
struct NewsletterSubscriberEdges {
    edges: Option<Vec<NewsletterSubscriber>>,
}

/// A newsletter subscriber record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterSubscriber {
    pub id: String,
}

/// Build the MEX query spec for fetching newsletter info.
///
/// This is the common implementation behind [`get_newsletter_info_spec`] and
/// [`get_newsletter_info_with_invite_spec`].
fn build_fetch_newsletter_spec(
    key: &str,
    key_type: NewsletterKeyType,
    fetch_viewer_metadata: bool,
    is_desktop: bool,
) -> MexQuerySpec {
    build_mex_spec(
        QUERY_FETCH_NEWSLETTER,
        json!({
            "fetch_creation_time": true,
            "fetch_full_image": true,
            "fetch_viewer_metadata": fetch_viewer_metadata,
            "input": {
                "key": key,
                "type": key_type,
            },
        }),
        is_desktop,
    )
}

/// Parse newsletter metadata from a MEX response.
fn parse_newsletter_info(resp: &MexResponse) -> Result<Option<NewsletterMetadata>> {
    let data = mex_data(resp)?;
    let info: RespGetNewsletterInfo = serde_json::from_value(data.clone())?;
    Ok(info.xwa2_newsletter)
}

/// Build a MEX query spec for fetching newsletter info by JID.
///
/// The returned [`MexQuerySpec`] can be sent through the normal IQ pipeline.
/// After receiving the [`MexResponse`], call [`parse_newsletter_info`] to
/// extract the metadata.
pub fn get_newsletter_info_spec(jid: &Jid, is_desktop: bool) -> MexQuerySpec {
    build_fetch_newsletter_spec(
        &jid.to_string(),
        NewsletterKeyType::Jid,
        true,
        is_desktop,
    )
}

/// Build a MEX query spec for fetching newsletter info by invite link.
///
/// Accepts either the full link (`https://whatsapp.com/channel/...`) or just
/// the invite code portion.
///
/// Note: the response will not contain viewer metadata.
pub fn get_newsletter_info_with_invite_spec(link: &str, is_desktop: bool) -> MexQuerySpec {
    let key = link
        .strip_prefix(NEWSLETTER_LINK_PREFIX)
        .unwrap_or(link);
    build_fetch_newsletter_spec(key, NewsletterKeyType::Invite, false, is_desktop)
}

/// Build a MEX query spec for listing all subscribed newsletters.
pub fn get_subscribed_newsletters_spec(is_desktop: bool) -> MexQuerySpec {
    build_mex_spec(
        QUERY_SUBSCRIBED_NEWSLETTERS,
        json!({}),
        is_desktop,
    )
}

/// Parse subscribed newsletters from a MEX response.
pub fn parse_subscribed_newsletters(resp: &MexResponse) -> Result<Vec<NewsletterMetadata>> {
    let data = mex_data(resp)?;
    let info: RespGetSubscribedNewsletters = serde_json::from_value(data.clone())?;
    Ok(info.xwa2_newsletter_subscribed.unwrap_or_default())
}

/// Build a MEX mutation spec for following (joining) a newsletter.
pub fn follow_newsletter_spec(jid: &Jid, is_desktop: bool) -> MexQuerySpec {
    build_mex_spec(
        MUTATION_FOLLOW_NEWSLETTER,
        json!({ "newsletter_id": jid.to_string() }),
        is_desktop,
    )
}

/// Build a MEX mutation spec for unfollowing (leaving) a newsletter.
pub fn unfollow_newsletter_spec(jid: &Jid, is_desktop: bool) -> MexQuerySpec {
    build_mex_spec(
        MUTATION_UNFOLLOW_NEWSLETTER,
        json!({ "newsletter_id": jid.to_string() }),
        is_desktop,
    )
}

/// Build a MEX mutation spec for muting or unmuting a newsletter.
pub fn newsletter_toggle_mute_spec(jid: &Jid, mute: bool, is_desktop: bool) -> MexQuerySpec {
    let query_id = if mute {
        MUTATION_MUTE_NEWSLETTER
    } else {
        MUTATION_UNMUTE_NEWSLETTER
    };
    build_mex_spec(
        query_id,
        json!({ "newsletter_id": jid.to_string() }),
        is_desktop,
    )
}

// ── Create newsletter ──────────────────────────────────────────────────────

/// Parameters for creating a new WhatsApp channel.
#[derive(Debug, Clone, Serialize)]
pub struct CreateNewsletterParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Base64-encoded JPEG picture data, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<Vec<u8>>,
}

/// Build a MEX mutation spec for creating a newsletter.
pub fn create_newsletter_spec(
    params: &CreateNewsletterParams,
    is_desktop: bool,
) -> MexQuerySpec {
    // The MEX API expects the params nested under `newsletter_input`.
    let variables = json!({
        "newsletter_input": params,
    });
    build_mex_spec(MUTATION_CREATE_NEWSLETTER, variables, is_desktop)
}

/// Parse the created newsletter metadata from a MEX response.
pub fn parse_create_newsletter(resp: &MexResponse) -> Result<Option<NewsletterMetadata>> {
    let data = mex_data(resp)?;
    let info: RespCreateNewsletter = serde_json::from_value(data.clone())?;
    Ok(info.xwa2_newsletter_create)
}

// ── Get newsletter subscribers ─────────────────────────────────────────────

/// Build a MEX query spec for fetching newsletter subscribers.
///
/// The `limit` parameter controls how many subscribers to fetch.
pub fn get_newsletter_subscribers_spec(
    jid: &Jid,
    limit: u32,
    is_desktop: bool,
) -> MexQuerySpec {
    build_mex_spec(
        QUERY_NEWSLETTER_SUBSCRIBERS,
        json!({
            "input": {
                "newsletter_id": jid.to_string(),
                "count": limit,
            },
        }),
        is_desktop,
    )
}

/// Parse newsletter subscribers from a MEX response.
pub fn parse_newsletter_subscribers(resp: &MexResponse) -> Result<Vec<NewsletterSubscriber>> {
    let data = mex_data(resp)?;
    let info: RespGetNewsletterSubscribers = serde_json::from_value(data.clone())?;
    Ok(info
        .xwa2_newsletter_subscribers
        .and_then(|d| d.subscribers)
        .and_then(|s| s.edges)
        .unwrap_or_default())
}

// ── Newsletter messages (standard IQ) ──────────────────────────────────────

/// Parameters for fetching newsletter messages.
#[derive(Debug, Clone, Default)]
pub struct GetNewsletterMessagesParams {
    /// Maximum number of messages to return.
    pub count: Option<u32>,
    /// Fetch messages before this server ID (for pagination).
    pub before: Option<MessageServerId>,
}

/// IQ spec for fetching newsletter messages.
///
/// Wire format:
/// ```xml
/// <iq xmlns="newsletter" type="get" to="s.whatsapp.net" id="...">
///   <messages type="jid" jid="NL_JID" count="25" before="SRV_ID"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct GetNewsletterMessagesSpec {
    pub jid: Jid,
    pub params: GetNewsletterMessagesParams,
}

impl IqSpec for GetNewsletterMessagesSpec {
    type Response = Vec<NewsletterMessage>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let mut builder = NodeBuilder::new("messages")
            .attr("type", "jid")
            .attr("jid", self.jid.to_string());

        if let Some(count) = self.params.count {
            builder = builder.attr("count", count.to_string());
        }
        if let Some(before) = self.params.before {
            builder = builder.attr("before", before.to_string());
        }

        InfoQuery::get(
            "newsletter",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![builder.build()])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let messages_node = response
            .get_optional_child("messages")
            .ok_or_else(|| anyhow!("Missing <messages> in newsletter messages response"))?;

        Ok(parse_newsletter_messages(messages_node))
    }
}

// ── Newsletter message updates (standard IQ) ──────────────────────────────

/// Parameters for fetching newsletter message updates (view/reaction counts).
#[derive(Debug, Clone, Default)]
pub struct GetNewsletterMessageUpdatesParams {
    /// Maximum number of updates to return.
    pub count: Option<u32>,
    /// Only return updates since this timestamp.
    pub since: Option<DateTime<Utc>>,
    /// Only return updates after this server ID.
    pub after: Option<MessageServerId>,
}

/// IQ spec for fetching newsletter message updates.
///
/// These are the same kind of updates that live-update subscriptions trigger
/// (reaction counts, view counts).
///
/// Wire format:
/// ```xml
/// <iq xmlns="newsletter" type="get" to="NL_JID" id="...">
///   <message_updates count="25" since="UNIX_TS" after="SRV_ID"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct GetNewsletterMessageUpdatesSpec {
    pub jid: Jid,
    pub params: GetNewsletterMessageUpdatesParams,
}

impl IqSpec for GetNewsletterMessageUpdatesSpec {
    type Response = Vec<NewsletterMessage>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let mut builder = NodeBuilder::new("message_updates");

        if let Some(count) = self.params.count {
            builder = builder.attr("count", count.to_string());
        }
        if let Some(since) = self.params.since {
            builder = builder.attr("since", since.timestamp().to_string());
        }
        if let Some(after) = self.params.after {
            builder = builder.attr("after", after.to_string());
        }

        InfoQuery::get(
            "newsletter",
            self.jid.clone(),
            Some(NodeContent::Nodes(vec![builder.build()])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let message_updates = response
            .get_optional_child("message_updates")
            .ok_or_else(|| anyhow!("Missing <message_updates> in response"))?;

        let messages_node = message_updates
            .get_optional_child("messages")
            .ok_or_else(|| anyhow!("Missing <messages> inside <message_updates>"))?;

        Ok(parse_newsletter_messages(messages_node))
    }
}

// ── Accept ToS notice ──────────────────────────────────────────────────────

/// IQ spec for accepting a Terms of Service notice.
///
/// To accept the terms for creating newsletters, use
/// `notice_id = "20601218"` and `stage = "5"`.
///
/// Wire format:
/// ```xml
/// <iq xmlns="tos" type="set" to="s.whatsapp.net" id="...">
///   <notice id="20601218" stage="5"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct AcceptTosNoticeSpec {
    pub notice_id: String,
    pub stage: String,
}

impl IqSpec for AcceptTosNoticeSpec {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let child = NodeBuilder::new("notice")
            .attr("id", &self.notice_id)
            .attr("stage", &self.stage)
            .build();

        InfoQuery::set(
            "tos",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![child])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        // Success is indicated by a result-type IQ response; no payload.
        Ok(())
    }
}

// ── Newsletter message parsing ─────────────────────────────────────────────

/// Parse a `<messages>` node into a list of [`NewsletterMessage`] structs.
///
/// Each `<message>` child has attributes `server_id`, `id`, `type`, and `t`
/// (Unix timestamp). Optional children:
///
/// - `<plaintext>` — protobuf-encoded `wa::Message` body
/// - `<views_count count="N"/>` — view count
/// - `<reactions>` — child `<reaction code="..." count="N"/>` nodes
pub fn parse_newsletter_messages(messages_node: &Node) -> Vec<NewsletterMessage> {
    let Some(children) = messages_node.children() else {
        return Vec::new();
    };

    let mut output = Vec::with_capacity(children.len());

    for child in children {
        if child.tag != "message" {
            continue;
        }

        let attrs = child.attrs();

        let message_server_id = attrs
            .optional_u64("server_id")
            .map(|v| v as MessageServerId)
            .unwrap_or(0);

        let message_id = attrs
            .optional_string("id")
            .unwrap_or("")
            .to_string();

        let msg_type = attrs
            .optional_string("type")
            .unwrap_or("")
            .to_string();

        let timestamp = attrs
            .optional_u64("t")
            .and_then(|t| Utc.timestamp_opt(t as i64, 0).single())
            .unwrap_or_else(Utc::now);

        let mut views_count = 0i32;
        let mut reaction_counts: HashMap<String, i32> = HashMap::new();
        let mut message: Option<wa::Message> = None;

        if let Some(sub_children) = child.children() {
            for sub in sub_children {
                match sub.tag.as_str() {
                    "plaintext" => {
                        if let Some(NodeContent::Bytes(bytes)) = &sub.content {
                            match wa::Message::decode(bytes.as_slice()) {
                                Ok(msg) => message = Some(msg),
                                Err(e) => {
                                    log::warn!(
                                        "Failed to unmarshal newsletter message: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    "views_count" => {
                        views_count = sub
                            .attrs()
                            .optional_u64("count")
                            .map(|v| v as i32)
                            .unwrap_or(0);
                    }
                    "reactions" => {
                        if let Some(reaction_children) = sub.children() {
                            for reaction in reaction_children {
                                let rattrs = reaction.attrs();
                                let code = rattrs
                                    .optional_string("code")
                                    .unwrap_or("")
                                    .to_string();
                                let count = rattrs
                                    .optional_u64("count")
                                    .map(|v| v as i32)
                                    .unwrap_or(0);
                                if !code.is_empty() {
                                    reaction_counts.insert(code, count);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        output.push(NewsletterMessage {
            message_server_id,
            message_id,
            r#type: msg_type,
            timestamp,
            views_count,
            reaction_counts,
            message,
        });
    }

    output
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wacore_binary::builder::NodeBuilder;

    // ── convert_query_id ───────────────────────────────────────────────

    #[test]
    fn convert_query_id_mobile_returns_same() {
        assert_eq!(
            convert_query_id(QUERY_FETCH_NEWSLETTER, false),
            QUERY_FETCH_NEWSLETTER
        );
    }

    #[test]
    fn convert_query_id_desktop_translates() {
        assert_eq!(
            convert_query_id(QUERY_FETCH_NEWSLETTER, true),
            QUERY_FETCH_NEWSLETTER_DESKTOP
        );
        assert_eq!(
            convert_query_id(MUTATION_FOLLOW_NEWSLETTER, true),
            MUTATION_FOLLOW_NEWSLETTER_DESKTOP
        );
        assert_eq!(
            convert_query_id(MUTATION_MUTE_NEWSLETTER, true),
            MUTATION_MUTE_NEWSLETTER_DESKTOP
        );
    }

    #[test]
    fn convert_query_id_unknown_returns_unchanged() {
        assert_eq!(convert_query_id("9999999999", true), "9999999999");
    }

    #[test]
    fn convert_all_mobile_ids_have_desktop_mapping() {
        let mobile_ids = [
            QUERY_FETCH_NEWSLETTER,
            QUERY_RECOMMENDED_NEWSLETTERS,
            QUERY_SUBSCRIBED_NEWSLETTERS,
            QUERY_NEWSLETTER_SUBSCRIBERS,
            MUTATION_MUTE_NEWSLETTER,
            MUTATION_UNMUTE_NEWSLETTER,
            MUTATION_UPDATE_NEWSLETTER,
            MUTATION_CREATE_NEWSLETTER,
            MUTATION_UNFOLLOW_NEWSLETTER,
            MUTATION_FOLLOW_NEWSLETTER,
        ];
        for id in &mobile_ids {
            let desktop = convert_query_id(id, true);
            assert_ne!(desktop, *id, "Mobile ID {id} should have a desktop mapping");
        }
    }

    // ── build_newsletter_mark_viewed ───────────────────────────────────

    #[test]
    fn mark_viewed_builds_correct_node() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let node = build_newsletter_mark_viewed(&jid, &[100, 200, 300], "req-1");

        assert_eq!(node.tag, "receipt");
        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("view".to_string())
        );
        assert_eq!(
            node.attrs.get("to").map(|v| v.to_string_value()),
            Some("120363001@newsletter".to_string())
        );
        assert_eq!(
            node.attrs.get("id").map(|v| v.to_string_value()),
            Some("req-1".to_string())
        );

        let children = node.children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].tag, "list");

        let items = children[0].children().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].attrs.get("server_id").map(|v| v.to_string_value()),
            Some("100".to_string())
        );
    }

    #[test]
    fn mark_viewed_empty_server_ids() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let node = build_newsletter_mark_viewed(&jid, &[], "req-2");

        let children = node.children().unwrap();
        let list_children = children[0].children();
        assert!(list_children.is_none() || list_children.unwrap().is_empty());
    }

    // ── build_newsletter_reaction ──────────────────────────────────────

    #[test]
    fn reaction_add_builds_correct_node() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let node = build_newsletter_reaction(&jid, 42, "\u{1f44d}", "3EB0MSG001");

        assert_eq!(node.tag, "message");
        assert_eq!(
            node.attrs.get("type").map(|v| v.to_string_value()),
            Some("reaction".to_string())
        );
        assert_eq!(
            node.attrs.get("server_id").map(|v| v.to_string_value()),
            Some("42".to_string())
        );
        assert!(node.attrs.get("edit").is_none());

        let children = node.children().unwrap();
        assert_eq!(children[0].tag, "reaction");
        assert_eq!(
            children[0].attrs.get("code").map(|v| v.to_string_value()),
            Some("\u{1f44d}".to_string())
        );
    }

    #[test]
    fn reaction_remove_has_edit_attribute() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let node = build_newsletter_reaction(&jid, 42, "", "3EB0MSG001");

        assert_eq!(
            node.attrs.get("edit").map(|v| v.to_string_value()),
            Some("7".to_string())
        );

        let children = node.children().unwrap();
        assert_eq!(children[0].tag, "reaction");
        // No "code" attribute when removing
        assert!(children[0].attrs.get("code").is_none());
    }

    // ── NewsletterSubscribeLiveUpdatesSpec ─────────────────────────────

    #[test]
    fn subscribe_live_updates_builds_correct_iq() {
        let spec = NewsletterSubscribeLiveUpdatesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
        };
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "newsletter");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);
        assert_eq!(iq.to.to_string(), "120363001@newsletter");

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "live_updates");
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn subscribe_live_updates_parses_duration() {
        let spec = NewsletterSubscribeLiveUpdatesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
        };

        let response = NodeBuilder::new("iq")
            .children([
                NodeBuilder::new("live_updates")
                    .attr("duration", "300")
                    .build(),
            ])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.duration, Duration::from_secs(300));
    }

    #[test]
    fn subscribe_live_updates_missing_duration_defaults_zero() {
        let spec = NewsletterSubscribeLiveUpdatesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
        };

        let response = NodeBuilder::new("iq")
            .children([NodeBuilder::new("live_updates").build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.duration, Duration::from_secs(0));
    }

    // ── MEX spec builders ─────────────────────────────────────────────

    #[test]
    fn get_newsletter_info_spec_builds_correct_mex() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = get_newsletter_info_spec(&jid, false);

        assert_eq!(spec.doc_id, QUERY_FETCH_NEWSLETTER);
        assert_eq!(spec.variables["fetch_viewer_metadata"], true);
        assert_eq!(spec.variables["input"]["type"], "JID");
        assert_eq!(
            spec.variables["input"]["key"],
            "120363001@newsletter"
        );
    }

    #[test]
    fn get_newsletter_info_with_invite_strips_prefix() {
        let spec = get_newsletter_info_with_invite_spec(
            "https://whatsapp.com/channel/0029VaXYZ123",
            false,
        );

        assert_eq!(spec.variables["input"]["key"], "0029VaXYZ123");
        assert_eq!(spec.variables["input"]["type"], "INVITE");
        assert_eq!(spec.variables["fetch_viewer_metadata"], false);
    }

    #[test]
    fn get_newsletter_info_with_invite_bare_code() {
        let spec = get_newsletter_info_with_invite_spec("0029VaXYZ123", false);
        assert_eq!(spec.variables["input"]["key"], "0029VaXYZ123");
    }

    #[test]
    fn follow_newsletter_spec_uses_correct_mutation() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = follow_newsletter_spec(&jid, false);
        assert_eq!(spec.doc_id, MUTATION_FOLLOW_NEWSLETTER);
        assert_eq!(
            spec.variables["newsletter_id"],
            "120363001@newsletter"
        );
    }

    #[test]
    fn unfollow_newsletter_spec_uses_correct_mutation() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = unfollow_newsletter_spec(&jid, false);
        assert_eq!(spec.doc_id, MUTATION_UNFOLLOW_NEWSLETTER);
    }

    #[test]
    fn mute_spec_selects_correct_mutation() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();

        let mute_spec = newsletter_toggle_mute_spec(&jid, true, false);
        assert_eq!(mute_spec.doc_id, MUTATION_MUTE_NEWSLETTER);

        let unmute_spec = newsletter_toggle_mute_spec(&jid, false, false);
        assert_eq!(unmute_spec.doc_id, MUTATION_UNMUTE_NEWSLETTER);
    }

    #[test]
    fn create_newsletter_spec_includes_params() {
        let params = CreateNewsletterParams {
            name: "Test Channel".to_string(),
            description: Some("A test channel".to_string()),
            picture: None,
        };
        let spec = create_newsletter_spec(&params, false);

        assert_eq!(spec.doc_id, MUTATION_CREATE_NEWSLETTER);
        assert_eq!(
            spec.variables["newsletter_input"]["name"],
            "Test Channel"
        );
        assert_eq!(
            spec.variables["newsletter_input"]["description"],
            "A test channel"
        );
    }

    #[test]
    fn get_subscribers_spec_includes_limit() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = get_newsletter_subscribers_spec(&jid, 50, false);

        assert_eq!(spec.doc_id, QUERY_NEWSLETTER_SUBSCRIBERS);
        assert_eq!(spec.variables["input"]["count"], 50);
    }

    // ── GetNewsletterMessagesSpec ──────────────────────────────────────

    #[test]
    fn get_messages_spec_builds_correct_iq() {
        let spec = GetNewsletterMessagesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessagesParams {
                count: Some(25),
                before: Some(999),
            },
        };
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "newsletter");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Get);
        assert_eq!(iq.to.to_string(), "s.whatsapp.net");

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "messages");
            assert_eq!(
                nodes[0].attrs.get("jid").map(|v| v.to_string_value()),
                Some("120363001@newsletter".to_string())
            );
            assert_eq!(
                nodes[0].attrs.get("count").map(|v| v.to_string_value()),
                Some("25".to_string())
            );
            assert_eq!(
                nodes[0].attrs.get("before").map(|v| v.to_string_value()),
                Some("999".to_string())
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn get_messages_spec_omits_optional_attrs() {
        let spec = GetNewsletterMessagesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessagesParams::default(),
        };
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert!(nodes[0].attrs.get("count").is_none());
            assert!(nodes[0].attrs.get("before").is_none());
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    // ── GetNewsletterMessageUpdatesSpec ────────────────────────────────

    #[test]
    fn get_message_updates_spec_builds_correct_iq() {
        let since = Utc.timestamp_opt(1700000000, 0).unwrap();
        let spec = GetNewsletterMessageUpdatesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessageUpdatesParams {
                count: Some(10),
                since: Some(since),
                after: Some(500),
            },
        };
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "newsletter");
        // Message updates are sent TO the newsletter JID, not to server.
        assert_eq!(iq.to.to_string(), "120363001@newsletter");

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "message_updates");
            assert_eq!(
                nodes[0].attrs.get("count").map(|v| v.to_string_value()),
                Some("10".to_string())
            );
            assert_eq!(
                nodes[0].attrs.get("since").map(|v| v.to_string_value()),
                Some("1700000000".to_string())
            );
            assert_eq!(
                nodes[0].attrs.get("after").map(|v| v.to_string_value()),
                Some("500".to_string())
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    // ── AcceptTosNoticeSpec ───────────────────────────────────────────

    #[test]
    fn accept_tos_notice_builds_correct_iq() {
        let spec = AcceptTosNoticeSpec {
            notice_id: "20601218".to_string(),
            stage: "5".to_string(),
        };
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "tos");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "notice");
            assert_eq!(
                nodes[0].attrs.get("id").map(|v| v.to_string_value()),
                Some("20601218".to_string())
            );
            assert_eq!(
                nodes[0].attrs.get("stage").map(|v| v.to_string_value()),
                Some("5".to_string())
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    // ── parse_newsletter_messages ─────────────────────────────────────

    #[test]
    fn parse_messages_basic() {
        let messages_node = NodeBuilder::new("messages")
            .children([
                NodeBuilder::new("message")
                    .attr("server_id", "101")
                    .attr("id", "3EB0MSG001")
                    .attr("type", "text")
                    .attr("t", "1700000000")
                    .children([
                        NodeBuilder::new("views_count")
                            .attr("count", "42")
                            .build(),
                        NodeBuilder::new("reactions")
                            .children([
                                NodeBuilder::new("reaction")
                                    .attr("code", "\u{1f44d}")
                                    .attr("count", "10")
                                    .build(),
                                NodeBuilder::new("reaction")
                                    .attr("code", "\u{2764}")
                                    .attr("count", "5")
                                    .build(),
                            ])
                            .build(),
                    ])
                    .build(),
            ])
            .build();

        let result = parse_newsletter_messages(&messages_node);
        assert_eq!(result.len(), 1);

        let msg = &result[0];
        assert_eq!(msg.message_server_id, 101);
        assert_eq!(msg.message_id, "3EB0MSG001");
        assert_eq!(msg.r#type, "text");
        assert_eq!(msg.views_count, 42);
        assert_eq!(msg.reaction_counts.len(), 2);
        assert_eq!(msg.reaction_counts["\u{1f44d}"], 10);
        assert_eq!(msg.reaction_counts["\u{2764}"], 5);
        assert!(msg.message.is_none()); // No <plaintext> child
    }

    #[test]
    fn parse_messages_skips_non_message_tags() {
        let messages_node = NodeBuilder::new("messages")
            .children([
                NodeBuilder::new("not_a_message").build(),
                NodeBuilder::new("message")
                    .attr("server_id", "1")
                    .attr("id", "M1")
                    .attr("type", "text")
                    .attr("t", "1700000000")
                    .build(),
            ])
            .build();

        let result = parse_newsletter_messages(&messages_node);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message_id, "M1");
    }

    #[test]
    fn parse_messages_empty_node() {
        let messages_node = NodeBuilder::new("messages").build();
        let result = parse_newsletter_messages(&messages_node);
        assert!(result.is_empty());
    }

    // ── MEX response parsing ──────────────────────────────────────────

    #[test]
    fn parse_newsletter_info_from_mex_data() {
        let data = serde_json::json!({
            "xwa2_newsletter": {
                "id": "120363001@newsletter",
                "state": {"type": "active"},
                "thread_metadata": {
                    "creation_time": 1700000000,
                    "invite": "0029VaXYZ",
                    "name": {"text": "Test Channel", "id": "n1", "update_time": 1700000000000000i64},
                    "description": {"text": "A test", "id": "d1", "update_time": 1700000000000000i64},
                    "subscribers_count": 100,
                    "verification": "verified",
                    "picture_url": null,
                    "preview_url": null,
                    "settings": {
                        "reaction_codes": {"value": "all"}
                    }
                },
                "viewer_metadata": {
                    "mute": "off",
                    "role": "subscriber"
                }
            }
        });

        let resp = MexResponse {
            data: Some(data),
            errors: None,
        };

        let info = parse_newsletter_info(&resp).unwrap();
        assert!(info.is_some());
        let meta = info.unwrap();
        assert_eq!(meta.id.to_string(), "120363001@newsletter");
        assert_eq!(meta.thread_metadata.name.text, "Test Channel");
        assert_eq!(meta.thread_metadata.subscriber_count, 100);
        assert!(meta.viewer_metadata.is_some());
    }

    #[test]
    fn parse_subscribed_newsletters_from_mex_data() {
        let data = serde_json::json!({
            "xwa2_newsletter_subscribed": [
                {
                    "id": "120363001@newsletter",
                    "state": {"type": "active"},
                    "thread_metadata": {
                        "creation_time": 1700000000,
                        "invite": "abc",
                        "name": {"text": "Ch1", "id": "n1", "update_time": 1700000000000000i64},
                        "description": {"text": "", "id": "d1", "update_time": 1700000000000000i64},
                        "subscribers_count": 50,
                        "verification": "unverified",
                        "settings": {"reaction_codes": {"value": "basic"}}
                    },
                    "viewer_metadata": {"mute": "on", "role": "admin"}
                }
            ]
        });

        let resp = MexResponse {
            data: Some(data),
            errors: None,
        };

        let newsletters = parse_subscribed_newsletters(&resp).unwrap();
        assert_eq!(newsletters.len(), 1);
        assert_eq!(newsletters[0].thread_metadata.name.text, "Ch1");
    }

    #[test]
    fn parse_create_newsletter_response() {
        let data = serde_json::json!({
            "xwa2_newsletter_create": {
                "id": "120363999@newsletter",
                "state": {"type": "active"},
                "thread_metadata": {
                    "creation_time": 1700000000,
                    "invite": "xyz",
                    "name": {"text": "New Channel", "id": "n1", "update_time": 1700000000000000i64},
                    "description": {"text": "Desc", "id": "d1", "update_time": 1700000000000000i64},
                    "subscribers_count": 1,
                    "verification": "unverified",
                    "settings": {"reaction_codes": {"value": "all"}}
                },
                "viewer_metadata": {"mute": "off", "role": "owner"}
            }
        });

        let resp = MexResponse {
            data: Some(data),
            errors: None,
        };

        let meta = parse_create_newsletter(&resp).unwrap().unwrap();
        assert_eq!(meta.id.to_string(), "120363999@newsletter");
        assert_eq!(meta.thread_metadata.name.text, "New Channel");
    }

    #[test]
    fn parse_newsletter_subscribers_response() {
        let data = serde_json::json!({
            "xwa2_newsletter_subscribers": {
                "subscribers": {
                    "edges": [
                        {"id": "user1"},
                        {"id": "user2"},
                        {"id": "user3"}
                    ]
                }
            }
        });

        let resp = MexResponse {
            data: Some(data),
            errors: None,
        };

        let subs = parse_newsletter_subscribers(&resp).unwrap();
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].id, "user1");
    }

    #[test]
    fn parse_newsletter_subscribers_empty() {
        let data = serde_json::json!({
            "xwa2_newsletter_subscribers": null
        });

        let resp = MexResponse {
            data: Some(data),
            errors: None,
        };

        let subs = parse_newsletter_subscribers(&resp).unwrap();
        assert!(subs.is_empty());
    }

    #[test]
    fn mex_data_returns_error_on_missing() {
        let resp = MexResponse {
            data: None,
            errors: None,
        };
        assert!(mex_data(&resp).is_err());
    }

    // ── Desktop mode query ID selection ───────────────────────────────

    #[test]
    fn desktop_mode_creates_correct_spec() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = get_newsletter_info_spec(&jid, true);
        assert_eq!(spec.doc_id, QUERY_FETCH_NEWSLETTER_DESKTOP);
    }

    #[test]
    fn desktop_mode_follow_uses_desktop_id() {
        let jid = Jid::try_from("120363001@newsletter").unwrap();
        let spec = follow_newsletter_spec(&jid, true);
        assert_eq!(spec.doc_id, MUTATION_FOLLOW_NEWSLETTER_DESKTOP);
    }

    #[test]
    fn desktop_mode_subscribed_uses_desktop_id() {
        let spec = get_subscribed_newsletters_spec(true);
        assert_eq!(spec.doc_id, QUERY_SUBSCRIBED_NEWSLETTERS_DESKTOP);
    }

    // ── GetNewsletterMessagesSpec parse ────────────────────────────────

    #[test]
    fn get_messages_spec_parses_response() {
        let spec = GetNewsletterMessagesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessagesParams::default(),
        };

        let response = NodeBuilder::new("iq")
            .children([NodeBuilder::new("messages")
                .children([NodeBuilder::new("message")
                    .attr("server_id", "50")
                    .attr("id", "M50")
                    .attr("type", "text")
                    .attr("t", "1700000000")
                    .build()])
                .build()])
            .build();

        let msgs = spec.parse_response(&response).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_server_id, 50);
    }

    #[test]
    fn get_messages_spec_missing_messages_node_errors() {
        let spec = GetNewsletterMessagesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessagesParams::default(),
        };

        let response = NodeBuilder::new("iq").build();
        assert!(spec.parse_response(&response).is_err());
    }

    // ── GetNewsletterMessageUpdatesSpec parse ──────────────────────────

    #[test]
    fn get_message_updates_parses_nested_response() {
        let spec = GetNewsletterMessageUpdatesSpec {
            jid: Jid::try_from("120363001@newsletter").unwrap(),
            params: GetNewsletterMessageUpdatesParams::default(),
        };

        let response = NodeBuilder::new("iq")
            .children([NodeBuilder::new("message_updates")
                .children([NodeBuilder::new("messages")
                    .children([NodeBuilder::new("message")
                        .attr("server_id", "75")
                        .attr("id", "M75")
                        .attr("type", "text")
                        .attr("t", "1700000000")
                        .children([
                            NodeBuilder::new("views_count")
                                .attr("count", "99")
                                .build(),
                        ])
                        .build()])
                    .build()])
                .build()])
            .build();

        let msgs = spec.parse_response(&response).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].views_count, 99);
    }
}
