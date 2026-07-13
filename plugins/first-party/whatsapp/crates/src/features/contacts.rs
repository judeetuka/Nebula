//! Contact information feature.
//!
//! Profile picture types are defined in `wacore::iq::contacts`.
//! Usync types are defined in `wacore::iq::usync`.

use crate::client::Client;
use crate::request::IqError;
use anyhow::Result;
use log::{debug, error};
use std::collections::HashMap;
use wacore::iq::contacts::{
    GetContactQRLinkSpec, ProfilePictureSpec, ProfilePictureType, ResolveBusinessMessageLinkSpec,
    ResolveContactQRLinkSpec, SetStatusMessageSpec,
};
use wacore::iq::usync::{ContactInfoSpec, DeviceListSpec, IsOnWhatsAppSpec, UserInfoSpec};
use wacore::types::events::{BusinessNameUpdate, Event};
use wacore::types::user::{VerifiedName, parse_verified_name};
use wacore_binary::jid::{Jid, JidExt};
use wacore_binary::node::Node;

// Re-export types from wacore
pub use wacore::iq::contacts::{
    BusinessMessageLinkTarget, ContactQRLinkTarget, ProfilePicture,
};
pub use wacore::iq::usync::{ContactInfo, IsOnWhatsAppResult, UserInfo};

pub struct Contacts<'a> {
    client: &'a Client,
}

impl<'a> Contacts<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    pub async fn is_on_whatsapp(&self, phones: &[&str]) -> Result<Vec<IsOnWhatsAppResult>> {
        if phones.is_empty() {
            return Ok(Vec::new());
        }

        debug!("is_on_whatsapp: checking {} numbers", phones.len());

        let request_id = self.client.generate_request_id();
        let phone_strings: Vec<String> = phones.iter().map(|s| s.to_string()).collect();
        let spec = IsOnWhatsAppSpec::new(phone_strings, request_id);

        Ok(self.client.execute(spec).await?)
    }

    pub async fn get_info(&self, phones: &[&str]) -> Result<Vec<ContactInfo>> {
        if phones.is_empty() {
            return Ok(Vec::new());
        }

        debug!("get_info: fetching info for {} numbers", phones.len());

        let request_id = self.client.generate_request_id();
        let phone_strings: Vec<String> = phones.iter().map(|s| s.to_string()).collect();
        let spec = ContactInfoSpec::new(phone_strings, request_id);

        Ok(self.client.execute(spec).await?)
    }

    pub async fn get_profile_picture(
        &self,
        jid: &Jid,
        preview: bool,
    ) -> Result<Option<ProfilePicture>> {
        debug!(
            "get_profile_picture: fetching {} picture for {}",
            if preview { "preview" } else { "full" },
            jid
        );

        let picture_type = if preview {
            ProfilePictureType::Preview
        } else {
            ProfilePictureType::Full
        };
        let mut spec = ProfilePictureSpec::new(jid, picture_type);

        // Include tctoken for user JIDs (skip groups, newsletters)
        if !jid.is_group()
            && !jid.is_newsletter()
            && let Some(token) = self.client.lookup_tc_token_for_jid(jid).await
        {
            spec = spec.with_tc_token(token);
        }

        Ok(self.client.execute(spec).await?)
    }

    pub async fn get_user_info(&self, jids: &[Jid]) -> Result<HashMap<Jid, UserInfo>> {
        if jids.is_empty() {
            return Ok(HashMap::new());
        }

        debug!("get_user_info: fetching info for {} JIDs", jids.len());

        let request_id = self.client.generate_request_id();
        let spec = UserInfoSpec::new(jids.to_vec(), request_id);

        Ok(self.client.execute(spec).await?)
    }

    /// Sets the current user's "About" status text.
    ///
    /// This is different from ephemeral status broadcast messages.
    pub async fn set_status_message(&self, text: &str) -> Result<(), IqError> {
        debug!("set_status_message: setting to '{}'", text);
        let spec = SetStatusMessageSpec::new(text);
        self.client.execute(spec).await?;
        Ok(())
    }

    /// Gets the device list for given JIDs with JID resolution.
    ///
    /// This is an enhanced version of the basic device lookup that handles
    /// PN-to-LID resolution. For each input JID that is a phone number,
    /// if a known LID mapping exists, the LID is queried instead.
    /// Bot JIDs are returned directly (they have no devices to query).
    ///
    /// Returns a flat list of device JIDs (with device IDs populated).
    /// The local device is excluded from the output.
    pub async fn get_user_devices_context(&self, jids: &[Jid]) -> Result<Vec<Jid>> {
        if jids.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            "get_user_devices_context: looking up devices for {} JIDs",
            jids.len()
        );

        // Resolve PN JIDs to LIDs where known, skip bots
        let mut resolved_jids = Vec::with_capacity(jids.len());
        let mut bot_jids = Vec::new();

        for jid in jids {
            if jid.is_bot() {
                bot_jids.push(jid.clone());
                continue;
            }
            // Try to resolve PN to LID for better device lookup
            let jid_str = jid.to_non_ad().to_string();
            if let Some(alt) = self.client.get_alt_jid_for(&jid_str).await {
                if let Ok(alt_jid) = alt.parse::<Jid>() {
                    resolved_jids.push(alt_jid);
                    continue;
                }
            }
            resolved_jids.push(jid.to_non_ad());
        }

        let mut all_devices = bot_jids;

        if !resolved_jids.is_empty() {
            let request_id = self.client.generate_request_id();
            let spec = DeviceListSpec::new(resolved_jids, request_id);
            let response = self.client.execute(spec).await?;

            for device_list in response.device_lists {
                all_devices.extend(device_list.devices);
            }
        }

        Ok(all_devices)
    }

    /// Resolves a business message short link (`wa.me/message/CODE`).
    ///
    /// Returns the target JID, push name, optional pre-filled message text,
    /// and business verification details.
    ///
    /// The `code` parameter can be the full URL or just the code part.
    pub async fn resolve_business_message_link(
        &self,
        code: &str,
    ) -> Result<BusinessMessageLinkTarget, IqError> {
        debug!("resolve_business_message_link: resolving '{}'", code);
        let spec = ResolveBusinessMessageLinkSpec::new(code);
        Ok(self.client.execute(spec).await?)
    }

    /// Resolves a contact share QR code link (`wa.me/qr/CODE`).
    ///
    /// Returns the target JID, push name, and link type.
    ///
    /// The `code` parameter can be the full URL or just the code part.
    pub async fn resolve_contact_qr_link(
        &self,
        code: &str,
    ) -> Result<ContactQRLinkTarget, IqError> {
        debug!("resolve_contact_qr_link: resolving '{}'", code);
        let spec = ResolveContactQRLinkSpec::new(code);
        Ok(self.client.execute(spec).await?)
    }

    /// Gets the current user's own contact share QR link code.
    ///
    /// The returned code can be prefixed with `https://wa.me/qr/` to form a
    /// scannable link.
    ///
    /// If `revoke` is true, the server revokes the current link and generates
    /// a new one.
    pub async fn get_contact_qr_link(&self, revoke: bool) -> Result<String, IqError> {
        debug!(
            "get_contact_qr_link: {}",
            if revoke { "revoking" } else { "fetching" }
        );
        let spec = if revoke {
            GetContactQRLinkSpec::revoke()
        } else {
            GetContactQRLinkSpec::get()
        };
        Ok(self.client.execute(spec).await?)
    }

    /// Parse a verified business name from a `<business>` protocol node.
    ///
    /// This decodes the `VerifiedNameCertificate` protobuf from the
    /// `<verified_name>` child of a `<business>` node, as found in usync
    /// responses and message nodes.
    ///
    /// Returns `None` if the node is not a `<business>` tag, has no
    /// `<verified_name>` child, or contains no binary protobuf data.
    pub fn parse_verified_name(
        &self,
        business_node: &Node,
    ) -> Result<Option<VerifiedName>> {
        parse_verified_name(business_node)
    }
}

impl Client {
    pub fn contacts(&self) -> Contacts<'_> {
        Contacts::new(self)
    }

    /// Store a business name for a contact and emit an event if it changed.
    ///
    /// Mirrors whatsmeow's `updateBusinessName`: stores under both the primary
    /// JID and its alternate (LID/PN), and dispatches a `BusinessNameUpdate`
    /// event when the name changes.
    pub(crate) async fn update_business_name(
        &self,
        user: &Jid,
        user_alt: Option<&Jid>,
        name: &str,
    ) {
        let user_str = user.to_non_ad().to_string();
        let backend = self.persistence_manager.backend();

        match backend.put_business_name(&user_str, name).await {
            Ok((true, old_name)) => {
                // Also store under alternate JID if known
                let alt_jid = if let Some(alt) = user_alt {
                    Some(alt.to_non_ad())
                } else {
                    self.get_alt_jid_for(&user_str)
                        .await
                        .and_then(|s| s.parse::<Jid>().ok())
                };

                if let Some(ref alt) = alt_jid {
                    let alt_str = alt.to_string();
                    if let Err(e) = backend.put_business_name(&alt_str, name).await {
                        error!(
                            "Failed to save business name for alt JID {}: {:?}",
                            alt_str, e
                        );
                    }
                }

                debug!(
                    "Business name of {} changed from '{}' to '{}', dispatching event",
                    user_str, old_name, name
                );
                self.core.event_bus.dispatch(&Event::BusinessNameUpdate(
                    BusinessNameUpdate {
                        jid: user.to_non_ad(),
                        jid_alt: alt_jid,
                        old_business_name: old_name,
                        new_business_name: name.to_string(),
                    },
                ));
            }
            Ok((false, _)) => {
                // Name unchanged, nothing to do
            }
            Err(e) => {
                error!(
                    "Failed to save business name of {} in store: {:?}",
                    user_str, e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contact_info_struct() {
        let jid: Jid = "1234567890@s.whatsapp.net"
            .parse()
            .expect("test JID should be valid");
        let lid: Jid = "12345678@lid".parse().expect("test JID should be valid");

        let info = ContactInfo {
            jid: jid.clone(),
            lid: Some(lid.clone()),
            is_registered: true,
            is_business: false,
            status: Some("Hey there!".to_string()),
            picture_id: Some(123456789),
        };

        assert!(info.is_registered);
        assert!(!info.is_business);
        assert_eq!(info.status, Some("Hey there!".to_string()));
        assert_eq!(info.picture_id, Some(123456789));
        assert!(info.lid.is_some());
    }

    #[test]
    fn test_profile_picture_struct() {
        let pic = ProfilePicture {
            id: "123456789".to_string(),
            url: "https://example.com/pic.jpg".to_string(),
            direct_path: Some("/v/pic.jpg".to_string()),
        };

        assert_eq!(pic.id, "123456789");
        assert_eq!(pic.url, "https://example.com/pic.jpg");
        assert!(pic.direct_path.is_some());
    }

    #[test]
    fn test_is_on_whatsapp_result_struct() {
        let jid: Jid = "1234567890@s.whatsapp.net"
            .parse()
            .expect("test JID should be valid");
        let result = IsOnWhatsAppResult {
            jid,
            is_registered: true,
        };

        assert!(result.is_registered);
    }

    #[test]
    fn test_business_message_link_target_struct() {
        let jid: Jid = "1234567890@s.whatsapp.net"
            .parse()
            .expect("test JID should be valid");

        let target = BusinessMessageLinkTarget {
            jid: jid.clone(),
            push_name: "Acme Corp".to_string(),
            message: "Hello!".to_string(),
            is_signed: Some(true),
            verified_name: Some("Acme Corp Official".to_string()),
            verified_level: Some("high".to_string()),
        };

        assert_eq!(target.jid.user, "1234567890");
        assert_eq!(target.push_name, "Acme Corp");
        assert_eq!(target.message, "Hello!");
        assert_eq!(target.is_signed, Some(true));
        assert_eq!(
            target.verified_name,
            Some("Acme Corp Official".to_string())
        );
    }

    #[test]
    fn test_contact_qr_link_target_struct() {
        let jid: Jid = "5551234@s.whatsapp.net"
            .parse()
            .expect("test JID should be valid");

        let target = ContactQRLinkTarget {
            jid: jid.clone(),
            push_name: Some("John Doe".to_string()),
            link_type: "contact".to_string(),
        };

        assert_eq!(target.jid.user, "5551234");
        assert_eq!(target.push_name, Some("John Doe".to_string()));
        assert_eq!(target.link_type, "contact");
    }

    #[test]
    fn test_parse_verified_name_utility() {
        use prost::Message;
        use wacore_binary::builder::NodeBuilder;
        use wacore_binary::node::NodeContent;
        use waproto::whatsapp as wa;

        // Build a valid VerifiedNameCertificate protobuf
        let details = wa::verified_name_certificate::Details {
            serial: Some(12345),
            issuer: Some("WhatsApp".to_string()),
            verified_name: Some("Test Business".to_string()),
            ..Default::default()
        };
        let mut details_bytes = Vec::new();
        details.encode(&mut details_bytes).unwrap();

        let cert = wa::VerifiedNameCertificate {
            details: Some(details_bytes.clone()),
            signature: Some(vec![0xAA, 0xBB]),
            server_signature: None,
        };
        let mut cert_bytes = Vec::new();
        cert.encode(&mut cert_bytes).unwrap();

        let business_node = NodeBuilder::new("business")
            .children([NodeBuilder::new("verified_name")
                .bytes(cert_bytes)
                .build()])
            .build();

        let result = parse_verified_name(&business_node).unwrap();
        assert!(result.is_some());
        let vname = result.unwrap();
        assert_eq!(
            vname.details.verified_name,
            Some("Test Business".to_string())
        );
        assert_eq!(vname.details.serial, Some(12345));
    }

    #[test]
    fn test_parse_verified_name_non_business_node() {
        use wacore_binary::builder::NodeBuilder;

        let not_business = NodeBuilder::new("other").build();
        let result = parse_verified_name(&not_business).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_verified_name_no_child() {
        use wacore_binary::builder::NodeBuilder;

        let biz = NodeBuilder::new("business").build();
        let result = parse_verified_name(&biz).unwrap();
        assert!(result.is_none());
    }
}
