//! Headless email operations via SMTP sending and IMAP reading.
//!
//! All email operations route through the Android platform bridge which uses
//! JavaMail for headless SMTP/IMAP (no intent-based email launching).
//! Supports credential persistence so callers via IPC can use saved accounts.

use crate::common;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Persisted email credentials for an account.
#[derive(Debug, Serialize, Deserialize)]
struct EmailCredentials {
    account_id: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    /// "smtp" or "imap"
    protocol: String,
}

/// Send an email via SMTP.
///
/// Params: `{smtpHost, port, username, password, to, subject, body}`
pub fn send_email(params: &Value) -> Result<String, String> {
    let smtp_host = params["smtpHost"]
        .as_str()
        .ok_or("Missing required field: smtpHost")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")?;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let to = params["to"]
        .as_str()
        .ok_or("Missing required field: to")?;
    let subject = params["subject"]
        .as_str()
        .ok_or("Missing required field: subject")?;
    let body = params["body"]
        .as_str()
        .ok_or("Missing required field: body")?;

    common::log(
        common::log_level::DEBUG,
        &format!("Sending email to {to} via {smtp_host}:{port}"),
    );

    let args = serde_json::json!({
        "smtpHost": smtp_host,
        "port": port,
        "username": username,
        "password": password,
        "to": to,
        "subject": subject,
        "body": body,
    });

    let result = common::invoke("android:email:sendEmail", &args.to_string())?;

    Ok(serde_json::json!({
        "status": "sent",
        "to": to,
        "subject": subject,
        "timestamp": common::now_ms(),
        "result": result,
    })
    .to_string())
}

/// Read emails from an IMAP mailbox.
///
/// Params: `{imapHost, port, username, password, folder, limit}`
pub fn read_emails(params: &Value) -> Result<String, String> {
    let imap_host = params["imapHost"]
        .as_str()
        .ok_or("Missing required field: imapHost")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")?;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let folder = params["folder"].as_str().unwrap_or("INBOX");
    let limit = params["limit"].as_u64().unwrap_or(20);

    common::log(
        common::log_level::DEBUG,
        &format!("Reading emails from {imap_host}:{port}/{folder} (limit: {limit})"),
    );

    let args = serde_json::json!({
        "imapHost": imap_host,
        "port": port,
        "username": username,
        "password": password,
        "folder": folder,
        "limit": limit,
    });

    common::invoke("android:email:readEmails", &args.to_string())
}

/// Check for new emails since a given timestamp.
///
/// Params: `{imapHost, port, username, password, folder, sinceTimestamp}`
pub fn check_new_emails(params: &Value) -> Result<String, String> {
    let imap_host = params["imapHost"]
        .as_str()
        .ok_or("Missing required field: imapHost")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")?;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let folder = params["folder"].as_str().unwrap_or("INBOX");
    let since_timestamp = params["sinceTimestamp"]
        .as_i64()
        .ok_or("Missing required field: sinceTimestamp")?;

    // Read recent emails and filter by timestamp.
    let args = serde_json::json!({
        "imapHost": imap_host,
        "port": port,
        "username": username,
        "password": password,
        "folder": folder,
        "limit": 50,
    });

    let result = common::invoke("android:email:readEmails", &args.to_string())?;

    // Parse the result and filter by timestamp.
    let emails: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));

    let filtered = if let Some(arr) = emails.as_array() {
        let new_emails: Vec<&Value> = arr
            .iter()
            .filter(|e| {
                e["timestamp"]
                    .as_i64()
                    .map(|ts| ts >= since_timestamp)
                    .unwrap_or(false)
            })
            .collect();
        serde_json::json!({
            "emails": new_emails,
            "count": new_emails.len(),
            "sinceTimestamp": since_timestamp,
        })
    } else {
        serde_json::json!({
            "emails": emails,
            "count": 0,
            "sinceTimestamp": since_timestamp,
        })
    };

    Ok(filtered.to_string())
}

/// Search emails by query in subject/body.
///
/// Params: `{imapHost, port, username, password, folder, query, limit}`
pub fn search_emails(params: &Value) -> Result<String, String> {
    let imap_host = params["imapHost"]
        .as_str()
        .ok_or("Missing required field: imapHost")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")?;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let folder = params["folder"].as_str().unwrap_or("INBOX");
    let query = params["query"]
        .as_str()
        .ok_or("Missing required field: query")?;
    let limit = params["limit"].as_u64().unwrap_or(20);

    // Read emails and filter by query match in subject or body.
    let args = serde_json::json!({
        "imapHost": imap_host,
        "port": port,
        "username": username,
        "password": password,
        "folder": folder,
        "limit": 200,
    });

    let result = common::invoke("android:email:readEmails", &args.to_string())?;
    let emails: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));

    let query_lower = query.to_lowercase();

    let matched = if let Some(arr) = emails.as_array() {
        let hits: Vec<&Value> = arr
            .iter()
            .filter(|e| {
                let subject_match = e["subject"]
                    .as_str()
                    .map(|s| s.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                let body_match = e["body"]
                    .as_str()
                    .map(|b| b.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                subject_match || body_match
            })
            .take(limit as usize)
            .collect();
        serde_json::json!({
            "emails": hits,
            "count": hits.len(),
            "query": query,
        })
    } else {
        serde_json::json!({
            "emails": [],
            "count": 0,
            "query": query,
        })
    };

    Ok(matched.to_string())
}

/// Get the total email count in a folder.
///
/// Params: `{imapHost, port, username, password, folder}`
pub fn get_email_count(params: &Value) -> Result<String, String> {
    let imap_host = params["imapHost"]
        .as_str()
        .ok_or("Missing required field: imapHost")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")?;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let folder = params["folder"].as_str().unwrap_or("INBOX");

    // Use limit=0 to request just a count from the bridge.
    let args = serde_json::json!({
        "imapHost": imap_host,
        "port": port,
        "username": username,
        "password": password,
        "folder": folder,
        "limit": 0,
    });

    let result = common::invoke("android:email:readEmails", &args.to_string())?;

    // Parse the result to extract count.
    let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Null);
    let count = if let Some(arr) = parsed.as_array() {
        arr.len()
    } else {
        parsed["count"].as_u64().unwrap_or(0) as usize
    };

    Ok(serde_json::json!({
        "folder": folder,
        "count": count,
    })
    .to_string())
}

/// Save email credentials for an account into plugin state.
///
/// Params: `{accountId, host, port, username, password, protocol}`
pub fn save_credentials(params: &Value) -> Result<String, String> {
    let account_id = params["accountId"]
        .as_str()
        .ok_or("Missing required field: accountId")?;
    let host = params["host"]
        .as_str()
        .ok_or("Missing required field: host")?;
    let port = params["port"]
        .as_u64()
        .ok_or("Missing required field: port")? as u16;
    let username = params["username"]
        .as_str()
        .ok_or("Missing required field: username")?;
    let password = params["password"]
        .as_str()
        .ok_or("Missing required field: password")?;
    let protocol = params["protocol"]
        .as_str()
        .ok_or("Missing required field: protocol")?;

    let creds = EmailCredentials {
        account_id: account_id.to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        password: password.to_string(),
        protocol: protocol.to_string(),
    };

    let key = format!("credentials:{account_id}");
    let value = serde_json::to_string(&creds)
        .map_err(|e| format!("Failed to serialize credentials: {e}"))?;
    common::set_state(&key, &value)?;

    common::log(
        common::log_level::INFO,
        &format!("Saved credentials for account: {account_id}"),
    );

    Ok(serde_json::json!({
        "status": "saved",
        "accountId": account_id,
        "protocol": protocol,
    })
    .to_string())
}

/// Load email credentials for an account from plugin state.
///
/// Params: `{accountId: string}`
pub fn load_credentials(params: &Value) -> Result<String, String> {
    let account_id = params["accountId"]
        .as_str()
        .ok_or("Missing required field: accountId")?;

    let key = format!("credentials:{account_id}");
    let value = common::get_state(&key)?;

    match value {
        Some(json) => Ok(json),
        None => Err(format!("No credentials found for account: {account_id}")),
    }
}

/// Delete email credentials for an account.
///
/// Params: `{accountId: string}`
pub fn delete_credentials(params: &Value) -> Result<String, String> {
    let account_id = params["accountId"]
        .as_str()
        .ok_or("Missing required field: accountId")?;

    let key = format!("credentials:{account_id}");
    common::delete_state(&key)?;

    common::log(
        common::log_level::INFO,
        &format!("Deleted credentials for account: {account_id}"),
    );

    Ok(serde_json::json!({
        "status": "deleted",
        "accountId": account_id,
    })
    .to_string())
}
