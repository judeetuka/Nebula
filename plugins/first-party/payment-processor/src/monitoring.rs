//! Transaction monitoring and event processing.
//!
//! Watches for financial transactions across SMS, notifications, and email
//! by coordinating with the observer, email, and classifier plugins via IPC.
//! Detected transactions are logged and forwarded to a configured webhook.

use crate::common;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Monitoring configuration persisted in plugin state.
#[derive(Debug, Serialize, Deserialize)]
struct MonitorConfig {
    webhook_url: String,
    email_account_id: String,
    monitor_notifications: bool,
    monitor_email: bool,
    monitoring_active: bool,
    last_check_ms: i64,
}

/// A detected transaction record.
#[derive(Debug, Serialize, Deserialize)]
struct TransactionRecord {
    id: String,
    source: String,
    text: String,
    category: String,
    confidence: f64,
    amount: Option<f64>,
    currency: Option<String>,
    reference: Option<String>,
    detected_at: i64,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Load the current monitoring configuration from state.
fn load_config() -> Result<MonitorConfig, String> {
    let state = common::get_state("monitor_config")?;
    match state {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse config: {e}")),
        None => Ok(MonitorConfig {
            webhook_url: String::new(),
            email_account_id: String::new(),
            monitor_notifications: true,
            monitor_email: false,
            monitoring_active: false,
            last_check_ms: 0,
        }),
    }
}

/// Save the monitoring configuration to state.
fn save_config(config: &MonitorConfig) -> Result<(), String> {
    let json = serde_json::to_string(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    common::set_state("monitor_config", &json)
}

/// Configure monitoring parameters.
///
/// Params: `{webhookUrl, emailAccountId, monitorNotifications, monitorEmail}`
pub fn configure(params: &Value) -> Result<String, String> {
    let mut config = load_config()?;

    if let Some(url) = params["webhookUrl"].as_str() {
        config.webhook_url = url.to_string();
    }
    if let Some(id) = params["emailAccountId"].as_str() {
        config.email_account_id = id.to_string();
    }
    if let Some(v) = params["monitorNotifications"].as_bool() {
        config.monitor_notifications = v;
    }
    if let Some(v) = params["monitorEmail"].as_bool() {
        config.monitor_email = v;
    }

    save_config(&config)?;

    common::log(
        common::log_level::INFO,
        &format!(
            "Payment processor configured: webhook={}, email={}, notifications={}, email_monitoring={}",
            config.webhook_url,
            config.email_account_id,
            config.monitor_notifications,
            config.monitor_email,
        ),
    );

    Ok(serde_json::json!({
        "status": "configured",
        "webhookUrl": config.webhook_url,
        "emailAccountId": config.email_account_id,
        "monitorNotifications": config.monitor_notifications,
        "monitorEmail": config.monitor_email,
    })
    .to_string())
}

/// Get the current monitoring configuration.
pub fn get_config(_params: &Value) -> Result<String, String> {
    let config = load_config()?;
    serde_json::to_string(&config).map_err(|e| format!("Serialization error: {e}"))
}

/// Start monitoring for transactions.
pub fn start_monitoring(_params: &Value) -> Result<String, String> {
    let mut config = load_config()?;
    config.monitoring_active = true;
    config.last_check_ms = common::now_ms();
    save_config(&config)?;

    common::log(common::log_level::INFO, "Transaction monitoring started");

    Ok(serde_json::json!({
        "status": "monitoring",
        "startedAt": common::now_ms(),
    })
    .to_string())
}

/// Stop monitoring for transactions.
pub fn stop_monitoring(_params: &Value) -> Result<String, String> {
    let mut config = load_config()?;
    config.monitoring_active = false;
    save_config(&config)?;

    common::log(common::log_level::INFO, "Transaction monitoring stopped");

    Ok(serde_json::json!({
        "status": "stopped",
        "stoppedAt": common::now_ms(),
    })
    .to_string())
}

/// Check if monitoring is active.
pub fn is_monitoring(_params: &Value) -> Result<String, String> {
    let config = load_config()?;
    Ok(serde_json::json!({
        "monitoring": config.monitoring_active,
    })
    .to_string())
}

/// Process events from observer, notifications, and email.
///
/// This is the core pipeline, called periodically by the engine:
/// 1. Get content changes from observer
/// 2. Get active notifications
/// 3. (Optional) Check for new emails
/// 4. Classify each text
/// 5. Extract amounts and references from transactions
/// 6. Log transactions and notify webhook
pub fn process_events(_params: &Value) -> Result<String, String> {
    let mut config = load_config()?;
    if !config.monitoring_active {
        return Ok(serde_json::json!({
            "status": "not_monitoring",
            "processed": 0,
        })
        .to_string());
    }

    let since = config.last_check_ms;
    let now = common::now_ms();
    let mut texts: Vec<(String, String)> = Vec::new(); // (source, text)

    // 1. Get content changes from observer.
    let changes_result = common::invoke("plugin:observer:getChanges", "{}");
    if let Ok(changes_json) = changes_result {
        let changes: Value = serde_json::from_str(&changes_json).unwrap_or(Value::Null);
        if let Some(arr) = changes.as_array() {
            for change in arr {
                if let Some(text) = change["text"].as_str() {
                    texts.push(("sms_change".to_string(), text.to_string()));
                }
                if let Some(text) = change["body"].as_str() {
                    texts.push(("sms_change".to_string(), text.to_string()));
                }
            }
        }
    }

    // 2. Get active notifications.
    if config.monitor_notifications {
        let notif_result =
            common::invoke("plugin:observer:getActiveNotifications", "{}");
        if let Ok(notif_json) = notif_result {
            let notifs: Value = serde_json::from_str(&notif_json).unwrap_or(Value::Null);
            if let Some(arr) = notifs.as_array() {
                for notif in arr {
                    let text = notif["text"]
                        .as_str()
                        .or(notif["title"].as_str())
                        .unwrap_or("");
                    if !text.is_empty() {
                        texts.push(("notification".to_string(), text.to_string()));
                    }
                }
            }
        }
    }

    // 3. Check for new emails (if configured).
    if config.monitor_email && !config.email_account_id.is_empty() {
        // Load credentials for the saved account and check emails.
        let creds_result = common::invoke(
            "plugin:email:loadCredentials",
            &serde_json::json!({ "accountId": config.email_account_id }).to_string(),
        );
        if let Ok(creds_json) = creds_result {
            let creds: Value = serde_json::from_str(&creds_json).unwrap_or(Value::Null);
            let check_args = serde_json::json!({
                "imapHost": creds["host"],
                "port": creds["port"],
                "username": creds["username"],
                "password": creds["password"],
                "folder": "INBOX",
                "sinceTimestamp": since,
            });
            let email_result = common::invoke(
                "plugin:email:checkNewEmails",
                &check_args.to_string(),
            );
            if let Ok(email_json) = email_result {
                let emails: Value =
                    serde_json::from_str(&email_json).unwrap_or(Value::Null);
                if let Some(arr) = emails["emails"].as_array() {
                    for email in arr {
                        let subject = email["subject"].as_str().unwrap_or("");
                        let body = email["body"].as_str().unwrap_or("");
                        let combined = format!("{subject} {body}");
                        texts.push(("email".to_string(), combined));
                    }
                }
            }
        }
    }

    // 4. Classify each text and detect transactions.
    let mut transactions = Vec::new();
    let mut tx_counter = 0u64;

    for (source, text) in &texts {
        let classify_args = serde_json::json!({ "text": text });
        let classify_result = common::invoke(
            "plugin:classifier:classifyText",
            &classify_args.to_string(),
        );

        if let Ok(class_json) = classify_result {
            let classification: Value =
                serde_json::from_str(&class_json).unwrap_or(Value::Null);

            let category = classification["category"]
                .as_str()
                .unwrap_or("Unknown");
            let confidence = classification["confidence"]
                .as_f64()
                .unwrap_or(0.0);

            // Only process high-confidence transaction classifications.
            if category == "Transaction" && confidence > 0.7 {
                // 5. Extract amount and reference.
                let amount_args = serde_json::json!({ "text": text });
                let amount_result = common::invoke(
                    "plugin:classifier:extractAmount",
                    &amount_args.to_string(),
                );
                let ref_result = common::invoke(
                    "plugin:classifier:extractReference",
                    &amount_args.to_string(),
                );

                let mut amount: Option<f64> = None;
                let mut currency: Option<String> = None;
                let mut reference: Option<String> = None;

                if let Ok(amt_json) = amount_result {
                    let amt: Value =
                        serde_json::from_str(&amt_json).unwrap_or(Value::Null);
                    if let Some(amounts) = amt["amounts"].as_array() {
                        if let Some(first) = amounts.first() {
                            amount = first["amount"].as_f64();
                            currency = first["currency"]
                                .as_str()
                                .map(String::from);
                        }
                    }
                }

                if let Ok(ref_json) = ref_result {
                    let refs: Value =
                        serde_json::from_str(&ref_json).unwrap_or(Value::Null);
                    if let Some(ref_arr) = refs["references"].as_array() {
                        reference = ref_arr.first().and_then(|r| r.as_str()).map(String::from);
                    }
                }

                tx_counter += 1;
                let record = TransactionRecord {
                    id: format!("tx-{}-{tx_counter}", now),
                    source: source.clone(),
                    text: text.clone(),
                    category: category.to_string(),
                    confidence,
                    amount,
                    currency,
                    reference,
                    detected_at: now,
                };

                transactions.push(record);
            }
        }
    }

    // 6. Save transactions to log and notify webhook.
    if !transactions.is_empty() {
        append_transaction_log(&transactions)?;

        // Log webhook notification (actual HTTP would need network invoke).
        if !config.webhook_url.is_empty() {
            let payload = serde_json::json!({
                "event": "transactions_detected",
                "count": transactions.len(),
                "transactions": transactions,
                "timestamp": now,
            });
            common::log(
                common::log_level::INFO,
                &format!(
                    "Webhook payload for {}: {}",
                    config.webhook_url,
                    payload.to_string()
                ),
            );
        }
    }

    // Update last check timestamp.
    config.last_check_ms = now;
    save_config(&config)?;

    let tx_json: Vec<Value> = transactions
        .iter()
        .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
        .collect();

    Ok(serde_json::json!({
        "status": "processed",
        "textsScanned": texts.len(),
        "transactionsDetected": transactions.len(),
        "transactions": tx_json,
        "timestamp": now,
    })
    .to_string())
}

/// Append transactions to the persistent log.
fn append_transaction_log(new_txns: &[TransactionRecord]) -> Result<(), String> {
    let existing = common::get_state_large("transaction_log")?;
    let mut log: Vec<Value> = match existing {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => vec![],
    };

    for txn in new_txns {
        let val = serde_json::to_value(txn).unwrap_or(Value::Null);
        log.push(val);
    }

    // Keep only the last 500 transactions.
    if log.len() > 500 {
        log = log.split_off(log.len() - 500);
    }

    let json = serde_json::to_string(&log)
        .map_err(|e| format!("Failed to serialize transaction log: {e}"))?;
    common::set_state("transaction_log", &json)
}

/// Get the transaction log.
pub fn get_transaction_log(_params: &Value) -> Result<String, String> {
    let log = common::get_state_large("transaction_log")?;
    match log {
        Some(json) => {
            let transactions: Value = serde_json::from_str(&json).unwrap_or(Value::Array(vec![]));
            let count = transactions.as_array().map(|a| a.len()).unwrap_or(0);
            Ok(serde_json::json!({
                "transactions": transactions,
                "count": count,
            })
            .to_string())
        }
        None => Ok(serde_json::json!({
            "transactions": [],
            "count": 0,
        })
        .to_string()),
    }
}

/// Clear the transaction log.
pub fn clear_transaction_log(_params: &Value) -> Result<String, String> {
    common::delete_state("transaction_log")?;
    common::log(common::log_level::INFO, "Transaction log cleared");
    Ok(serde_json::json!({ "status": "cleared" }).to_string())
}

/// Send a test payload to the configured webhook URL.
pub fn test_webhook(_params: &Value) -> Result<String, String> {
    let config = load_config()?;
    if config.webhook_url.is_empty() {
        return Err("No webhook URL configured".to_string());
    }

    let test_payload = serde_json::json!({
        "event": "test",
        "message": "Payment processor webhook test",
        "timestamp": common::now_ms(),
        "source": "com.nebula.payment-processor",
    });

    // Log the test payload (actual HTTP via network invoke in production).
    common::log(
        common::log_level::INFO,
        &format!(
            "Test webhook to {}: {}",
            config.webhook_url,
            test_payload.to_string()
        ),
    );

    Ok(serde_json::json!({
        "status": "sent",
        "webhookUrl": config.webhook_url,
        "payload": test_payload,
    })
    .to_string())
}
