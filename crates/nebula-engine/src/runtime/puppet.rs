//! Puppet mode controller for the master node.

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetAction {
    pub action_id: String,
    pub action_type: PuppetActionType,
    pub target_node: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PuppetActionType {
    UssdDial { code: String },
    SmsSend { to: String, message: String },
    ScreenTap { x: f32, y: f32 },
    TypeText { text: String },
    Navigate { direction: String },
    PluginAction { plugin_id: String, action: String },
    Custom { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetResult {
    pub action_id: String,
    pub target_node: String,
    pub success: bool,
    pub result_data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub executed_at: i64,
}

#[derive(Debug, Clone)]
pub struct PuppetSession {
    pub target_node: String,
    pub started_at: i64,
    pub actions_sent: u32,
    pub active: bool,
}

/// Manages puppet mode from the master node.
pub struct PuppetController {
    sessions: Arc<RwLock<HashMap<String, PuppetSession>>>,
    action_history: Arc<RwLock<Vec<PuppetAction>>>,
    result_history: Arc<RwLock<Vec<PuppetResult>>>,
    max_history: usize,
}

impl PuppetController {
    pub fn new(max_history: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            action_history: Arc::new(RwLock::new(Vec::new())),
            result_history: Arc::new(RwLock::new(Vec::new())),
            max_history,
        }
    }

    pub async fn start_session(&self, target_node: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get(target_node) {
            if s.active { bail!("Puppet session already active for node '{}'", target_node); }
        }
        sessions.insert(target_node.to_string(), PuppetSession {
            target_node: target_node.to_string(),
            started_at: chrono::Utc::now().timestamp(),
            actions_sent: 0, active: true,
        });
        Ok(())
    }

    pub async fn end_session(&self, target_node: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        match sessions.get_mut(target_node) {
            Some(s) if s.active => { s.active = false; Ok(()) }
            Some(_) => bail!("Puppet session for node '{}' is not active", target_node),
            None => bail!("No puppet session found for node '{}'", target_node),
        }
    }

    pub async fn is_session_active(&self, target_node: &str) -> bool {
        self.sessions.read().await.get(target_node).map_or(false, |s| s.active)
    }

    pub async fn queue_action(&self, action: PuppetAction) -> Result<()> {
        {
            let mut sessions = self.sessions.write().await;
            match sessions.get_mut(&action.target_node) {
                Some(s) if s.active => s.actions_sent += 1,
                _ => bail!("No active puppet session for node '{}'", action.target_node),
            }
        }
        let mut history = self.action_history.write().await;
        history.push(action);
        while history.len() > self.max_history { history.remove(0); }
        Ok(())
    }

    pub async fn record_result(&self, result: PuppetResult) {
        let mut history = self.result_history.write().await;
        history.push(result);
        while history.len() > self.max_history { history.remove(0); }
    }

    pub async fn active_sessions(&self) -> Vec<PuppetSession> {
        self.sessions.read().await.values().filter(|s| s.active).cloned().collect()
    }

    pub async fn action_history(&self, target_node: &str) -> Vec<PuppetAction> {
        self.action_history.read().await.iter().filter(|a| a.target_node == target_node).cloned().collect()
    }

    pub async fn result_history(&self, target_node: &str) -> Vec<PuppetResult> {
        self.result_history.read().await.iter().filter(|r| r.target_node == target_node).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action(id: &str, target: &str, t: PuppetActionType) -> PuppetAction {
        PuppetAction { action_id: id.into(), action_type: t, target_node: target.into(),
            payload: serde_json::json!({}), timestamp: chrono::Utc::now().timestamp() }
    }

    fn make_result(id: &str, target: &str, ok: bool) -> PuppetResult {
        PuppetResult { action_id: id.into(), target_node: target.into(), success: ok,
            result_data: if ok { Some(serde_json::json!({"ok":true})) } else { None },
            error: if ok { None } else { Some("failed".into()) }, executed_at: chrono::Utc::now().timestamp() }
    }

    #[tokio::test]
    async fn test_start_and_end_session() {
        let c = PuppetController::new(100);
        c.start_session("w1").await.unwrap();
        assert!(c.is_session_active("w1").await);
        c.end_session("w1").await.unwrap();
        assert!(!c.is_session_active("w1").await);
    }

    #[tokio::test]
    async fn test_is_session_active() {
        let c = PuppetController::new(100);
        assert!(!c.is_session_active("w1").await);
        c.start_session("w1").await.unwrap();
        assert!(c.is_session_active("w1").await);
        assert!(!c.is_session_active("w2").await);
    }

    #[tokio::test]
    async fn test_queue_action() {
        let c = PuppetController::new(100);
        c.start_session("w1").await.unwrap();
        c.queue_action(make_action("a1", "w1", PuppetActionType::UssdDial { code: "*123#".into() })).await.unwrap();
        assert_eq!(c.action_history("w1").await.len(), 1);
    }

    #[tokio::test]
    async fn test_record_result() {
        let c = PuppetController::new(100);
        c.record_result(make_result("a1", "w1", true)).await;
        assert_eq!(c.result_history("w1").await.len(), 1);
    }

    #[tokio::test]
    async fn test_active_sessions() {
        let c = PuppetController::new(100);
        c.start_session("w1").await.unwrap();
        c.start_session("w2").await.unwrap();
        c.start_session("w3").await.unwrap();
        c.end_session("w2").await.unwrap();
        assert_eq!(c.active_sessions().await.len(), 2);
    }

    #[tokio::test]
    async fn test_action_history_filtering() {
        let c = PuppetController::new(100);
        c.start_session("w1").await.unwrap();
        c.start_session("w2").await.unwrap();
        c.queue_action(make_action("a1", "w1", PuppetActionType::ScreenTap { x: 0.5, y: 0.5 })).await.unwrap();
        c.queue_action(make_action("a2", "w2", PuppetActionType::Navigate { direction: "back".into() })).await.unwrap();
        c.queue_action(make_action("a3", "w1", PuppetActionType::TypeText { text: "hi".into() })).await.unwrap();
        assert_eq!(c.action_history("w1").await.len(), 2);
        assert_eq!(c.action_history("w2").await.len(), 1);
    }

    #[tokio::test]
    async fn test_result_history_filtering() {
        let c = PuppetController::new(100);
        c.record_result(make_result("r1", "w1", true)).await;
        c.record_result(make_result("r2", "w2", false)).await;
        c.record_result(make_result("r3", "w1", true)).await;
        assert_eq!(c.result_history("w1").await.len(), 2);
        assert_eq!(c.result_history("w2").await.len(), 1);
    }

    #[tokio::test]
    async fn test_puppet_action_type_serialization() {
        let types = vec![
            PuppetActionType::UssdDial { code: "*100#".into() },
            PuppetActionType::SmsSend { to: "+1234".into(), message: "hi".into() },
            PuppetActionType::ScreenTap { x: 0.25, y: 0.75 },
            PuppetActionType::TypeText { text: "test".into() },
            PuppetActionType::Navigate { direction: "home".into() },
            PuppetActionType::PluginAction { plugin_id: "p".into(), action: "a".into() },
            PuppetActionType::Custom { name: "reboot".into() },
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let back: PuppetActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, back);
        }
    }

    #[tokio::test]
    async fn test_max_history_limit() {
        let c = PuppetController::new(3);
        c.start_session("w1").await.unwrap();
        for i in 0..5 {
            c.queue_action(make_action(&format!("a{i}"), "w1", PuppetActionType::ScreenTap { x: i as f32, y: 0.0 })).await.unwrap();
        }
        let h = c.action_history("w1").await;
        assert_eq!(h.len(), 3);
        assert_eq!(h[0].action_id, "a2");
    }

    #[tokio::test]
    async fn test_start_duplicate_session_fails() {
        let c = PuppetController::new(100);
        c.start_session("w1").await.unwrap();
        assert!(c.start_session("w1").await.is_err());
    }

    #[tokio::test]
    async fn test_end_nonexistent_session_fails() {
        let c = PuppetController::new(100);
        assert!(c.end_session("ghost").await.is_err());
    }
}
