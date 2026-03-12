//! USSD payment operations.
//!
//! Triggers USSD-based payments and multi-step USSD flows by coordinating
//! with the comm-link plugin via IPC. Maintains a payment history log.

use crate::common;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A USSD payment record.
#[derive(Debug, Serialize, Deserialize)]
struct PaymentRecord {
    id: String,
    code: String,
    sim_slot: u32,
    steps: Vec<String>,
    status: String,
    result: String,
    initiated_at: i64,
}

/// Trigger a single-step USSD payment.
///
/// Params: `{code: string, simSlot?: number}`
pub fn trigger_ussd_payment(params: &Value) -> Result<String, String> {
    let code = params["code"]
        .as_str()
        .ok_or("Missing required field: code")?;
    let sim_slot = params["simSlot"].as_u64().unwrap_or(0) as u32;

    common::log(
        common::log_level::INFO,
        &format!("Triggering USSD payment: {code} on SIM {sim_slot}"),
    );

    let args = serde_json::json!({
        "code": code,
        "simSlot": sim_slot,
    });

    let result = common::invoke(
        "plugin:comm-link:executeUssd",
        &args.to_string(),
    )?;

    let now = common::now_ms();
    let record = PaymentRecord {
        id: format!("pay-{now}"),
        code: code.to_string(),
        sim_slot,
        steps: vec![code.to_string()],
        status: "completed".to_string(),
        result: result.clone(),
        initiated_at: now,
    };

    append_payment_history(&record)?;

    Ok(serde_json::json!({
        "status": "completed",
        "code": code,
        "simSlot": sim_slot,
        "result": result,
        "paymentId": record.id,
        "timestamp": now,
    })
    .to_string())
}

/// Execute a multi-step USSD payment flow.
///
/// Params: `{steps: [string], simSlot?: number}`
///
/// The first step dials the USSD code, subsequent steps send replies
/// within the active session.
pub fn start_ussd_payment_flow(params: &Value) -> Result<String, String> {
    let steps = params["steps"]
        .as_array()
        .ok_or("Missing required field: steps (array)")?;
    let sim_slot = params["simSlot"].as_u64().unwrap_or(0) as u32;

    if steps.is_empty() {
        return Err("Steps array must not be empty".to_string());
    }

    let step_strings: Vec<String> = steps
        .iter()
        .filter_map(|s| s.as_str().map(String::from))
        .collect();

    common::log(
        common::log_level::INFO,
        &format!(
            "Starting USSD payment flow with {} steps on SIM {sim_slot}",
            step_strings.len()
        ),
    );

    // Step 1: Start the USSD session with the first code.
    let first_code = &step_strings[0];
    let start_args = serde_json::json!({
        "code": first_code,
        "simSlot": sim_slot,
    });

    let start_result = common::invoke(
        "plugin:comm-link:startUssdSession",
        &start_args.to_string(),
    )?;

    let mut results = vec![start_result];

    // Steps 2+: Send replies in sequence.
    for step in step_strings.iter().skip(1) {
        let reply_args = serde_json::json!({
            "reply": step,
        });

        let reply_result = common::invoke(
            "plugin:comm-link:sendUssdReply",
            &reply_args.to_string(),
        );

        match reply_result {
            Ok(r) => results.push(r),
            Err(e) => {
                common::log(
                    common::log_level::ERROR,
                    &format!("USSD step failed at '{}': {}", step, e),
                );
                results.push(format!("error: {e}"));
                break;
            }
        }
    }

    let now = common::now_ms();
    let record = PaymentRecord {
        id: format!("pay-{now}"),
        code: first_code.clone(),
        sim_slot,
        steps: step_strings.clone(),
        status: "completed".to_string(),
        result: results.last().cloned().unwrap_or_default(),
        initiated_at: now,
    };

    append_payment_history(&record)?;

    Ok(serde_json::json!({
        "status": "completed",
        "steps": step_strings,
        "results": results,
        "simSlot": sim_slot,
        "paymentId": record.id,
        "timestamp": now,
    })
    .to_string())
}

/// Get USSD payment history.
pub fn get_payment_history(_params: &Value) -> Result<String, String> {
    let history = common::get_state_large("payment_history")?;
    match history {
        Some(json) => {
            let payments: Value = serde_json::from_str(&json).unwrap_or(Value::Array(vec![]));
            let count = payments.as_array().map(|a| a.len()).unwrap_or(0);
            Ok(serde_json::json!({
                "payments": payments,
                "count": count,
            })
            .to_string())
        }
        None => Ok(serde_json::json!({
            "payments": [],
            "count": 0,
        })
        .to_string()),
    }
}

/// Append a payment record to the persistent history.
fn append_payment_history(record: &PaymentRecord) -> Result<(), String> {
    let existing = common::get_state_large("payment_history")?;
    let mut history: Vec<Value> = match existing {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => vec![],
    };

    let val =
        serde_json::to_value(record).map_err(|e| format!("Serialization error: {e}"))?;
    history.push(val);

    // Keep only the last 200 payments.
    if history.len() > 200 {
        history = history.split_off(history.len() - 200);
    }

    let json = serde_json::to_string(&history)
        .map_err(|e| format!("Failed to serialize payment history: {e}"))?;
    common::set_state("payment_history", &json)
}
