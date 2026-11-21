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

    pub fn search(self, input: &str, dark: Option<bool>) -> String {
        let query: String = url::form_urlencoded::byte_serialize(input.as_bytes()).collect();
        let mut address = format!("{}{query}", self.prefix());
        if let (Self::Duckduckgo, Some(dark)) = (self, dark) {
            address.push_str(if dark { "&kae=d" } else { "&kae=-1" });
        }
        address
    }

    pub fn query_from(self, url: &Url) -> Option<String> {
        let (host, path, key) = match self {
            Self::Google => ("www.google.com", "/search", "q"),
            Self::Yandex => ("yandex.ru", "/search/", "text"),
            Self::Duckduckgo => ("duckduckgo.com", "/", "q"),
            Self::Bing => ("www.bing.com", "/search", "q"),
            Self::Brave => ("search.brave.com", "/search", "q"),
        };
        if url.scheme() != "https"
            || url.host_str() != Some(host)
            || url.path() != path
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        url.query_pairs()
            .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
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
    typed_address(input).unwrap_or_else(|| engine.search(input, dark))
}

pub fn typed_address(input: &str) -> Option<String> {
    let input = input.trim();
    if valid_web_url(input) {
        return Url::parse(input).ok().map(|url| url.to_string());
    }
    if input.contains("://") || input.chars().any(char::is_whitespace) {
        return None;
    }
    let authority = input.split(['/', '?', '#']).next()?;
    let host = authority.split(':').next()?;
    if !(host.contains('.') || host.eq_ignore_ascii_case("localhost") || authority.starts_with('['))
    {
        return None;
    }
    let scheme = if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        "http"
    } else {
        "https"
    };
    let candidate = format!("{scheme}://{input}");
    valid_web_url(&candidate).then(|| Url::parse(&candidate).expect("validated").to_string())
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
    let history = data
        .history
        .iter()
        .rev()
        .filter(|_| data.browser_settings.history_enabled)
        .map(|entry| Suggestion {
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
    fn addresses_ports_and_search() {
        assert_eq!(destination("example.org:8443"), "https://example.org:8443/");
        assert_eq!(destination("localhost:8080"), "http://localhost:8080/");
        assert_eq!(
            destination("localhost:8080/notes?tag=work"),
            "http://localhost:8080/notes?tag=work"
        );
        assert_eq!(
            destination("example.org:8443/path#faq"),
            "https://example.org:8443/path#faq"
        );
        assert_eq!(destination("[::1]:8080"), "https://[::1]:8080/");
        assert_eq!(destination("пример.рф"), "https://xn--e1afmkfd.xn--p1ai/");
        assert_eq!(
            destination("как дела"),
            "https://www.google.com/search?q=%D0%BA%D0%B0%D0%BA+%D0%B4%D0%B5%D0%BB%D0%B0"
        );
    }
}
