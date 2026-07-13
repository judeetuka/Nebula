//! Contact-related IQ specifications.
//!
//! ## Profile Picture Wire Format
//! ```xml
//! <!-- Request (with optional tctoken for privacy gating) -->
//! <iq xmlns="w:profile:picture" type="get" to="s.whatsapp.net" target="1234567890@s.whatsapp.net" id="...">
//!   <picture type="preview" query="url">
//!     <tctoken><!-- raw token bytes (optional) --></tctoken>
//!   </picture>
//! </iq>
//!
//! <!-- Response (success) -->
//! <iq from="s.whatsapp.net" id="..." type="result">
//!   <picture id="123456789" url="https://..." direct_path="/v/..."/>
//! </iq>
//!
//! <!-- Response (not found) -->
//! <iq from="s.whatsapp.net" id="..." type="result">
//!   <picture>
//!     <error code="404" text="item-not-found"/>
//!   </picture>
//! </iq>
//! ```

use crate::iq::spec::IqSpec;
use crate::iq::tctoken::build_tc_token_node;
use crate::request::InfoQuery;
use anyhow::anyhow;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};

/// Profile picture information.
#[derive(Debug, Clone)]
pub struct ProfilePicture {
    pub id: String,
    pub url: String,
    pub direct_path: Option<String>,
}

/// Profile picture type (preview thumbnail or full-size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfilePictureType {
    #[default]
    Preview,
    Full,
}

impl ProfilePictureType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Full => "image",
        }
    }
}

/// Fetches the profile picture URL for a given JID.
#[derive(Debug, Clone)]
pub struct ProfilePictureSpec {
    pub jid: Jid,
    pub picture_type: ProfilePictureType,
    /// Optional tctoken to include in the IQ for privacy gating.
    pub tc_token: Option<Vec<u8>>,
}

impl ProfilePictureSpec {
    pub fn preview(jid: &Jid) -> Self {
        Self {
            jid: jid.clone(),
            picture_type: ProfilePictureType::Preview,
            tc_token: None,
        }
    }

    pub fn full(jid: &Jid) -> Self {
        Self {
            jid: jid.clone(),
            picture_type: ProfilePictureType::Full,
            tc_token: None,
        }
    }

    pub fn new(jid: &Jid, picture_type: ProfilePictureType) -> Self {
        Self {
            jid: jid.clone(),
            picture_type,
            tc_token: None,
        }
    }

    /// Include a tctoken in the profile picture IQ for privacy gating.
    pub fn with_tc_token(mut self, token: Vec<u8>) -> Self {
        self.tc_token = Some(token);
        self
    }
}

impl IqSpec for ProfilePictureSpec {
    type Response = Option<ProfilePicture>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let mut picture_builder = NodeBuilder::new("picture")
            .attr("type", self.picture_type.as_str())
            .attr("query", "url");

        // tctoken is a child of <picture>, matching WhatsApp Web's mixin merge pattern
        if let Some(token) = &self.tc_token {
            picture_builder = picture_builder.children([build_tc_token_node(token)]);
        }

        InfoQuery::get(
            "w:profile:picture",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![picture_builder.build()])),
        )
        .with_target_ref(&self.jid)
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        let picture_node = match response.get_optional_child("picture") {
            Some(p) => p,
            None => return Ok(None),
        };

        // Check for error response
        if let Some(error_node) = picture_node.get_optional_child("error") {
            let code = error_node.attrs().optional_string("code").unwrap_or("0");
            if code == "404" || code == "401" {
                return Ok(None);
            }
            let text = error_node
                .attrs()
                .optional_string("text")
                .unwrap_or("unknown error");
            return Err(anyhow!("Profile picture error {}: {}", code, text));
        }

        let id = picture_node
            .attrs()
            .optional_string("id")
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Picture response missing 'id' attribute"))?;

        let url = picture_node
            .attrs()
            .optional_string("url")
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Picture response missing 'url' attribute"))?;

        let direct_path = picture_node
            .attrs()
            .optional_string("direct_path")
            .map(|s| s.to_string());

        Ok(Some(ProfilePicture {
            id,
            url,
            direct_path,
        }))
    }
}

// ---------------------------------------------------------------------------
// SetStatusMessageSpec — set own "About" text
// ---------------------------------------------------------------------------

/// Sets the user's own status message ("About" text).
///
/// ## Wire Format
/// ```xml
/// <iq xmlns="status" type="set" to="s.whatsapp.net" id="...">
///   <status>TEXT</status>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetStatusMessageSpec {
    pub text: String,
}

impl SetStatusMessageSpec {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl IqSpec for SetStatusMessageSpec {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let status_node = NodeBuilder::new("status")
            .string_content(self.text.clone())
            .build();

        InfoQuery::set(
            "status",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![status_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response, anyhow::Error> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// QR link constants
// ---------------------------------------------------------------------------

/// URL prefix for business message links (short form).
pub const BUSINESS_MESSAGE_LINK_PREFIX: &str = "https://wa.me/message/";
/// URL prefix for business message links (direct API form).
pub const BUSINESS_MESSAGE_LINK_DIRECT_PREFIX: &str = "https://api.whatsapp.com/message/";
/// URL prefix for contact QR links (short form).
pub const CONTACT_QR_LINK_PREFIX: &str = "https://wa.me/qr/";
/// URL prefix for contact QR links (direct API form).
pub const CONTACT_QR_LINK_DIRECT_PREFIX: &str = "https://api.whatsapp.com/qr/";

// ---------------------------------------------------------------------------
// ResolveBusinessMessageLinkSpec
// ---------------------------------------------------------------------------

/// Resolved target from a business message link (`wa.me/message/CODE`).
#[derive(Debug, Clone)]
pub struct BusinessMessageLinkTarget {
    pub jid: Jid,
    pub push_name: String,
    pub message: String,
    pub is_signed: Option<bool>,
    pub verified_name: Option<String>,
    pub verified_level: Option<String>,
}

/// Resolves a business message short link to the target JID, push name, and
/// optional pre-filled message text.
///
/// ## Wire Format
/// ```xml
/// <!-- Request -->
/// <iq xmlns="w:qr" type="get" to="s.whatsapp.net" id="...">
///   <qr code="CODE"/>
/// </iq>
///
/// <!-- Response -->
/// <iq from="s.whatsapp.net" id="..." type="result">
///   <qr jid="..." notify="...">
///     <message>pre-filled text</message>
///     <business is_signed="true" verified_name="Acme" verified_level="high"/>
///   </qr>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct ResolveBusinessMessageLinkSpec {
    pub code: String,
}

impl ResolveBusinessMessageLinkSpec {
    /// Create from a raw code or full URL (prefixes are stripped automatically).
    pub fn new(code: impl Into<String>) -> Self {
        let mut code = code.into();
        // Strip known URL prefixes, leaving just the code
        for prefix in &[
            BUSINESS_MESSAGE_LINK_PREFIX,
            BUSINESS_MESSAGE_LINK_DIRECT_PREFIX,
        ] {
            code = code.strip_prefix(prefix).unwrap_or(&code).to_string();
        }
        Self { code }
    }
}

impl IqSpec for ResolveBusinessMessageLinkSpec {
    type Response = BusinessMessageLinkTarget;

    fn build_iq(&self) -> InfoQuery<'static> {
        let qr_node = NodeBuilder::new("qr")
            .attr("code", self.code.as_str())
            .build();

        InfoQuery::get(
            "w:qr",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![qr_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        let qr_node = response
            .get_optional_child("qr")
            .ok_or_else(|| anyhow!("Response missing <qr> node"))?;

        let jid = qr_node
            .attrs()
            .optional_jid("jid")
            .ok_or_else(|| anyhow!("<qr> missing 'jid' attribute"))?;
        let push_name = qr_node
            .attrs()
            .optional_string("notify")
            .unwrap_or("")
            .to_string();

        let message = qr_node
            .get_optional_child("message")
            .and_then(|m| match &m.content {
                Some(NodeContent::String(s)) => Some(s.clone()),
                Some(NodeContent::Bytes(b)) => String::from_utf8(b.clone()).ok(),
                _ => None,
            })
            .unwrap_or_default();

        let (is_signed, verified_name, verified_level) =
            if let Some(biz) = qr_node.get_optional_child("business") {
                let is_signed = biz.attrs().optional_string("is_signed").map(|s| s == "true");
                let vname = biz
                    .attrs()
                    .optional_string("verified_name")
                    .map(|s| s.to_string());
                let vlevel = biz
                    .attrs()
                    .optional_string("verified_level")
                    .map(|s| s.to_string());
                (is_signed, vname, vlevel)
            } else {
                (None, None, None)
            };

        Ok(BusinessMessageLinkTarget {
            jid,
            push_name,
            message,
            is_signed,
            verified_name,
            verified_level,
        })
    }
}

// ---------------------------------------------------------------------------
// ResolveContactQRLinkSpec
// ---------------------------------------------------------------------------

/// Resolved target from a contact QR code link (`wa.me/qr/CODE`).
#[derive(Debug, Clone)]
pub struct ContactQRLinkTarget {
    pub jid: Jid,
    pub push_name: Option<String>,
    pub link_type: String,
}

/// Resolves a contact share QR code link.
///
/// ## Wire Format
/// ```xml
/// <!-- Request -->
/// <iq xmlns="w:qr" type="get" to="s.whatsapp.net" id="...">
///   <qr code="CODE"/>
/// </iq>
///
/// <!-- Response -->
/// <iq from="s.whatsapp.net" id="..." type="result">
///   <qr jid="..." notify="..." type="contact"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct ResolveContactQRLinkSpec {
    pub code: String,
}

impl ResolveContactQRLinkSpec {
    /// Create from a raw code or full URL (prefixes are stripped automatically).
    pub fn new(code: impl Into<String>) -> Self {
        let mut code = code.into();
        for prefix in &[CONTACT_QR_LINK_PREFIX, CONTACT_QR_LINK_DIRECT_PREFIX] {
            code = code.strip_prefix(prefix).unwrap_or(&code).to_string();
        }
        Self { code }
    }
}

impl IqSpec for ResolveContactQRLinkSpec {
    type Response = ContactQRLinkTarget;

    fn build_iq(&self) -> InfoQuery<'static> {
        let qr_node = NodeBuilder::new("qr")
            .attr("code", self.code.as_str())
            .build();

        InfoQuery::get(
            "w:qr",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![qr_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        let qr_node = response
            .get_optional_child("qr")
            .ok_or_else(|| anyhow!("Response missing <qr> node"))?;

        let jid = qr_node
            .attrs()
            .optional_jid("jid")
            .ok_or_else(|| anyhow!("<qr> missing 'jid' attribute"))?;
        let push_name = qr_node
            .attrs()
            .optional_string("notify")
            .map(|s| s.to_string());
        let link_type = qr_node
            .attrs()
            .optional_string("type")
            .unwrap_or("contact")
            .to_string();

        Ok(ContactQRLinkTarget {
            jid,
            push_name,
            link_type,
        })
    }
}

// ---------------------------------------------------------------------------
// GetContactQRLinkSpec
// ---------------------------------------------------------------------------

/// Gets or revokes your own contact QR code link.
///
/// ## Wire Format
/// ```xml
/// <!-- Request (get) -->
/// <iq xmlns="w:qr" type="set" to="s.whatsapp.net" id="...">
///   <qr type="contact" action="get"/>
/// </iq>
///
/// <!-- Request (revoke) -->
/// <iq xmlns="w:qr" type="set" to="s.whatsapp.net" id="...">
///   <qr type="contact" action="revoke"/>
/// </iq>
///
/// <!-- Response -->
/// <iq from="s.whatsapp.net" id="..." type="result">
///   <qr code="ABCDEF123456"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct GetContactQRLinkSpec {
    pub revoke: bool,
}

impl GetContactQRLinkSpec {
    pub fn get() -> Self {
        Self { revoke: false }
    }

    pub fn revoke() -> Self {
        Self { revoke: true }
    }
}

impl IqSpec for GetContactQRLinkSpec {
    type Response = String;

    fn build_iq(&self) -> InfoQuery<'static> {
        let action = if self.revoke { "revoke" } else { "get" };
        let qr_node = NodeBuilder::new("qr")
            .attr("type", "contact")
            .attr("action", action)
            .build();

        InfoQuery::set(
            "w:qr",
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![qr_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response, anyhow::Error> {
        let qr_node = response
            .get_optional_child("qr")
            .ok_or_else(|| anyhow!("Response missing <qr> node"))?;

        let code = qr_node
            .attrs()
            .optional_string("code")
            .ok_or_else(|| anyhow!("<qr> missing 'code' attribute"))?
            .to_string();

        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_picture_spec_preview() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid);

        assert_eq!(spec.picture_type, ProfilePictureType::Preview);

        let iq = spec.build_iq();
        assert_eq!(iq.namespace, "w:profile:picture");
        assert_eq!(iq.target, Some(jid));

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "picture");
            assert_eq!(
                nodes[0].attrs.get("type").and_then(|s| s.as_str()),
                Some("preview")
            );
        }
    }

    #[test]
    fn test_profile_picture_spec_full() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::full(&jid);

        assert_eq!(spec.picture_type, ProfilePictureType::Full);

        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(
                nodes[0].attrs.get("type").and_then(|s| s.as_str()),
                Some("image")
            );
        }
    }

    #[test]
    fn test_profile_picture_spec_parse_success() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid);

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("picture")
                .attr("id", "123456789")
                .attr("url", "https://example.com/pic.jpg")
                .attr("direct_path", "/v/pic.jpg")
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert!(result.is_some());

        let pic = result.unwrap();
        assert_eq!(pic.id, "123456789");
        assert_eq!(pic.url, "https://example.com/pic.jpg");
        assert_eq!(pic.direct_path, Some("/v/pic.jpg".to_string()));
    }

    #[test]
    fn test_profile_picture_spec_parse_not_found() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid);

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("picture")
                .children([NodeBuilder::new("error")
                    .attr("code", "404")
                    .attr("text", "item-not-found")
                    .build()])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_profile_picture_spec_parse_no_picture_node() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid);

        let response = NodeBuilder::new("iq").attr("type", "result").build();

        let result = spec.parse_response(&response).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_profile_picture_spec_with_tc_token() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid).with_tc_token(vec![0xCA, 0xFE, 0xBA, 0xBE]);

        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1, "IQ should have one child: picture");
            let picture = &nodes[0];
            assert_eq!(picture.tag, "picture");

            // tctoken is a child of picture (matching WhatsApp Web's mixin merge)
            let tctoken_children: Vec<_> = picture.get_children_by_tag("tctoken").collect();
            assert_eq!(tctoken_children.len(), 1);
            match &tctoken_children[0].content {
                Some(NodeContent::Bytes(data)) => {
                    assert_eq!(data, &[0xCA, 0xFE, 0xBA, 0xBE]);
                }
                _ => panic!("Expected binary content in tctoken node"),
            }
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_profile_picture_spec_without_tc_token() {
        let jid: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = ProfilePictureSpec::preview(&jid);

        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1, "IQ should have one child: picture");
            let picture = &nodes[0];
            assert_eq!(picture.tag, "picture");
            let tctoken_children: Vec<_> = picture.get_children_by_tag("tctoken").collect();
            assert_eq!(tctoken_children.len(), 0, "No tctoken without token");
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    // -----------------------------------------------------------------------
    // SetStatusMessageSpec tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_status_message_spec_build_iq() {
        let spec = SetStatusMessageSpec::new("Hello World");
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "status");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].tag, "status");
            match &nodes[0].content {
                Some(NodeContent::String(s)) => assert_eq!(s, "Hello World"),
                _ => panic!("Expected string content in <status> node"),
            }
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_set_status_message_spec_parse_response() {
        let spec = SetStatusMessageSpec::new("test");
        let response = NodeBuilder::new("iq").attr("type", "result").build();
        assert!(spec.parse_response(&response).is_ok());
    }

    // -----------------------------------------------------------------------
    // ResolveBusinessMessageLinkSpec tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_business_message_link_strips_prefix() {
        let spec1 = ResolveBusinessMessageLinkSpec::new("https://wa.me/message/ABC123");
        assert_eq!(spec1.code, "ABC123");

        let spec2 =
            ResolveBusinessMessageLinkSpec::new("https://api.whatsapp.com/message/XYZ789");
        assert_eq!(spec2.code, "XYZ789");

        let spec3 = ResolveBusinessMessageLinkSpec::new("RAWCODE");
        assert_eq!(spec3.code, "RAWCODE");
    }

    #[test]
    fn test_resolve_business_message_link_build_iq() {
        let spec = ResolveBusinessMessageLinkSpec::new("ABC123");
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "w:qr");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Get);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "qr");
            assert_eq!(
                nodes[0].attrs.get("code").and_then(|v| v.as_str()),
                Some("ABC123")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_resolve_business_message_link_parse_response() {
        let spec = ResolveBusinessMessageLinkSpec::new("ABC123");

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr")
                .attr("jid", "1234567890@s.whatsapp.net")
                .attr("notify", "Acme Corp")
                .children([
                    NodeBuilder::new("message")
                        .string_content("Hi, I want to order")
                        .build(),
                    NodeBuilder::new("business")
                        .attr("is_signed", "true")
                        .attr("verified_name", "Acme Corp Official")
                        .attr("verified_level", "high")
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.jid.user, "1234567890");
        assert_eq!(result.push_name, "Acme Corp");
        assert_eq!(result.message, "Hi, I want to order");
        assert_eq!(result.is_signed, Some(true));
        assert_eq!(
            result.verified_name,
            Some("Acme Corp Official".to_string())
        );
        assert_eq!(result.verified_level, Some("high".to_string()));
    }

    #[test]
    fn test_resolve_business_message_link_parse_minimal_response() {
        let spec = ResolveBusinessMessageLinkSpec::new("CODE");

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr")
                .attr("jid", "999@s.whatsapp.net")
                .attr("notify", "Shop")
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.jid.user, "999");
        assert_eq!(result.push_name, "Shop");
        assert!(result.message.is_empty());
        assert!(result.is_signed.is_none());
        assert!(result.verified_name.is_none());
    }

    // -----------------------------------------------------------------------
    // ResolveContactQRLinkSpec tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_contact_qr_link_strips_prefix() {
        let spec1 = ResolveContactQRLinkSpec::new("https://wa.me/qr/CODE");
        assert_eq!(spec1.code, "CODE");

        let spec2 = ResolveContactQRLinkSpec::new("https://api.whatsapp.com/qr/CODE2");
        assert_eq!(spec2.code, "CODE2");

        let spec3 = ResolveContactQRLinkSpec::new("RAWCODE");
        assert_eq!(spec3.code, "RAWCODE");
    }

    #[test]
    fn test_resolve_contact_qr_link_build_iq() {
        let spec = ResolveContactQRLinkSpec::new("MYCODE");
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "w:qr");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Get);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "qr");
            assert_eq!(
                nodes[0].attrs.get("code").and_then(|v| v.as_str()),
                Some("MYCODE")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_resolve_contact_qr_link_parse_response() {
        let spec = ResolveContactQRLinkSpec::new("MYCODE");

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr")
                .attr("jid", "5551234@s.whatsapp.net")
                .attr("notify", "John Doe")
                .attr("type", "contact")
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.jid.user, "5551234");
        assert_eq!(result.push_name, Some("John Doe".to_string()));
        assert_eq!(result.link_type, "contact");
    }

    #[test]
    fn test_resolve_contact_qr_link_parse_no_notify() {
        let spec = ResolveContactQRLinkSpec::new("CODE");

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr")
                .attr("jid", "555@s.whatsapp.net")
                .attr("type", "contact")
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert!(result.push_name.is_none());
    }

    // -----------------------------------------------------------------------
    // GetContactQRLinkSpec tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_contact_qr_link_spec_get() {
        let spec = GetContactQRLinkSpec::get();
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, "w:qr");
        assert_eq!(iq.query_type, crate::request::InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "qr");
            assert_eq!(
                nodes[0].attrs.get("type").and_then(|v| v.as_str()),
                Some("contact")
            );
            assert_eq!(
                nodes[0].attrs.get("action").and_then(|v| v.as_str()),
                Some("get")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_get_contact_qr_link_spec_revoke() {
        let spec = GetContactQRLinkSpec::revoke();
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(
                nodes[0].attrs.get("action").and_then(|v| v.as_str()),
                Some("revoke")
            );
        } else {
            panic!("Expected NodeContent::Nodes");
        }
    }

    #[test]
    fn test_get_contact_qr_link_spec_parse_response() {
        let spec = GetContactQRLinkSpec::get();

        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr")
                .attr("code", "ABCDEF123456")
                .build()])
            .build();

        let code = spec.parse_response(&response).unwrap();
        assert_eq!(code, "ABCDEF123456");
    }

    #[test]
    fn test_get_contact_qr_link_spec_parse_missing_code() {
        let spec = GetContactQRLinkSpec::get();
        let response = NodeBuilder::new("iq")
            .attr("type", "result")
            .children([NodeBuilder::new("qr").build()])
            .build();

        assert!(spec.parse_response(&response).is_err());
    }
}
