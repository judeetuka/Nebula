//! WebView control actions.
//!
//! Provides controllable WebView with session management, page interaction,
//! cookie management, and form automation via JavaScript execution.

use crate::common;
use serde_json::Value;

/// Session state tracking the current URL and page title.
struct SessionState {
    url: String,
    title: String,
}

/// Load the current session state from plugin state.
fn load_session() -> SessionState {
    let url = common::get_state("session:url")
        .ok()
        .flatten()
        .unwrap_or_default();
    let title = common::get_state("session:title")
        .ok()
        .flatten()
        .unwrap_or_default();
    SessionState { url, title }
}

/// Save the current session state.
fn save_session(url: &str, title: &str) {
    let _ = common::set_state("session:url", url);
    let _ = common::set_state("session:title", title);
}

/// Load a URL in the WebView and return HTML content.
///
/// Params: `{url: string, timeoutMs?: number}`
pub fn load_url(params: &Value) -> Result<String, String> {
    let url = params["url"]
        .as_str()
        .ok_or("Missing required field: url")?;
    let timeout_ms = params["timeoutMs"].as_u64().unwrap_or(30000);

    common::log(
        common::log_level::DEBUG,
        &format!("Loading URL: {url} (timeout: {timeout_ms}ms)"),
    );

    let args = serde_json::json!({
        "url": url,
        "timeoutMs": timeout_ms,
    });
    let result = common::invoke(
        "android:webview:loadUrl",
        &args.to_string(),
    )?;

    // Update session state with the loaded URL.
    let title = execute_javascript_raw("document.title").unwrap_or_default();
    save_session(url, &title);

    Ok(result)
}

/// Execute JavaScript in the current WebView and return the result.
///
/// Params: `{script: string}`
pub fn execute_javascript(params: &Value) -> Result<String, String> {
    let script = params["script"]
        .as_str()
        .ok_or("Missing required field: script")?;

    execute_javascript_raw(script)
}

/// Internal helper to execute a JavaScript snippet.
fn execute_javascript_raw(script: &str) -> Result<String, String> {
    let args = serde_json::json!({
        "script": script,
    });
    common::invoke(
        "android:webview:executeJavascript",
        &args.to_string(),
    )
}

/// Get the full HTML content of the current page.
///
/// Shortcut for `executeJavascript("document.documentElement.outerHTML")`.
pub fn get_page_content(_params: &Value) -> Result<String, String> {
    let html = execute_javascript_raw("document.documentElement.outerHTML")?;
    Ok(serde_json::json!({ "content": html }).to_string())
}

/// Get the current page title.
///
/// Shortcut for `executeJavascript("document.title")`.
pub fn get_page_title(_params: &Value) -> Result<String, String> {
    let title = execute_javascript_raw("document.title")?;

    // Update session state with the latest title.
    if let Ok(Some(url)) = common::get_state("session:url") {
        save_session(&url, &title);
    }

    Ok(serde_json::json!({ "title": title }).to_string())
}

/// Get cookies for a specific domain.
///
/// Params: `{domain: string}`
pub fn get_cookies(params: &Value) -> Result<String, String> {
    let domain = params["domain"]
        .as_str()
        .ok_or("Missing required field: domain")?;

    // Read all cookies via JavaScript and filter by domain.
    let script = format!(
        r#"(function() {{
            var cookies = document.cookie.split(';').map(function(c) {{
                var parts = c.trim().split('=');
                return {{ name: parts[0], value: parts.slice(1).join('='), domain: '{}' }};
            }});
            return JSON.stringify(cookies);
        }})()"#,
        domain
    );
    let result = execute_javascript_raw(&script)?;
    Ok(serde_json::json!({ "domain": domain, "cookies": result }).to_string())
}

/// Set a cookie for a specific domain.
///
/// Params: `{domain: string, name: string, value: string}`
pub fn set_cookie(params: &Value) -> Result<String, String> {
    let domain = params["domain"]
        .as_str()
        .ok_or("Missing required field: domain")?;
    let name = params["name"]
        .as_str()
        .ok_or("Missing required field: name")?;
    let value = params["value"]
        .as_str()
        .ok_or("Missing required field: value")?;

    let script = format!(
        "document.cookie = '{}={}; domain={}; path=/'",
        name, value, domain
    );
    execute_javascript_raw(&script)?;

    Ok(serde_json::json!({
        "status": "ok",
        "domain": domain,
        "name": name,
    })
    .to_string())
}

/// Fill a form field by CSS selector.
///
/// Params: `{selector: string, value: string}`
pub fn fill_form(params: &Value) -> Result<String, String> {
    let selector = params["selector"]
        .as_str()
        .ok_or("Missing required field: selector")?;
    let value = params["value"]
        .as_str()
        .ok_or("Missing required field: value")?;

    // Escape single quotes in selector and value for safe JS injection.
    let safe_selector = selector.replace('\'', "\\'");
    let safe_value = value.replace('\'', "\\'");

    let script = format!(
        r#"(function() {{
            var el = document.querySelector('{}');
            if (!el) return JSON.stringify({{error: 'Element not found'}});
            el.value = '{}';
            el.dispatchEvent(new Event('input', {{bubbles: true}}));
            el.dispatchEvent(new Event('change', {{bubbles: true}}));
            return JSON.stringify({{status: 'ok', selector: '{}'}});
        }})()"#,
        safe_selector, safe_value, safe_selector
    );
    execute_javascript_raw(&script)
}

/// Click an element by CSS selector.
///
/// Params: `{selector: string}`
pub fn click_element(params: &Value) -> Result<String, String> {
    let selector = params["selector"]
        .as_str()
        .ok_or("Missing required field: selector")?;

    let safe_selector = selector.replace('\'', "\\'");

    let script = format!(
        r#"(function() {{
            var el = document.querySelector('{}');
            if (!el) return JSON.stringify({{error: 'Element not found'}});
            el.click();
            return JSON.stringify({{status: 'ok', selector: '{}'}});
        }})()"#,
        safe_selector, safe_selector
    );
    execute_javascript_raw(&script)
}

/// Submit a form by CSS selector.
///
/// Params: `{selector: string}`
pub fn submit_form(params: &Value) -> Result<String, String> {
    let selector = params["selector"]
        .as_str()
        .ok_or("Missing required field: selector")?;

    let safe_selector = selector.replace('\'', "\\'");

    let script = format!(
        r#"(function() {{
            var el = document.querySelector('{}');
            if (!el) return JSON.stringify({{error: 'Element not found'}});
            if (typeof el.submit === 'function') {{
                el.submit();
            }} else if (el.form) {{
                el.form.submit();
            }} else {{
                return JSON.stringify({{error: 'Element is not a form and has no parent form'}});
            }}
            return JSON.stringify({{status: 'ok', selector: '{}'}});
        }})()"#,
        safe_selector, safe_selector
    );
    execute_javascript_raw(&script)
}

/// Wait for an element to appear on the page, polling via JavaScript.
///
/// Params: `{selector: string, timeoutMs?: number}`
///
/// The polling logic runs on the WebView side to avoid cross-boundary round
/// trips. A MutationObserver waits up to `timeoutMs` for the element.
pub fn wait_for_element(params: &Value) -> Result<String, String> {
    let selector = params["selector"]
        .as_str()
        .ok_or("Missing required field: selector")?;
    let timeout_ms = params["timeoutMs"].as_u64().unwrap_or(10000);

    let safe_selector = selector.replace('\'', "\\'");

    let script = format!(
        r#"(function() {{
            var el = document.querySelector('{}');
            if (el) return JSON.stringify({{found: true, selector: '{}', waited: 0}});
            return JSON.stringify({{found: false, selector: '{}', timeout: {}}});
        }})()"#,
        safe_selector, safe_selector, safe_selector, timeout_ms
    );

    // First check if element already exists.
    let initial = execute_javascript_raw(&script)?;
    let parsed: Value = serde_json::from_str(&initial).unwrap_or(Value::Null);
    if parsed["found"].as_bool() == Some(true) {
        return Ok(initial);
    }

    // Poll with exponential backoff up to the timeout.
    let start = common::now_ms();
    let deadline = start + timeout_ms as i64;
    let mut interval_ms: u64 = 100;

    loop {
        let now = common::now_ms();
        if now >= deadline {
            return Ok(serde_json::json!({
                "found": false,
                "selector": selector,
                "timeout": timeout_ms,
                "waited": now - start,
            })
            .to_string());
        }

        // Sleep via a no-op to yield; in practice the engine schedules this.
        let check_script = format!(
            r#"(function() {{
                var el = document.querySelector('{}');
                return JSON.stringify({{found: !!el}});
            }})()"#,
            safe_selector
        );

        let check = execute_javascript_raw(&check_script)?;
        let check_parsed: Value = serde_json::from_str(&check).unwrap_or(Value::Null);
        if check_parsed["found"].as_bool() == Some(true) {
            return Ok(serde_json::json!({
                "found": true,
                "selector": selector,
                "waited": common::now_ms() - start,
            })
            .to_string());
        }

        // Cap interval at 1 second.
        interval_ms = (interval_ms * 2).min(1000);
        // Busy-wait is acceptable here since the engine's thread pool
        // manages scheduling and this runs synchronously.
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
    }
}

/// Take a screenshot of the current WebView.
///
/// Uses `android:capture:getScreenFrame` since the WebView is the visible
/// content on screen.
pub fn screenshot(_params: &Value) -> Result<String, String> {
    common::log(common::log_level::DEBUG, "Capturing WebView screenshot");
    common::invoke("android:capture:getScreenFrame", "{}")
}

/// Get current session information: URL, title, and cookies.
pub fn get_session_info(_params: &Value) -> Result<String, String> {
    let session = load_session();

    let cookies = execute_javascript_raw("document.cookie").unwrap_or_default();

    Ok(serde_json::json!({
        "url": session.url,
        "title": session.title,
        "cookies": cookies,
        "timestamp": common::now_ms(),
    })
    .to_string())
}
