//! Contacts and calendar operations.
//!
//! Read/write contacts and calendar events via the Android platform bridge.
//! Supports offline caching of contacts via plugin state and bulk export.

use crate::common;
use serde_json::Value;

/// Read contacts from the device.
///
/// Params: `{limit?: number}`
pub fn read_contacts(params: &Value) -> Result<String, String> {
    let limit = params["limit"].as_u64().unwrap_or(100);

    common::log(
        common::log_level::DEBUG,
        &format!("Reading contacts (limit: {limit})"),
    );

    let args = serde_json::json!({ "limit": limit });
    common::invoke("android:contacts:readContacts", &args.to_string())
}

/// Add a new contact to the device.
///
/// Params: `{name: string, phone?: string, email?: string}`
pub fn add_contact(params: &Value) -> Result<String, String> {
    let name = params["name"]
        .as_str()
        .ok_or("Missing required field: name")?;
    let phone = params["phone"].as_str().unwrap_or("");
    let email = params["email"].as_str().unwrap_or("");

    common::log(
        common::log_level::DEBUG,
        &format!("Adding contact: {name}"),
    );

    let args = serde_json::json!({
        "name": name,
        "phone": phone,
        "email": email,
    });

    let result = common::invoke("android:contacts:addContact", &args.to_string())?;

    Ok(serde_json::json!({
        "status": "added",
        "name": name,
        "timestamp": common::now_ms(),
        "result": result,
    })
    .to_string())
}

/// Delete a contact by ID.
///
/// Params: `{contactId: string}`
pub fn delete_contact(params: &Value) -> Result<String, String> {
    let contact_id = params["contactId"]
        .as_str()
        .ok_or("Missing required field: contactId")?;

    common::log(
        common::log_level::DEBUG,
        &format!("Deleting contact: {contact_id}"),
    );

    let args = serde_json::json!({ "contactId": contact_id });
    let result = common::invoke("android:contacts:deleteContact", &args.to_string())?;

    Ok(serde_json::json!({
        "status": "deleted",
        "contactId": contact_id,
        "result": result,
    })
    .to_string())
}

/// Search contacts by name or phone number.
///
/// Params: `{query: string, limit?: number}`
pub fn search_contacts(params: &Value) -> Result<String, String> {
    let query = params["query"]
        .as_str()
        .ok_or("Missing required field: query")?;
    let limit = params["limit"].as_u64().unwrap_or(50);

    // Read contacts and filter locally.
    let args = serde_json::json!({ "limit": 1000 });
    let result = common::invoke("android:contacts:readContacts", &args.to_string())?;

    let contacts: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
    let query_lower = query.to_lowercase();

    let matched = if let Some(arr) = contacts.as_array() {
        let hits: Vec<&Value> = arr
            .iter()
            .filter(|c| {
                let name_match = c["name"]
                    .as_str()
                    .map(|n| n.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                let phone_match = c["phone"]
                    .as_str()
                    .map(|p| p.contains(query))
                    .unwrap_or(false);
                name_match || phone_match
            })
            .take(limit as usize)
            .collect();
        serde_json::json!({
            "contacts": hits,
            "count": hits.len(),
            "query": query,
        })
    } else {
        serde_json::json!({
            "contacts": [],
            "count": 0,
            "query": query,
        })
    };

    Ok(matched.to_string())
}

/// Find a contact by phone number.
///
/// Params: `{phone: string}`
pub fn get_contact_by_phone(params: &Value) -> Result<String, String> {
    let phone = params["phone"]
        .as_str()
        .ok_or("Missing required field: phone")?;

    // Read contacts and filter by exact phone match.
    let args = serde_json::json!({ "limit": 5000 });
    let result = common::invoke("android:contacts:readContacts", &args.to_string())?;

    let contacts: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));

    // Strip non-digit characters for comparison.
    let phone_digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

    let found = if let Some(arr) = contacts.as_array() {
        arr.iter()
            .find(|c| {
                c["phone"]
                    .as_str()
                    .map(|p| {
                        let p_digits: String =
                            p.chars().filter(|ch| ch.is_ascii_digit()).collect();
                        p_digits.ends_with(&phone_digits) || phone_digits.ends_with(&p_digits)
                    })
                    .unwrap_or(false)
            })
            .cloned()
    } else {
        None
    };

    match found {
        Some(contact) => Ok(contact.to_string()),
        None => Ok(serde_json::json!({
            "found": false,
            "phone": phone,
        })
        .to_string()),
    }
}

/// Read calendar events from the device.
///
/// Params: `{limit?: number}`
///
/// Returns an "unsupported" error gracefully if the Android bridge does not
/// implement this method.
pub fn read_calendar_events(params: &Value) -> Result<String, String> {
    let limit = params["limit"].as_u64().unwrap_or(50);

    common::log(
        common::log_level::DEBUG,
        &format!("Reading calendar events (limit: {limit})"),
    );

    let args = serde_json::json!({ "limit": limit });
    let result = common::invoke("android:contacts:readCalendarEvents", &args.to_string());

    match result {
        Ok(data) => Ok(data),
        Err(e) => {
            common::log(
                common::log_level::WARN,
                &format!("Calendar events not supported: {e}"),
            );
            Ok(serde_json::json!({
                "error": "unsupported",
                "message": "Calendar event reading is not available on this device",
                "details": e,
            })
            .to_string())
        }
    }
}

/// Add a calendar event.
///
/// Params: `{title: string, description?: string, startMs: number, endMs: number}`
///
/// Returns an "unsupported" error gracefully if the Android bridge does not
/// implement this method.
pub fn add_calendar_event(params: &Value) -> Result<String, String> {
    let title = params["title"]
        .as_str()
        .ok_or("Missing required field: title")?;
    let description = params["description"].as_str().unwrap_or("");
    let start_ms = params["startMs"]
        .as_i64()
        .ok_or("Missing required field: startMs")?;
    let end_ms = params["endMs"]
        .as_i64()
        .ok_or("Missing required field: endMs")?;

    common::log(
        common::log_level::DEBUG,
        &format!("Adding calendar event: {title}"),
    );

    let args = serde_json::json!({
        "title": title,
        "description": description,
        "startMs": start_ms,
        "endMs": end_ms,
    });

    let result = common::invoke("android:contacts:addCalendarEvent", &args.to_string());

    match result {
        Ok(data) => Ok(serde_json::json!({
            "status": "added",
            "title": title,
            "result": data,
        })
        .to_string()),
        Err(e) => {
            common::log(
                common::log_level::WARN,
                &format!("Calendar events not supported: {e}"),
            );
            Ok(serde_json::json!({
                "error": "unsupported",
                "message": "Calendar event creation is not available on this device",
                "details": e,
            })
            .to_string())
        }
    }
}

/// Sync all contacts to plugin state for offline access.
///
/// Reads all contacts and persists them as a JSON blob in the plugin's
/// key-value store under the key `"contacts:cache"`.
pub fn sync_contacts_to_state(_params: &Value) -> Result<String, String> {
    common::log(common::log_level::INFO, "Syncing contacts to state");

    let args = serde_json::json!({ "limit": 10000 });
    let result = common::invoke("android:contacts:readContacts", &args.to_string())?;

    let contacts: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
    let count = contacts.as_array().map(|a| a.len()).unwrap_or(0);

    let cache = serde_json::json!({
        "contacts": contacts,
        "count": count,
        "syncedAt": common::now_ms(),
    });

    common::set_state("contacts:cache", &cache.to_string())?;

    common::log(
        common::log_level::INFO,
        &format!("Synced {count} contacts to state"),
    );

    Ok(serde_json::json!({
        "status": "synced",
        "count": count,
        "timestamp": common::now_ms(),
    })
    .to_string())
}

/// Get the total contact count.
///
/// Reads a minimal set of contacts to determine the count.
pub fn get_contact_count(_params: &Value) -> Result<String, String> {
    // Read with a high limit; the bridge returns all contacts.
    let args = serde_json::json!({ "limit": 1 });
    let result = common::invoke("android:contacts:readContacts", &args.to_string())?;

    let contacts: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));

    // The bridge may return a count field or just the array.
    let count = if let Some(n) = contacts["totalCount"].as_u64() {
        n
    } else if let Some(arr) = contacts.as_array() {
        arr.len() as u64
    } else {
        0
    };

    Ok(serde_json::json!({
        "count": count,
    })
    .to_string())
}

/// Export all contacts as a full JSON dump.
pub fn export_contacts(_params: &Value) -> Result<String, String> {
    common::log(common::log_level::INFO, "Exporting all contacts");

    let args = serde_json::json!({ "limit": 10000 });
    let result = common::invoke("android:contacts:readContacts", &args.to_string())?;

    let contacts: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
    let count = contacts.as_array().map(|a| a.len()).unwrap_or(0);

    Ok(serde_json::json!({
        "contacts": contacts,
        "count": count,
        "exportedAt": common::now_ms(),
    })
    .to_string())
}
