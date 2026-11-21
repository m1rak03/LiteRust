//! Small, explicitly invoked tools for the active page.

use url::Url;

const SELECTED_TEXT_BYTES: usize = 900;
pub const TOOL_KEYS: [&str; 9] = [
    "notes",
    "calculator",
    "print",
    "find",
    "downloads",
    "history",
    "bookmarks",
    "translate",
    "save",
];

pub fn hidden_tools(keys: Vec<String>) -> Vec<String> {
    TOOL_KEYS
        .into_iter()
        .filter(|key| keys.iter().any(|value| value == key))
        .map(str::to_owned)
        .collect()
}

pub fn selected_text(json: &str) -> Option<String> {
    if json.len() > 6000 {
        return None;
    }
    let text: String = serde_json::from_str(json).ok()?;
    let text = text.trim();
    let mut end = text.len().min(SELECTED_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let result = text[..end].trim_end();
    (!result.is_empty()).then(|| result.to_owned())
}

pub fn translation_address(text: &str, language: crate::storage::Language) -> Option<String> {
    if text.trim().is_empty() || text.len() > SELECTED_TEXT_BYTES {
        return None;
    }
    let mut url = Url::parse("https://translate.google.com/").ok()?;
    url.query_pairs_mut()
        .append_pair("sl", "auto")
        .append_pair("tl", language.pick("ru", "en"))
        .append_pair("text", text)
        .append_pair("op", "translate");
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Language;

    #[test]
    fn selection_is_bounded_at_utf8_boundary() {
        assert!(selected_text("null").is_none());
        assert!(selected_text(r#"" ""#).is_none());
        let text = "🚀".repeat(260);
        let result = selected_text(&serde_json::to_string(&text).unwrap()).unwrap();
        assert_eq!(result.len(), SELECTED_TEXT_BYTES);
        assert_eq!(result.chars().count(), 225);
    }

    #[test]
    fn translation_uses_only_the_selected_text_on_the_known_service() {
        let text = "Texto & привет? # test";
        let address = translation_address(text, Language::En).unwrap();
        let url = Url::parse(&address).unwrap();
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://translate.google.com"
        );
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["text"], text);
        assert_eq!(pairs["tl"], "en");
    }
}
