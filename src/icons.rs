use base64::{engine::general_purpose::STANDARD, Engine};
pub const ACTIONS: &[&str] = &[
    "fullscreen",
    "copy",
    "print",
    "private",
    "app",
    "back",
    "forward",
    "reload",
    "home",
    "bookmark",
    "bookmarks",
    "history",
    "downloads",
    "key",
    "more",
    "settings",
    "appearance",
    "shield",
    "globe",
    "lock",
    "info",
    "warning",
    "close",
    "plus",
    "power",
    "chevron",
    "user",
    "search",
];
pub fn valid_target(target: &str) -> bool {
    ACTIONS.contains(&target)
        || target
            .strip_prefix("site:")
            .is_some_and(crate::storage::valid_web_url)
}
pub fn png_bytes(value: &str) -> Option<Vec<u8>> {
    if value.len() > 40_000 {
        return None;
    }
    let bytes = STANDARD
        .decode(value.strip_prefix("data:image/png;base64,")?)
        .ok()?;
    (bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes.len() > 24
        && bytes[16..20] == 64u32.to_be_bytes()
        && bytes[20..24] == 64u32.to_be_bytes())
    .then_some(bytes)
}
pub fn ico(value: &str) -> Option<Vec<u8>> {
    let bytes = png_bytes(value)?;
    let mut out = vec![0, 0, 1, 0, 1, 0, 64, 64, 0, 0, 1, 0, 32, 0];
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    out.extend(bytes);
    Some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_and_active_content() {
        assert!(valid_target("home"));
        assert!(valid_target("site:https://example.org/"));
        assert!(!valid_target("../file"));
        assert!(!valid_target("site:javascript:alert(1)"));
        assert!(png_bytes("data:image/svg+xml,<svg/>").is_none());
        assert!(ico("data:image/png;base64,AAAA").is_none());
    }
}
