use crate::http::{HttpClient, HttpRequest};
use crate::store::commands::DeviceCommand;
use crate::store::persistence_manager::PersistenceManager;
use anyhow::{Result, anyhow};
use log::{debug, warn};
use std::sync::Arc;

pub use wacore::version::parse_sw_js;

const SW_URL: &str = "https://web.whatsapp.com/sw.js";

/// WhatsApp Web homepage URL — used as a fallback version source.
/// Mirrors whatsmeow's `socket.Origin` constant.
const HOMEPAGE_URL: &str = "https://web.whatsapp.com";

/// Chrome-like user agent for version fetch requests.
const VERSION_FETCH_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

/// Build an HTTP request with browser-like headers for fetching WhatsApp Web pages.
/// Mirrors the header set from whatsmeow's `GetLatestVersion` in update.go.
fn build_version_fetch_request(url: &str) -> HttpRequest {
    HttpRequest::get(url)
        .with_header("user-agent", VERSION_FETCH_UA)
        .with_header("sec-fetch-dest", "document")
        .with_header("sec-fetch-mode", "navigate")
        .with_header("sec-fetch-site", "none")
        .with_header("sec-fetch-user", "?1")
        .with_header(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
        )
        .with_header("accept-language", "en-US,en;q=0.9")
}

/// Fetch the latest WhatsApp Web version from the sw.js service worker script.
///
/// This is the primary version source. Parses `client_revision` from the
/// service worker and returns `(2, 3000, revision)`.
pub async fn fetch_latest_app_version(
    http_client: &Arc<dyn HttpClient>,
) -> Result<(u32, u32, u32)> {
    let request = build_version_fetch_request(SW_URL);
    let response = http_client
        .execute(request)
        .await
        .map_err(|e| anyhow!("HTTP request to {} failed: {}", SW_URL, e))?;

    let body_str = response
        .body_string()
        .map_err(|e| anyhow!("Failed to decode response body: {}", e))?;

    parse_sw_js(&body_str)
        .ok_or_else(|| anyhow!("Could not find 'client_revision' version in sw.js response"))
}

/// Fetch the latest WhatsApp Web version from the homepage.
///
/// Fallback method matching whatsmeow's `GetLatestVersion` from `update.go`.
/// Scrapes `https://web.whatsapp.com` and extracts `"client_revision":(\d+)`
/// from the inline JavaScript/JSON. Returns `(2, 3000, revision)`.
pub async fn fetch_latest_app_version_from_homepage(
    http_client: &Arc<dyn HttpClient>,
) -> Result<(u32, u32, u32)> {
    let request = build_version_fetch_request(HOMEPAGE_URL);
    let response = http_client
        .execute(request)
        .await
        .map_err(|e| anyhow!("HTTP request to {} failed: {}", HOMEPAGE_URL, e))?;

    let body_str = response
        .body_string()
        .map_err(|e| anyhow!("Failed to decode response body: {}", e))?;

    // The homepage HTML contains the same client_revision field as sw.js
    parse_sw_js(&body_str)
        .ok_or_else(|| anyhow!("Could not find 'client_revision' in homepage response"))
}

/// Fetch the latest version, trying sw.js first then falling back to the homepage.
///
/// Combines both approaches for robustness. If the sw.js endpoint fails or
/// doesn't contain the version, falls back to the homepage scraping method
/// that matches whatsmeow's approach.
pub async fn fetch_latest_app_version_with_fallback(
    http_client: &Arc<dyn HttpClient>,
) -> Result<(u32, u32, u32)> {
    match fetch_latest_app_version(http_client).await {
        Ok(version) => Ok(version),
        Err(sw_err) => {
            warn!(
                "sw.js version fetch failed ({}), trying homepage fallback",
                sw_err
            );
            fetch_latest_app_version_from_homepage(http_client)
                .await
                .map_err(|homepage_err| {
                    anyhow!(
                        "Both version fetch methods failed. sw.js: {}; homepage: {}",
                        sw_err,
                        homepage_err
                    )
                })
        }
    }
}

pub async fn resolve_and_update_version(
    persistence_manager: &Arc<PersistenceManager>,
    http_client: &Arc<dyn HttpClient>,
    override_version: Option<(u32, u32, u32)>,
) -> Result<()> {
    if let Some((p, s, t)) = override_version {
        debug!("Using user-provided override version: {}.{}.{}", p, s, t);
        persistence_manager
            .process_command(DeviceCommand::SetAppVersion((p, s, t)))
            .await;
        return Ok(());
    }

    let device = persistence_manager.get_device_snapshot().await;
    let last_fetched_ms = device.app_version_last_fetched_ms;

    let needs_fetch = if last_fetched_ms == 0 {
        true
    } else {
        match chrono::DateTime::from_timestamp_millis(last_fetched_ms) {
            Some(last_fetched_dt) => {
                chrono::Utc::now().signed_duration_since(last_fetched_dt)
                    > chrono::Duration::hours(24)
            }
            None => true,
        }
    };

    if needs_fetch {
        debug!("WhatsApp version is stale or missing, fetching latest...");
        let (p, s, t) = fetch_latest_app_version_with_fallback(http_client)
            .await
            .map_err(|e| anyhow!("Failed to fetch latest WhatsApp version: {}", e))?;
        debug!("Fetched latest version: {}.{}.{}", p, s, t);
        persistence_manager
            .process_command(DeviceCommand::SetAppVersion((p, s, t)))
            .await;
    } else {
        debug!(
            "Using cached version: {}.{}.{}",
            device.app_version_primary, device.app_version_secondary, device.app_version_tertiary
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_fetch_urls() {
        assert_eq!(SW_URL, "https://web.whatsapp.com/sw.js");
        assert_eq!(HOMEPAGE_URL, "https://web.whatsapp.com");
    }

    #[test]
    fn test_build_version_fetch_request_has_browser_headers() {
        let req = build_version_fetch_request("https://example.com");
        // Verify the request includes the whatsmeow-matching headers
        assert!(req.headers.contains_key("user-agent"));
        assert!(req.headers.contains_key("sec-fetch-dest"));
        assert!(req.headers.contains_key("sec-fetch-mode"));
        assert!(req.headers.contains_key("sec-fetch-site"));
        assert!(req.headers.contains_key("sec-fetch-user"));
        assert!(req.headers.contains_key("accept"));
        assert!(req.headers.contains_key("accept-language"));
        assert_eq!(req.headers["sec-fetch-dest"], "document");
        assert_eq!(req.headers["sec-fetch-mode"], "navigate");
        assert_eq!(req.headers["sec-fetch-user"], "?1");
    }

    #[test]
    fn test_parse_sw_js_client_revision_quoted() {
        let s = r#"var x = {"client_revision": "123456"};"#;
        assert_eq!(parse_sw_js(s), Some((2, 3000, 123456)));
    }

    #[test]
    fn test_parse_sw_js_client_revision_unquoted() {
        let s = r#"client_revision:12345;"#;
        assert_eq!(parse_sw_js(s), Some((2, 3000, 12345)));
    }

    #[test]
    fn test_parse_sw_js_assets_fallback() {
        let s = "... assets-manifest-98765 ...";
        assert_eq!(parse_sw_js(s), Some((2, 3000, 0)));
    }

    #[test]
    fn test_parse_sw_js_from_homepage_html() {
        // Simulates the homepage HTML containing the version in inline JS,
        // matching the whatsmeow GetLatestVersion approach.
        let html = r#"<!DOCTYPE html><html><head><script>
            var config = {"client_revision":1234567890,"server_revision":1234567890};
        </script></head><body></body></html>"#;
        assert_eq!(parse_sw_js(html), Some((2, 3000, 1234567890)));
    }

    #[test]
    fn test_parse_sw_js_realistic_sw_js() {
        let s = r#"__DEV__=0;/*FB_PKG_DELIM*/
self.__swData=JSON.parse(/*BTDS*/"{\"dynamic_data\":{\"dynamic_modules\":{\"cr:375\":{\"__rc\":[\"WAWebFtsLightClient\",null]},\"cr:1126\":{\"__rc\":[\"TimeSliceSham\",null]},\"cr:4122\":{\"__rc\":[null,null]},\"cr:4324\":{\"__rc\":[null,null]},\"cr:4533\":{\"__rc\":[null,null]},\"cr:4722\":{\"__rc\":[null,null]},\"cr:4941\":{\"__rc\":[null,null]},\"cr:5151\":{\"__rc\":[null,null]},\"cr:5292\":{\"__rc\":[null,null]},\"cr:5411\":{\"__rc\":[null,null]},\"cr:5664\":{\"__rc\":[null,null]},\"cr:6640\":{\"__rc\":[null,null]},\"cr:8978\":{\"__rc\":[null,null]},\"cr:9565\":{\"__rc\":[null,null]},\"cr:10197\":{\"__rc\":[null,null]},\"cr:10198\":{\"__rc\":[null,null]},\"cr:17160\":{\"__rc\":[null,null]},\"cr:17219\":{\"__rc\":[null,null]},\"cr:21223\":{\"__rc\":[null,null]},\"IntlCurrentLocale\":{\"code\":\"en_US\"},\"WAWebSwResources\":{\"wa_default_notification_icon\":\"https:\\\/\\\/static.whatsapp.net\\\/rsrc.php\\\/v4\\\/yX\\\/r\\\/JYPizEwERE4.png\"},\"SiteData\":{\"server_revision\":1026131876,\"client_revision\":1026131876,\"push_phase\":\"C3\",\"pkg_cohort\":\"BP:DEFAULT\",\"haste_session\":\"20320.BP:DEFAULT.2.0...0\",\"pr\":1,\"manifest_base_uri\":\"https:\\\/\\\/static.whatsapp.net\",\"manifest_origin\":null,\"manifest_version_prefix\":null,\"be_one_ahead\":false,\"is_rtl\":false,\"is_experimental_tier\":false,\"is_jit_warmed_up\":true,\"hsi\":\"7540800780599698108\",\"semr_host_bucket\":\"3\",\"bl_hash_version\":2,\"comet_env\":0,\"wbloks_env\":false,\"ef_page\":null,\"compose_bootloads\":false,\"spin\":4,\"__spin_r\":1026131876,\"__spin_b\":\"trunk\",\"__spin_t\":1755729499,\"vip\":\"2a03:2880:f205:c5:face:b00c:0:167\"}},\"hsdp\":{\"bxData\":{\"32186\":{\"uri\":\"https:\\\/\\\/static.whatsapp.net\\\/rsrc.php\\\/v4\\\/yR\\\/r\\\/aCneqBxOSs-.png\"},\"32187\":{\"uri\":\"https:\\\/\\\/static.whatsapp.net\\\/rsrc.php\\\/v4\\\/yT\\\/r\\\/s0hoT-Vu8xP.png\"}},\"gkxData\":{\"4112\":{\"result\":false,\"hash\":null},\"5943\":{\"result\":false,\"hash\":null},\"7685\":{\"result\":false,\"hash\":null},\"10314\":{\"result\":false,\"hash\":null},\"16915\":{\"result\":false,\"hash\":null},\"16928\":{\"result\":false,\"hash\":null},\"17038\":{\"result\":false,\"hash\":null},\"26256\":{\"result\":false,\"hash\":null},\"26258\":{\"result\":true,\"hash\":null},\"26259\":{\"result\":false,\"hash\":null}},\"justknobxData\":{\"371\":{\"r\":true},\"1050\":{\"r\":false},\"1617\":{\"r\":165},\"1618\":{\"r\":8},\"1619\":{\"r\":1},\"1620\":{\"r\":2},\"1621\":{\"r\":4},\"1622\":{\"r\":0},\"1623\":{\"r\":6},\"1624\":{\"r\":1},\"1662\":{\"r\":2},\"1663\":{\"r\":14},\"1664\":{\"r\":2},\"1854\":{\"r\":false},\"2237\":{\"r\":false},\"2337\":{\"r\":false},\"2517\":{\"r\":true},\"3717\":{\"r\":1},\"4952\":{\"r\":true}}}}}");

      if (self.trustedTypes && self.trustedTypes.createPolicy) {
        const escapeScriptURLPolicy = self.trustedTypes.createPolicy("workerPolicy", {
          createScriptURL: url => url
        });
        importScripts(escapeScriptURLPolicy.createScriptURL("https:\/\/static.whatsapp.net\/rsrc.php\/v4\/yq\/r\/odrxy-7zVX8.js"));
      } else {
         importScripts("https:\/\/static.whatsapp.net\/rsrc.php\/v4\/yq\/r\/odrxy-7zVX8.js");
      }"#;

        assert_eq!(parse_sw_js(s), Some((2, 3000, 1026131876)));
    }
}
