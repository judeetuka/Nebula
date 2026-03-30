//! WhatsApp Status posting feature.
//!
//! Sends status updates to a list of explicit recipients using the
//! broadcast send path (to `status@broadcast` with `<participants>`).
//!
//! WhatsApp Status requires the group encryption path: sender key
//! encryption with explicit `<participants>` stanza listing each
//! recipient's devices. The `to` field is `status@broadcast`, but the
//! stanza must contain per-device encrypted SKDMs just like a group
//! message. The normal DM path resolves zero devices for broadcast
//! JIDs and silently drops the message.

use crate::client::Client;
use crate::store::signal_adapter::SignalProtocolStoreAdapter;
use anyhow::{Result, anyhow};
use wacore::client::context::GroupInfo;
use wacore::types::message::AddressingMode;
use wacore_binary::jid::{Jid, JidExt as _};
use waproto::whatsapp as wa;

pub struct Status<'a> {
    client: &'a Client,
}

impl<'a> Status<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Send a status update to specific recipients.
    ///
    /// This uses the group encryption path to encrypt the message for
    /// each recipient's devices, then sends to `status@broadcast` with
    /// a `<participants>` stanza containing per-device SKDMs.
    ///
    /// Recipients must be phone-number JIDs (e.g. `1234567890@s.whatsapp.net`).
    /// SKDM distribution is always forced because status recipient lists
    /// are ephemeral and may change between posts.
    pub async fn send_status(
        &self,
        message: wa::Message,
        recipients: Vec<Jid>,
    ) -> Result<String> {
        let to: Jid = "status@broadcast"
            .parse()
            .map_err(|e| anyhow!("Invalid status JID: {}", e))?;

        let request_id = self.client.generate_message_id().await;

        // Build synthetic GroupInfo with the explicit recipients.
        // Status always uses PN addressing (phone numbers, not LIDs).
        let mut group_info = GroupInfo::new(recipients, AddressingMode::Pn);

        let device_snapshot = self.client.persistence_manager.get_device_snapshot().await;
        let own_jid = device_snapshot
            .pn
            .clone()
            .ok_or_else(|| anyhow!("Not logged in"))?;
        let own_lid = device_snapshot
            .lid
            .clone()
            .ok_or_else(|| anyhow!("LID not set, cannot send status"))?;
        let account_info = device_snapshot.account.clone();

        // Store for retry handling (matches group send path)
        self.client
            .add_recent_message(to.clone(), request_id.clone(), &message)
            .await;

        // Ensure own JID is in the participant list (same as group path
        // in send_message_impl lines 218-224)
        if !group_info
            .participants
            .iter()
            .any(|p| p.is_same_user_as(&own_jid))
        {
            group_info.participants.push(own_jid.to_non_ad());
        }

        let device_store_arc = self.client.persistence_manager.get_device_arc().await;
        let mut store_adapter = SignalProtocolStoreAdapter::new(device_store_arc);

        let mut stores = wacore::send::SignalStores {
            session_store: &mut store_adapter.session_store,
            identity_store: &mut store_adapter.identity_store,
            prekey_store: &mut store_adapter.pre_key_store,
            signed_prekey_store: &store_adapter.signed_pre_key_store,
            sender_key_store: &mut store_adapter.sender_key_store,
        };

        // Always force SKDM distribution: status recipient lists are
        // ephemeral and may change between posts, so there is no point
        // caching sender key state for status@broadcast.
        let stanza = wacore::send::prepare_group_stanza(
            &mut stores,
            self.client,
            &mut group_info,
            &own_jid,
            &own_lid,
            account_info.as_ref(),
            to,
            &message,
            request_id.clone(),
            true,   // force_skdm_distribution -- always for status
            None,   // distribute to all resolved devices
            None,   // no edit attribute
            vec![], // no extra stanza nodes
        )
        .await?;

        self.client
            .send_node(stanza)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(request_id)
    }

    /// Delete (revoke) a previously posted status for all recipients.
    ///
    /// Sends a `ProtocolMessage::Revoke` via the same group encryption path
    /// as `send_status`, with `edit="7"` (SenderRevoke).
    pub async fn delete_status(
        &self,
        message_id: String,
        recipients: Vec<Jid>,
    ) -> Result<()> {
        let to: Jid = "status@broadcast"
            .parse()
            .map_err(|e| anyhow!("Invalid status JID: {}", e))?;

        let request_id = self.client.generate_message_id().await;

        // Build the revoke ProtocolMessage
        let revoke_message = wa::Message {
            protocol_message: Some(Box::new(wa::message::ProtocolMessage {
                key: Some(wa::MessageKey {
                    remote_jid: Some(to.to_string()),
                    from_me: Some(true),
                    id: Some(message_id),
                    participant: None,
                }),
                r#type: Some(wa::message::protocol_message::Type::Revoke as i32),
                ..Default::default()
            })),
            ..Default::default()
        };

        let mut group_info = GroupInfo::new(recipients, AddressingMode::Pn);

        let device_snapshot = self.client.persistence_manager.get_device_snapshot().await;
        let own_jid = device_snapshot
            .pn
            .clone()
            .ok_or_else(|| anyhow!("Not logged in"))?;
        let own_lid = device_snapshot
            .lid
            .clone()
            .ok_or_else(|| anyhow!("LID not set, cannot delete status"))?;
        let account_info = device_snapshot.account.clone();

        if !group_info
            .participants
            .iter()
            .any(|p| p.is_same_user_as(&own_jid))
        {
            group_info.participants.push(own_jid.to_non_ad());
        }

        let device_store_arc = self.client.persistence_manager.get_device_arc().await;
        let mut store_adapter = SignalProtocolStoreAdapter::new(device_store_arc);

        let mut stores = wacore::send::SignalStores {
            session_store: &mut store_adapter.session_store,
            identity_store: &mut store_adapter.identity_store,
            prekey_store: &mut store_adapter.pre_key_store,
            signed_prekey_store: &store_adapter.signed_pre_key_store,
            sender_key_store: &mut store_adapter.sender_key_store,
        };

        let stanza = wacore::send::prepare_group_stanza(
            &mut stores,
            self.client,
            &mut group_info,
            &own_jid,
            &own_lid,
            account_info.as_ref(),
            to,
            &revoke_message,
            request_id,
            true,   // force_skdm_distribution
            None,   // distribute to all resolved devices
            Some(wacore::types::message::EditAttribute::SenderRevoke),
            vec![], // no extra stanza nodes
        )
        .await?;

        self.client
            .send_node(stanza)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(())
    }
}

impl Client {
    pub fn status(&self) -> Status<'_> {
        Status::new(self)
    }
}
