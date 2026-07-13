//! Connection event parsing for WhatsApp stream lifecycle.
//!
//! Ports whatsmeow/connectionevents.go -- pure parsing functions that extract
//! structured event data from incoming XML stanzas. The client layer calls
//! these parsers and dispatches the resulting events.
//!
//! Four stanza types are handled:
//!
//! - `<stream:error>` -- server-initiated stream teardown with a reason code
//! - `<ib>` -- information broadcast with offline sync and downgrade info
//! - `<failure>` -- connect-phase failure with a reason code
//! - `<success>` -- connect-phase success with LID and server timestamp

use chrono::{Duration, Utc};
use wacore_binary::attrs::AttrParser;
use wacore_binary::jid::Jid;
use wacore_binary::node::Node;

use crate::types::events::{
    ClientOutdated, ConnectFailure, ConnectFailureReason, Event, LoggedOut, OfflineSyncCompleted,
    OfflineSyncPreview, QrScannedWithoutMultidevice, StreamError, StreamReplaced, TempBanReason,
    TemporaryBan,
};

// ── Stream error parsing ───────────────────────────────────────────────────

/// The action the client layer should take after parsing a `<stream:error>`.
#[derive(Debug, Clone)]
pub enum StreamErrorAction {
    /// Server requested reconnection (code 515).
    Reconnect,
    /// Device was removed from the account. Session should be deleted.
    LoggedOut { reason: ConnectFailureReason },
    /// Another device replaced this connection.
    StreamReplaced,
    /// Server is restarting (code 503). Auto-reconnect will handle it.
    ServerRestart,
    /// CAT token needs refreshing before reconnecting (code 413 or 414).
    CatRefreshNeeded { code: String },
    /// Unrecognized stream error. The raw node is preserved for logging.
    Unknown { code: String, raw: Node },
}

/// Parse a `<stream:error>` node into a [`StreamErrorAction`].
///
/// Wire format:
/// ```xml
/// <stream:error code="515"/>
/// <stream:error code="401"><conflict type="device_removed"/></stream:error>
/// <stream:error><conflict type="replaced"/></stream:error>
/// ```
pub fn parse_stream_error(node: &Node) -> StreamErrorAction {
    let mut parser = AttrParser::new(node);
    let code = parser.optional_string("code").unwrap_or("").to_string();

    let conflict_type = node.get_optional_child("conflict").and_then(|conflict| {
        let mut cp = AttrParser::new(conflict);
        cp.optional_string("type").map(str::to_string)
    });

    match (code.as_str(), conflict_type.as_deref()) {
        ("515", _) => StreamErrorAction::Reconnect,

        ("401", Some("device_removed")) => StreamErrorAction::LoggedOut {
            reason: ConnectFailureReason::LoggedOut,
        },

        (_, Some("replaced")) => StreamErrorAction::StreamReplaced,

        ("503", _) => StreamErrorAction::ServerRestart,

        ("413" | "414", _) => StreamErrorAction::CatRefreshNeeded { code },

        _ => StreamErrorAction::Unknown {
            code,
            raw: node.clone(),
        },
    }
}

// ── IB node parsing ────────────────────────────────────────────────────────

/// Dirty state notification from the server.
///
/// Currently logged but not acted upon, matching whatsmeow behavior.
#[derive(Debug, Clone)]
pub struct DirtyState {
    pub timestamp: i64,
    pub dirty_type: String,
}

/// Parse an `<ib>` node's children into a list of events.
///
/// Wire format:
/// ```xml
/// <ib>
///   <downgrade_webclient/>
///   <offline_preview count="42" appdata="5" message="30" notification="5" receipt="2"/>
///   <offline count="42"/>
///   <dirty timestamp="1713100000" type="account_sync"/>
/// </ib>
/// ```
///
/// Returns a `Vec<Event>` because a single `<ib>` node can contain
/// multiple event-producing children.
pub fn parse_ib_node(node: &Node) -> Vec<Event> {
    let children = match node.children() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut events = Vec::new();

    for child in children {
        match child.tag.as_str() {
            "downgrade_webclient" => {
                events.push(Event::QrScannedWithoutMultidevice(
                    QrScannedWithoutMultidevice,
                ));
            }

            "offline_preview" => {
                let mut parser = AttrParser::new(child);
                let total = parser.optional_unix_time("count").unwrap_or(0) as i32;
                let app_data_changes = parser.optional_unix_time("appdata").unwrap_or(0) as i32;
                let messages = parser.optional_unix_time("message").unwrap_or(0) as i32;
                let notifications = parser.optional_unix_time("notification").unwrap_or(0) as i32;
                let receipts = parser.optional_unix_time("receipt").unwrap_or(0) as i32;

                events.push(Event::OfflineSyncPreview(OfflineSyncPreview {
                    total,
                    app_data_changes,
                    messages,
                    notifications,
                    receipts,
                }));
            }

            "offline" => {
                let mut parser = AttrParser::new(child);
                let count = parser.optional_unix_time("count").unwrap_or(0) as i32;

                events.push(Event::OfflineSyncCompleted(OfflineSyncCompleted { count }));
            }

            "dirty" => {
                // Matched but intentionally not acted upon -- mirrors whatsmeow.
                // The dirty state is parsed for completeness but produces no event.
                let mut parser = AttrParser::new(child);
                let _state = DirtyState {
                    timestamp: parser.optional_unix_time("timestamp").unwrap_or(0),
                    dirty_type: parser.optional_string("type").unwrap_or("").to_string(),
                };
                log::debug!(
                    "Received dirty notification: type={}, timestamp={}",
                    _state.dirty_type,
                    _state.timestamp
                );
            }

            tag => {
                log::debug!("Ignoring unknown <ib> child: <{}>", tag);
            }
        }
    }

    events
}

// ── Connect failure parsing ────────────────────────────────────────────────

/// The action the client layer should take after parsing a `<failure>` node.
#[derive(Debug, Clone)]
pub enum ConnectFailureAction {
    /// User was logged out. Session should be deleted.
    LoggedOut { reason: ConnectFailureReason },
    /// Temporary ban with a reason code and expiration.
    TemporaryBan {
        code: TempBanReason,
        expire: Duration,
    },
    /// Client version is outdated and must be updated.
    ClientOutdated,
    /// CAT token needs refreshing. Auto-reconnect should handle it.
    CatRefreshNeeded {
        reason: ConnectFailureReason,
        message: String,
    },
    /// Server error that should auto-reconnect (503, 500).
    AutoReconnect {
        reason: ConnectFailureReason,
        message: String,
    },
    /// Unknown failure. The raw node is preserved for inspection.
    Unknown {
        reason: ConnectFailureReason,
        message: String,
        raw: Node,
    },
}

/// Parse a `<failure>` node into a [`ConnectFailureAction`].
///
/// Wire format:
/// ```xml
/// <failure reason="401"/>
/// <failure reason="402" code="101" expire="86400"/>
/// <failure reason="405" message="client outdated"/>
/// ```
pub fn parse_connect_failure(node: &Node) -> ConnectFailureAction {
    let mut parser = AttrParser::new(node);

    let reason_code = parser.optional_unix_time("reason").unwrap_or(0) as i32;
    let reason = ConnectFailureReason::from(reason_code);
    let message = parser.optional_string("message").unwrap_or("").to_string();

    if reason.is_logged_out() {
        return ConnectFailureAction::LoggedOut { reason };
    }

    if reason == ConnectFailureReason::TempBanned {
        let ban_code = parser.optional_unix_time("code").unwrap_or(0) as i32;
        let expire_secs = parser.optional_unix_time("expire").unwrap_or(0);
        return ConnectFailureAction::TemporaryBan {
            code: TempBanReason::from(ban_code),
            expire: Duration::seconds(expire_secs),
        };
    }

    if reason == ConnectFailureReason::ClientOutdated {
        return ConnectFailureAction::ClientOutdated;
    }

    if reason == ConnectFailureReason::CatInvalid || reason == ConnectFailureReason::CatExpired {
        return ConnectFailureAction::CatRefreshNeeded { reason, message };
    }

    if reason.should_reconnect() {
        return ConnectFailureAction::AutoReconnect { reason, message };
    }

    ConnectFailureAction::Unknown {
        reason,
        message,
        raw: node.clone(),
    }
}

/// Convert a [`ConnectFailureAction`] into the appropriate [`Event`].
///
/// This is a convenience for the client layer to dispatch the parsed action
/// directly without matching on every variant.
pub fn connect_failure_to_event(action: &ConnectFailureAction) -> Event {
    match action {
        ConnectFailureAction::LoggedOut { reason } => Event::LoggedOut(LoggedOut {
            on_connect: true,
            reason: *reason,
        }),
        ConnectFailureAction::TemporaryBan { code, expire } => Event::TemporaryBan(TemporaryBan {
            code: code.clone(),
            expire: *expire,
        }),
        ConnectFailureAction::ClientOutdated => Event::ClientOutdated(ClientOutdated),
        ConnectFailureAction::CatRefreshNeeded { reason, message } => {
            Event::ConnectFailure(ConnectFailure {
                reason: *reason,
                message: message.clone(),
                raw: None,
            })
        }
        ConnectFailureAction::AutoReconnect { reason, message } => {
            Event::ConnectFailure(ConnectFailure {
                reason: *reason,
                message: message.clone(),
                raw: None,
            })
        }
        ConnectFailureAction::Unknown {
            reason,
            message,
            raw,
        } => Event::ConnectFailure(ConnectFailure {
            reason: *reason,
            message: message.clone(),
            raw: Some(raw.clone()),
        }),
    }
}

/// Convert a [`StreamErrorAction`] into the appropriate [`Event`].
///
/// Not all stream error actions map to dispatchable events (e.g. `Reconnect`
/// and `ServerRestart` are handled by the client reconnect logic), so this
/// returns `Option<Event>`.
pub fn stream_error_to_event(action: &StreamErrorAction) -> Option<Event> {
    match action {
        StreamErrorAction::LoggedOut { reason } => Some(Event::LoggedOut(LoggedOut {
            on_connect: false,
            reason: *reason,
        })),
        StreamErrorAction::StreamReplaced => Some(Event::StreamReplaced(StreamReplaced)),
        StreamErrorAction::Unknown { code, raw } => Some(Event::StreamError(StreamError {
            code: code.clone(),
            raw: Some(raw.clone()),
        })),
        // Reconnect, ServerRestart, and CatRefreshNeeded are handled by
        // the client layer's reconnection logic, not via events.
        StreamErrorAction::Reconnect
        | StreamErrorAction::ServerRestart
        | StreamErrorAction::CatRefreshNeeded { .. } => None,
    }
}

// ── Connect success parsing ────────────────────────────────────────────────

/// Information extracted from a successful `<success>` handshake node.
#[derive(Debug, Clone)]
pub struct ConnectSuccessInfo {
    /// The LID (Linked Identity) JID assigned to this device.
    pub lid: Option<Jid>,
    /// Server-reported timestamp (Unix seconds).
    pub server_timestamp: i64,
    /// Offset between the server clock and the local clock, in milliseconds.
    /// Computed as `(server_time - local_time)` so adding this to `now()` gives
    /// an approximation of server time.
    pub server_time_offset_ms: i64,
}

/// Parse a `<success>` node into [`ConnectSuccessInfo`].
///
/// Wire format:
/// ```xml
/// <success lid="LID_JID" t="1713100000"/>
/// ```
pub fn parse_connect_success(node: &Node) -> ConnectSuccessInfo {
    let mut parser = AttrParser::new(node);

    let lid = parser.optional_jid("lid");
    let server_ts = parser.optional_unix_time("t").unwrap_or(0);

    // Compute offset: server timestamp (seconds) converted to ms, minus local time in ms.
    // Round local time to the nearest second to match whatsmeow behavior.
    let local_now = Utc::now();
    let local_ts_rounded = local_now.timestamp();
    let server_time_offset_ms = (server_ts - local_ts_rounded) * 1000;

    ConnectSuccessInfo {
        lid,
        server_timestamp: server_ts,
        server_time_offset_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wacore_binary::builder::NodeBuilder;

    // ── Stream error tests ─────────────────────────────────────────────

    #[test]
    fn test_parse_stream_error_515_reconnect() {
        let node = NodeBuilder::new("stream:error").attr("code", "515").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::Reconnect));
    }

    #[test]
    fn test_parse_stream_error_401_device_removed() {
        let conflict = NodeBuilder::new("conflict")
            .attr("type", "device_removed")
            .build();
        let node = NodeBuilder::new("stream:error")
            .attr("code", "401")
            .children([conflict])
            .build();

        let action = parse_stream_error(&node);
        assert!(matches!(
            action,
            StreamErrorAction::LoggedOut {
                reason: ConnectFailureReason::LoggedOut
            }
        ));
    }

    #[test]
    fn test_parse_stream_error_replaced() {
        let conflict = NodeBuilder::new("conflict")
            .attr("type", "replaced")
            .build();
        let node = NodeBuilder::new("stream:error")
            .children([conflict])
            .build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::StreamReplaced));
    }

    #[test]
    fn test_parse_stream_error_503_server_restart() {
        let node = NodeBuilder::new("stream:error").attr("code", "503").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::ServerRestart));
    }

    #[test]
    fn test_parse_stream_error_413_cat_expired() {
        let node = NodeBuilder::new("stream:error").attr("code", "413").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::CatRefreshNeeded { .. }));
        if let StreamErrorAction::CatRefreshNeeded { code } = action {
            assert_eq!(code, "413");
        }
    }

    #[test]
    fn test_parse_stream_error_414_cat_invalid() {
        let node = NodeBuilder::new("stream:error").attr("code", "414").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::CatRefreshNeeded { .. }));
        if let StreamErrorAction::CatRefreshNeeded { code } = action {
            assert_eq!(code, "414");
        }
    }

    #[test]
    fn test_parse_stream_error_unknown() {
        let node = NodeBuilder::new("stream:error").attr("code", "999").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::Unknown { .. }));
        if let StreamErrorAction::Unknown { code, .. } = action {
            assert_eq!(code, "999");
        }
    }

    #[test]
    fn test_parse_stream_error_no_code() {
        let node = NodeBuilder::new("stream:error").build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::Unknown { .. }));
        if let StreamErrorAction::Unknown { code, .. } = action {
            assert_eq!(code, "");
        }
    }

    #[test]
    fn test_parse_stream_error_replaced_takes_priority_over_unknown_code() {
        // Even with an unrecognized code, "replaced" conflict type wins.
        let conflict = NodeBuilder::new("conflict")
            .attr("type", "replaced")
            .build();
        let node = NodeBuilder::new("stream:error")
            .attr("code", "999")
            .children([conflict])
            .build();

        let action = parse_stream_error(&node);
        assert!(matches!(action, StreamErrorAction::StreamReplaced));
    }

    // ── IB node tests ──────────────────────────────────────────────────

    #[test]
    fn test_parse_ib_downgrade_webclient() {
        let node = NodeBuilder::new("ib")
            .children([NodeBuilder::new("downgrade_webclient").build()])
            .build();

        let events = parse_ib_node(&node);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::QrScannedWithoutMultidevice(_)));
    }

    #[test]
    fn test_parse_ib_offline_preview() {
        let preview = NodeBuilder::new("offline_preview")
            .attr("count", "42")
            .attr("appdata", "5")
            .attr("message", "30")
            .attr("notification", "5")
            .attr("receipt", "2")
            .build();

        let node = NodeBuilder::new("ib").children([preview]).build();

        let events = parse_ib_node(&node);
        assert_eq!(events.len(), 1);

        if let Event::OfflineSyncPreview(ref preview) = events[0] {
            assert_eq!(preview.total, 42);
            assert_eq!(preview.app_data_changes, 5);
            assert_eq!(preview.messages, 30);
            assert_eq!(preview.notifications, 5);
            assert_eq!(preview.receipts, 2);
        } else {
            panic!("Expected OfflineSyncPreview event");
        }
    }

    #[test]
    fn test_parse_ib_offline_completed() {
        let offline = NodeBuilder::new("offline").attr("count", "42").build();

        let node = NodeBuilder::new("ib").children([offline]).build();

        let events = parse_ib_node(&node);
        assert_eq!(events.len(), 1);

        if let Event::OfflineSyncCompleted(ref completed) = events[0] {
            assert_eq!(completed.count, 42);
        } else {
            panic!("Expected OfflineSyncCompleted event");
        }
    }

    #[test]
    fn test_parse_ib_dirty_produces_no_event() {
        let dirty = NodeBuilder::new("dirty")
            .attr("timestamp", "1713100000")
            .attr("type", "account_sync")
            .build();

        let node = NodeBuilder::new("ib").children([dirty]).build();

        let events = parse_ib_node(&node);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_ib_multiple_children() {
        let node = NodeBuilder::new("ib")
            .children([
                NodeBuilder::new("downgrade_webclient").build(),
                NodeBuilder::new("offline_preview")
                    .attr("count", "10")
                    .attr("appdata", "1")
                    .attr("message", "5")
                    .attr("notification", "3")
                    .attr("receipt", "1")
                    .build(),
                NodeBuilder::new("offline").attr("count", "10").build(),
            ])
            .build();

        let events = parse_ib_node(&node);
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], Event::QrScannedWithoutMultidevice(_)));
        assert!(matches!(events[1], Event::OfflineSyncPreview(_)));
        assert!(matches!(events[2], Event::OfflineSyncCompleted(_)));
    }

    #[test]
    fn test_parse_ib_empty() {
        let node = NodeBuilder::new("ib").build();
        let events = parse_ib_node(&node);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_ib_unknown_child_ignored() {
        let node = NodeBuilder::new("ib")
            .children([NodeBuilder::new("some_future_tag").build()])
            .build();

        let events = parse_ib_node(&node);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_ib_offline_preview_missing_attrs() {
        // Missing attributes should default to 0.
        let preview = NodeBuilder::new("offline_preview")
            .attr("count", "10")
            .build();

        let node = NodeBuilder::new("ib").children([preview]).build();

        let events = parse_ib_node(&node);
        assert_eq!(events.len(), 1);

        if let Event::OfflineSyncPreview(ref p) = events[0] {
            assert_eq!(p.total, 10);
            assert_eq!(p.app_data_changes, 0);
            assert_eq!(p.messages, 0);
            assert_eq!(p.notifications, 0);
            assert_eq!(p.receipts, 0);
        } else {
            panic!("Expected OfflineSyncPreview event");
        }
    }

    // ── Connect failure tests ──────────────────────────────────────────

    #[test]
    fn test_parse_connect_failure_logged_out_401() {
        let node = NodeBuilder::new("failure").attr("reason", "401").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::LoggedOut {
                reason: ConnectFailureReason::LoggedOut,
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_logged_out_403() {
        let node = NodeBuilder::new("failure").attr("reason", "403").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::LoggedOut {
                reason: ConnectFailureReason::MainDeviceGone,
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_logged_out_406() {
        let node = NodeBuilder::new("failure").attr("reason", "406").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::LoggedOut {
                reason: ConnectFailureReason::UnknownLogout,
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_temp_banned() {
        let node = NodeBuilder::new("failure")
            .attr("reason", "402")
            .attr("code", "101")
            .attr("expire", "86400")
            .build();

        let action = parse_connect_failure(&node);
        if let ConnectFailureAction::TemporaryBan { code, expire } = action {
            assert_eq!(code, TempBanReason::SentToTooManyPeople);
            assert_eq!(expire, Duration::seconds(86400));
        } else {
            panic!("Expected TemporaryBan, got {:?}", action);
        }
    }

    #[test]
    fn test_parse_connect_failure_client_outdated() {
        let node = NodeBuilder::new("failure").attr("reason", "405").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(action, ConnectFailureAction::ClientOutdated));
    }

    #[test]
    fn test_parse_connect_failure_cat_expired() {
        let node = NodeBuilder::new("failure")
            .attr("reason", "413")
            .attr("message", "cat expired")
            .build();

        let action = parse_connect_failure(&node);
        if let ConnectFailureAction::CatRefreshNeeded { reason, message } = action {
            assert_eq!(reason, ConnectFailureReason::CatExpired);
            assert_eq!(message, "cat expired");
        } else {
            panic!("Expected CatRefreshNeeded, got {:?}", action);
        }
    }

    #[test]
    fn test_parse_connect_failure_cat_invalid() {
        let node = NodeBuilder::new("failure").attr("reason", "414").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::CatRefreshNeeded {
                reason: ConnectFailureReason::CatInvalid,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_service_unavailable() {
        let node = NodeBuilder::new("failure")
            .attr("reason", "503")
            .attr("message", "service unavailable")
            .build();

        let action = parse_connect_failure(&node);
        if let ConnectFailureAction::AutoReconnect { reason, message } = action {
            assert_eq!(reason, ConnectFailureReason::ServiceUnavailable);
            assert_eq!(message, "service unavailable");
        } else {
            panic!("Expected AutoReconnect, got {:?}", action);
        }
    }

    #[test]
    fn test_parse_connect_failure_internal_server_error() {
        let node = NodeBuilder::new("failure").attr("reason", "500").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::AutoReconnect {
                reason: ConnectFailureReason::InternalServerError,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_unknown() {
        let node = NodeBuilder::new("failure")
            .attr("reason", "999")
            .attr("message", "something broke")
            .build();

        let action = parse_connect_failure(&node);
        if let ConnectFailureAction::Unknown {
            reason,
            message,
            raw,
        } = action
        {
            assert_eq!(reason, ConnectFailureReason::Unknown(999));
            assert_eq!(message, "something broke");
            assert_eq!(raw.tag, "failure");
        } else {
            panic!("Expected Unknown, got {:?}", action);
        }
    }

    #[test]
    fn test_parse_connect_failure_no_reason() {
        // Missing reason attr defaults to 0, which is Unknown(0).
        let node = NodeBuilder::new("failure").build();

        let action = parse_connect_failure(&node);
        assert!(matches!(
            action,
            ConnectFailureAction::Unknown {
                reason: ConnectFailureReason::Unknown(0),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_connect_failure_temp_banned_unknown_code() {
        let node = NodeBuilder::new("failure")
            .attr("reason", "402")
            .attr("code", "999")
            .attr("expire", "3600")
            .build();

        let action = parse_connect_failure(&node);
        if let ConnectFailureAction::TemporaryBan { code, expire } = action {
            assert_eq!(code, TempBanReason::Unknown(999));
            assert_eq!(expire, Duration::seconds(3600));
        } else {
            panic!("Expected TemporaryBan, got {:?}", action);
        }
    }

    // ── Connect success tests ──────────────────────────────────────────

    #[test]
    fn test_parse_connect_success_with_lid() {
        let ts = Utc::now().timestamp();
        let node = NodeBuilder::new("success")
            .attr("t", ts.to_string())
            .attr("lid", "12345:67@lid")
            .build();

        let info = parse_connect_success(&node);
        assert!(info.lid.is_some());
        assert_eq!(info.server_timestamp, ts);
        // Offset should be close to zero since we use the current time.
        assert!(info.server_time_offset_ms.abs() < 2000);
    }

    #[test]
    fn test_parse_connect_success_without_lid() {
        let ts = Utc::now().timestamp();
        let node = NodeBuilder::new("success")
            .attr("t", ts.to_string())
            .build();

        let info = parse_connect_success(&node);
        assert!(info.lid.is_none());
        assert_eq!(info.server_timestamp, ts);
    }

    #[test]
    fn test_parse_connect_success_server_ahead() {
        // Server is 60 seconds ahead of local clock.
        let ts = Utc::now().timestamp() + 60;
        let node = NodeBuilder::new("success")
            .attr("t", ts.to_string())
            .build();

        let info = parse_connect_success(&node);
        // Offset should be approximately +60000ms, allow 2s tolerance.
        assert!(info.server_time_offset_ms > 58_000);
        assert!(info.server_time_offset_ms < 62_000);
    }

    #[test]
    fn test_parse_connect_success_server_behind() {
        // Server is 30 seconds behind local clock.
        let ts = Utc::now().timestamp() - 30;
        let node = NodeBuilder::new("success")
            .attr("t", ts.to_string())
            .build();

        let info = parse_connect_success(&node);
        // Offset should be approximately -30000ms, allow 2s tolerance.
        assert!(info.server_time_offset_ms < -28_000);
        assert!(info.server_time_offset_ms > -32_000);
    }

    #[test]
    fn test_parse_connect_success_missing_timestamp() {
        let node = NodeBuilder::new("success").build();

        let info = parse_connect_success(&node);
        assert!(info.lid.is_none());
        assert_eq!(info.server_timestamp, 0);
    }

    // ── Event conversion tests ─────────────────────────────────────────

    #[test]
    fn test_connect_failure_to_event_logged_out() {
        let action = ConnectFailureAction::LoggedOut {
            reason: ConnectFailureReason::LoggedOut,
        };
        let event = connect_failure_to_event(&action);
        assert!(matches!(event, Event::LoggedOut(_)));

        if let Event::LoggedOut(ref lo) = event {
            assert!(lo.on_connect);
            assert_eq!(lo.reason, ConnectFailureReason::LoggedOut);
        }
    }

    #[test]
    fn test_connect_failure_to_event_temp_ban() {
        let action = ConnectFailureAction::TemporaryBan {
            code: TempBanReason::BlockedByUsers,
            expire: Duration::seconds(7200),
        };
        let event = connect_failure_to_event(&action);

        if let Event::TemporaryBan(ref ban) = event {
            assert_eq!(ban.code, TempBanReason::BlockedByUsers);
            assert_eq!(ban.expire, Duration::seconds(7200));
        } else {
            panic!("Expected TemporaryBan event, got {:?}", event);
        }
    }

    #[test]
    fn test_connect_failure_to_event_client_outdated() {
        let action = ConnectFailureAction::ClientOutdated;
        let event = connect_failure_to_event(&action);
        assert!(matches!(event, Event::ClientOutdated(_)));
    }

    #[test]
    fn test_stream_error_to_event_logged_out() {
        let action = StreamErrorAction::LoggedOut {
            reason: ConnectFailureReason::LoggedOut,
        };
        let event = stream_error_to_event(&action);
        assert!(event.is_some());

        if let Some(Event::LoggedOut(ref lo)) = event {
            assert!(!lo.on_connect);
        }
    }

    #[test]
    fn test_stream_error_to_event_replaced() {
        let action = StreamErrorAction::StreamReplaced;
        let event = stream_error_to_event(&action);
        assert!(matches!(event, Some(Event::StreamReplaced(_))));
    }

    #[test]
    fn test_stream_error_to_event_reconnect_no_event() {
        let action = StreamErrorAction::Reconnect;
        assert!(stream_error_to_event(&action).is_none());
    }

    #[test]
    fn test_stream_error_to_event_server_restart_no_event() {
        let action = StreamErrorAction::ServerRestart;
        assert!(stream_error_to_event(&action).is_none());
    }

    #[test]
    fn test_stream_error_to_event_cat_refresh_no_event() {
        let action = StreamErrorAction::CatRefreshNeeded {
            code: "413".to_string(),
        };
        assert!(stream_error_to_event(&action).is_none());
    }

    #[test]
    fn test_stream_error_to_event_unknown() {
        let raw = NodeBuilder::new("stream:error").attr("code", "777").build();
        let action = StreamErrorAction::Unknown {
            code: "777".to_string(),
            raw: raw.clone(),
        };
        let event = stream_error_to_event(&action);

        if let Some(Event::StreamError(ref se)) = event {
            assert_eq!(se.code, "777");
            assert!(se.raw.is_some());
        } else {
            panic!("Expected StreamError event");
        }
    }
}
