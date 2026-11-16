//! Small opt-in WebView2/Chromium network blocking experiment.
//! This is not EasyList/uBlock and does not cover all frames/service workers.
//! Rules are deliberately restricted to known advertising hosts. No telemetry.

pub const NETWORK_PATTERNS: &[&str] = &[
    "*://*.doubleclick.net/*",
    "*://doubleclick.net/*",
    "*://*.googlesyndication.com/*",
    "*://googlesyndication.com/*",
    "*://*.googleadservices.com/*",
    "*://googleadservices.com/*",
    "*://*.adnxs.com/*",
    "*://adnxs.com/*",
    "*://*.taboola.com/*",
    "*://*.outbrain.com/*",
    "*://*.criteo.com/*",
    "*://*.criteo.net/*",
    "*://*.pubmatic.com/*",
    "*://*.rubiconproject.com/*",
    "*://*.openx.net/*",
    "*://*.scorecardresearch.com/*",
];

pub fn blocked_urls_payload(enabled: bool) -> String {
    let patterns: &[&str] = if enabled { NETWORK_PATTERNS } else { &[] };
    // The `urls` field is deprecated but supported by Chrome DevTools Protocol.
    // It avoids differing urlPatterns behavior in older WebView2 runtimes.
    serde_json::json!({ "urls": patterns }).to_string()
}

#[cfg(all(windows, feature = "desktop"))]
fn send_cdp(
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    command: &'static str,
    arguments: &str,
) -> Result<(), String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::PCWSTR;
    let name: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    let args: Vec<u16> = arguments.encode_utf16().chain(std::iter::once(0)).collect();
    let done =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |status, _response| {
            if status.is_err() {
                eprintln!("LiteRust CDP command {command} failed: {status:?}");
            }
            Ok(())
        }));
    // Windows COM copies the parameters during this call, while the completion
    // callback is retained by WebView2 for the asynchronous result.
    unsafe {
        webview
            .CallDevToolsProtocolMethod(PCWSTR(name.as_ptr()), PCWSTR(args.as_ptr()), &done)
            .map_err(|error| format!("{command}: {error}"))
    }
}

/// WebView2 network-level blocking, not CSS hiding. Per-WebView and opt-in.
/// The CDP calls complete asynchronously; errors are logged, not silently claimed successful.
#[cfg(all(windows, feature = "desktop"))]
pub fn apply(webview: &wry::WebView, enabled: bool) -> Result<(), String> {
    use wry::WebViewExtWindows;
    let core = webview.webview();
    if enabled {
        send_cdp(&core, "Network.enable", "{}")?;
    }
    send_cdp(
        &core,
        "Network.setBlockedURLs",
        &blocked_urls_payload(enabled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabled_has_valid_host_patterns() {
        let value: serde_json::Value = serde_json::from_str(&blocked_urls_payload(true)).unwrap();
        assert_eq!(
            value["urls"].as_array().unwrap().len(),
            NETWORK_PATTERNS.len()
        );
        assert!(NETWORK_PATTERNS.len() >= 12);
        for pattern in NETWORK_PATTERNS {
            assert!(pattern.starts_with("*://"));
            assert!(pattern.ends_with("/*"));
            assert!(!pattern.contains("passwords.google.com"));
        }
    }
    #[test]
    fn disabled_clears_all_blocked_urls() {
        assert_eq!(blocked_urls_payload(false), "{\"urls\":[]}");
    }
}
