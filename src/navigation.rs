use crate::storage::{valid_web_url, NEW_TAB};
use url::Url;

/// Only HTTP(S) addresses may be typed/opened. All other input is a search.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchEngine {
    #[default]
    Google,
    Yandex,
    Duckduckgo,
    Bing,
    Brave,
}
impl SearchEngine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Yandex => "Яндекс",
            Self::Duckduckgo => "DuckDuckGo",
            Self::Bing => "Bing",
            Self::Brave => "Brave",
        }
    }
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Google => "https://www.google.com/search?q=",
            Self::Yandex => "https://yandex.ru/search/?text=",
            Self::Duckduckgo => "https://duckduckgo.com/?q=",
            Self::Bing => "https://www.bing.com/search?q=",
            Self::Brave => "https://search.brave.com/search?q=",
        }
    }
}
pub fn destination(input: &str) -> String {
    destination_with_engine(input, SearchEngine::default())
}
pub fn destination_with_engine(input: &str, engine: SearchEngine) -> String {
    destination_for(input, engine, None)
}

fn destination_for(input: &str, engine: SearchEngine, dark: Option<bool>) -> String {
    let input = input.trim();
    if input.is_empty() {
        return NEW_TAB.to_owned();
    }
    if valid_web_url(input) {
        return Url::parse(input).expect("validated").to_string();
    }
    let unsupported_absolute = Url::parse(input)
        .is_ok_and(|url| !matches!(url.scheme(), "http" | "https"))
        && !input.starts_with("localhost:");
    let numeric_port = input.rsplit_once(':').is_some_and(|(host, port)| {
        (host.contains('.') || host == "localhost" || host.starts_with('['))
            && port.parse::<u16>().is_ok()
    });
    let looks_like_host = (!unsupported_absolute || numeric_port)
        && !input.chars().any(char::is_whitespace)
        && !input.contains("://")
        && (input.contains('.')
            || input.eq_ignore_ascii_case("localhost")
            || input.starts_with("localhost:")
            || input.starts_with('['));
    if looks_like_host {
        let scheme = if input.starts_with("localhost") || input.starts_with("127.0.0.1") {
            "http"
        } else {
            "https"
        };
        let candidate = format!("{scheme}://{input}");
        if valid_web_url(&candidate) {
            return Url::parse(&candidate).expect("validated").to_string();
        }
    }
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("q", input);
    let mut destination = format!(
        "{}{}",
        engine.prefix(),
        serializer.finish().strip_prefix("q=").unwrap_or_default()
    );
    if let (SearchEngine::Duckduckgo, Some(dark)) = (engine, dark) {
        destination.push_str(if dark { "&kae=d" } else { "&kae=-1" });
    }
    destination
}

pub fn destination_with_theme(input: &str, engine: SearchEngine, dark: bool) -> String {
    destination_for(input, engine, Some(dark))
}

#[derive(serde::Serialize)]
pub struct Suggestion<'a> {
    pub title: &'a str,
    pub url: &'a str,
}

pub fn suggestion_candidates(
    data: &crate::storage::BrowserData,
) -> impl Iterator<Item = Suggestion<'_>> {
    let history = data.history.iter().rev().map(|entry| Suggestion {
        title: &entry.title,
        url: &entry.url,
    });
    let bookmarks = data.bookmarks.iter().rev().map(|entry| Suggestion {
        title: &entry.title,
        url: &entry.url,
    });
    bookmarks
        .chain(history)
        .filter(|entry| crate::security::safe_top_level_navigation(entry.url))
}

pub fn suggestions<'a>(data: &'a crate::storage::BrowserData, query: &str) -> Vec<Suggestion<'a>> {
    let query = query.trim();
    if query.chars().count() < 2 || query.len() > 512 {
        return Vec::new();
    }
    let query = query.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    suggestion_candidates(data)
        .filter(|entry| {
            entry.title.to_lowercase().contains(&query) || entry.url.to_lowercase().contains(&query)
        })
        .filter(|entry| seen.insert(entry.url))
        .take(6)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn providers_encode_queries_without_changing_addresses() {
        for engine in [
            SearchEngine::Google,
            SearchEngine::Yandex,
            SearchEngine::Duckduckgo,
            SearchEngine::Bing,
            SearchEngine::Brave,
        ] {
            let query = destination_with_engine("rust & браузер", engine);
            let parsed = Url::parse(&query).unwrap();
            assert_eq!(parsed.scheme(), "https");
            assert_eq!(parsed.query_pairs().next().unwrap().1, "rust & браузер");
            assert!(query.starts_with(engine.prefix()));
            assert_eq!(
                destination_with_engine("example.org", engine),
                "https://example.org/"
            );
            let restored: SearchEngine =
                serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
            assert_eq!(restored.prefix(), engine.prefix());
        }
    }
    #[test]
    fn theme_changes_searches_only() {
        assert_eq!(
            destination_with_theme("rust browser", SearchEngine::Duckduckgo, true),
            "https://duckduckgo.com/?q=rust+browser&kae=d"
        );
        assert_eq!(
            destination_with_theme("duckduckgo.com/?q=rust", SearchEngine::Duckduckgo, true),
            "https://duckduckgo.com/?q=rust"
        );
    }

    #[test]
    fn suggestions_match_case_insensitively_and_deduplicate() {
        let mut data = crate::storage::BrowserData::default();
        let url = "https://example.org/";
        data.toggle_bookmark(url, "Русский сайт");
        data.record_visit(url, "Русский сайт");
        assert_eq!(suggestions(&data, "РУССКИЙ").len(), 1);
        assert_eq!(suggestions(&data, "EXAMPLE")[0].url, url);
        assert!(suggestions(&data, "e").is_empty());
    }

    #[test]
    fn addresses_ports_and_search() {
        assert_eq!(destination("example.org:8443"), "https://example.org:8443/");
        assert_eq!(destination("localhost:8080"), "http://localhost:8080/");
        assert_eq!(destination("[::1]:8080"), "https://[::1]:8080/");
        assert_eq!(destination("пример.рф"), "https://xn--e1afmkfd.xn--p1ai/");
        assert_eq!(
            destination("как дела"),
            "https://www.google.com/search?q=%D0%BA%D0%B0%D0%BA+%D0%B4%D0%B5%D0%BB%D0%B0"
        );
    }
}
