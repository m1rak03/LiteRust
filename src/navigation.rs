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
    format!(
        "{}{}",
        engine.prefix(),
        serializer.finish().strip_prefix("q=").unwrap_or_default()
    )
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
