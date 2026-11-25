//! Official Google account destinations.
pub const OFFICIAL_MANAGER_URL: &str = "https://passwords.google.com/";

pub const OFFICIAL_ACCOUNT_URL: &str = "https://myaccount.google.com/";
pub fn open_official_account() -> Result<(), std::io::Error> {
    open_url(OFFICIAL_ACCOUNT_URL)
}

fn open_url(url: &str) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    let mut command = {
        let mut command =
            std::process::Command::new(crate::security::windows_system_binary("rundll32.exe")?);
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_is_googles_https_host_without_embedded_credentials() {
        let url = url::Url::parse(OFFICIAL_MANAGER_URL).unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("passwords.google.com"));
        assert_eq!(url.path(), "/");
        assert!(url.username().is_empty());
        assert!(url.password().is_none());
        assert!(url.query().is_none());
    }
}
