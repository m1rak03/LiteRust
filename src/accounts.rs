//! Only official account pages; no inference from cookies or navigation.
pub fn url(provider: &str) -> Option<&'static str> {
    match provider {
        "google" => Some("https://accounts.google.com/AccountChooser?continue=https%3A%2F%2Fmyaccount.google.com%2F"),
        "yandex" => Some("https://passport.yandex.ru/auth?retpath=https%3A%2F%2Fid.yandex.ru%2F"),
        "microsoft" => Some("https://account.microsoft.com/"),
        "yahoo" => Some("https://login.yahoo.com/"),
        "opera" => Some("https://auth.opera.com/account/login"),
        "firefox" => Some("https://accounts.firefox.com/"),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn fixed_providers_only() {
        for name in ["google", "yandex", "microsoft", "yahoo", "opera", "firefox"] {
            assert!(crate::security::safe_top_level_navigation(
                super::url(name).unwrap()
            ));
        }
        assert!(super::url("https://evil.example").is_none());
    }
}
