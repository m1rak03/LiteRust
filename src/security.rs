//! Simple URL hygiene, not a Google Safe Browsing or anti-phishing database.
use url::Url;

pub fn safe_top_level_navigation(input: &str) -> bool {
    if input.len() > 8192 || input.chars().any(char::is_control) {
        return false;
    }
    if input == "about:blank" {
        return true;
    }
    let Ok(url) = Url::parse(input) else {
        return false;
    };
    if !matches!(url.scheme(), "https" | "http") {
        return false;
    }
    if url.host_str().is_none() {
        return false;
    }
    // Username/password within a URL makes a seemingly trusted domain misleading:
    // https://accounts.google.com@phishing.example/ actually goes to phishing.example.
    url.username().is_empty() && url.password().is_none()
}

pub fn hostname_warning(input: &str) -> Option<&'static str> {
    let url = Url::parse(input).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let hostname = url.host_str()?.to_ascii_lowercase();
    if hostname.split('.').any(|label| label.starts_with("xn--")) {
        return Some("Домен в Punycode (xn--): проверьте адрес перед вводом пароля. Это не означает, что сайт вредоносный.");
    }
    if url.scheme() == "http" && hostname != "localhost" && hostname != "127.0.0.1" {
        return Some("Незащищённое соединение HTTP. Не вводите пароли и платёжные данные.");
    }
    None
}

/// Reject unsafe address-bar input instead of sending credentials to a search engine.
pub fn reject_address_input(input: &str) -> bool {
    let input = input.trim();
    if input.len() > 8192 || input.chars().any(char::is_control) {
        return true;
    }
    if let Ok(url) = Url::parse(input) {
        if matches!(url.scheme(), "javascript" | "data" | "file" | "vbscript") {
            return true;
        }
        if matches!(url.scheme(), "http" | "https") && !safe_top_level_navigation(input) {
            return true;
        }
    }
    if !input.chars().any(char::is_whitespace) && input.contains('@') {
        return Url::parse(&format!("https://{input}")).is_ok_and(|u| !u.username().is_empty());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_input_never_sends_credentials_to_search() {
        for input in [
            "https://google.com@evil.example/",
            "name:secret@example.org",
            "file:///C:/x",
            "javascript:alert(1)",
            "https://exam\nple.org",
        ] {
            assert!(reject_address_input(input), "{input}");
        }
        assert!(reject_address_input(&"x".repeat(8193)));
        assert!(!reject_address_input("пример.рф"));
        assert!(!reject_address_input("обычный поиск"));
    }
    #[test]
    fn prevents_url_userinfo_deception() {
        assert!(!safe_top_level_navigation(
            "https://google.com@evil.example/path"
        ));
        assert!(!safe_top_level_navigation(
            "https://name:password@site.example/"
        ));
        assert!(!safe_top_level_navigation("javascript:alert(1)"));
        assert!(!safe_top_level_navigation("file:///C:/passwords.txt"));
        assert!(safe_top_level_navigation("https://accounts.google.com/"));
    }
    #[test]
    fn warns_about_punycode_but_does_not_block_international_domains() {
        assert!(hostname_warning("https://xn--e1afmkfd.xn--p1ai").is_some());
        assert!(safe_top_level_navigation("https://xn--e1afmkfd.xn--p1ai/"));
        assert!(hostname_warning("http://example.org").is_some());
        assert!(hostname_warning("https://example.org").is_none());
    }
}

/// Only call with compile-time control templates, before inserting user data.
/// Random nonces allow our scripts but block inline handlers and injected scripts.
pub fn control_document(template: &str) -> std::io::Result<String> {
    use base64::Engine;
    let mut random = [0u8; 24];
    getrandom::getrandom(&mut random)
        .map_err(|_| std::io::Error::other("Secure random unavailable"))?;
    let nonce = base64::engine::general_purpose::STANDARD.encode(random);
    Ok(template
        .replace(
            "script-src 'unsafe-inline'",
            &format!("script-src 'nonce-{nonce}'"),
        )
        .replace("<script>", &format!("<script nonce=\"{nonce}\">")))
}

#[cfg(test)]
mod control_tests {
    #[test]
    fn controls_use_fresh_nonces_and_no_inline_script_permission() {
        let html = include_str!("../ui/toolbar.html");
        let first = super::control_document(html).unwrap();
        assert!(!first.contains("script-src 'unsafe-inline'"));
        assert!(!first.contains("<script>"));
        assert!(first.contains("<script nonce="));
        assert_ne!(first, super::control_document(html).unwrap());
    }
}

/// Resolve Windows utilities without searching the current directory, PATH or
/// caller-controlled environment variables. Callers only pass fixed paths.
#[cfg(windows)]
pub fn windows_system_binary(relative: &str) -> std::io::Result<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut buffer = vec![0u16; 32768];
    let size = unsafe {
        windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut buffer))
    } as usize;
    if size == 0 || size >= buffer.len() {
        return Err(std::io::Error::other(
            "Windows system directory unavailable",
        ));
    }
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..size])).join(relative))
}
