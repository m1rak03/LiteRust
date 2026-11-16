//! Import user-exported Chrome/Chromium/Firefox Netscape bookmark HTML.
//! This parses HTML as data; it does not run HTML or JavaScript.
use crate::storage::{compact_title, valid_web_url, Bookmark, BrowserData, MAX_BOOKMARKS};
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

// Keep IPC and DOM parsing bounded. Large bookmark collections can be split into files.
pub const MAX_IMPORT_BYTES: usize = 1_500_000;

#[derive(Default, Debug, PartialEq, Eq)]
pub struct ImportStats {
    pub added: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub skipped_capacity: usize,
}

pub fn import_bookmarks(data: &mut BrowserData, html: &str) -> Result<ImportStats, &'static str> {
    if html.len() > MAX_IMPORT_BYTES {
        return Err("Файл слишком большой (максимум 1,5 МБ).");
    }
    if html.trim().is_empty() {
        return Err("Файл пуст.");
    }
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").expect("constant CSS selector");
    if document.select(&selector).next().is_none() {
        return Err("В выбранном HTML-файле не найдены закладки.");
    }
    let current = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    let mut existing: HashSet<String> = data
        .bookmarks
        .iter()
        .filter_map(|b| Url::parse(&b.url).ok().map(|url| url.to_string()))
        .collect();
    let mut stats = ImportStats::default();
    for node in document.select(&selector) {
        let href = node.value().attr("href").unwrap_or_default().trim();
        if !valid_web_url(href) {
            stats.rejected += 1;
            continue;
        }
        let normalized = match Url::parse(href) {
            Ok(url) => url.to_string(),
            Err(_) => {
                stats.rejected += 1;
                continue;
            }
        };
        // Normalize URL spelling to avoid importing a duplicate without a trailing slash.
        if !existing.insert(normalized.clone()) {
            stats.duplicates += 1;
            continue;
        }
        if data.bookmarks.len() >= MAX_BOOKMARKS {
            stats.skipped_capacity += 1;
            continue;
        }
        let label = node.text().collect::<Vec<_>>().join(" ");
        let title = compact_title(label.trim());
        data.bookmarks.push(Bookmark {
            url: normalized,
            title: if title.is_empty() { href.into() } else { title },
            added_at: current,
        });
        stats.added += 1;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_chrome_html_decodes_entities_and_prevents_duplicates() {
        let sample = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
          <DL><DT><A HREF="https://www.rust-lang.org/" ADD_DATE="44">Rust &amp; Stuff</A>
          <DT><A HREF="https://example.org/path?a=1&amp;b=2">Example</A>
          <DT><A HREF="javascript:alert(1)">Unsafe</A>
          <DT><A HREF="file:///C:/secrets.txt">Unsafe file</A>
          <DT><A HREF="https://www.rust-lang.org/">Repeated</A></DL>"#;
        let mut browser = BrowserData::default();
        let stats = import_bookmarks(&mut browser, sample).unwrap();
        assert_eq!(
            stats,
            ImportStats {
                added: 2,
                duplicates: 1,
                rejected: 2,
                skipped_capacity: 0
            }
        );
        assert_eq!(browser.bookmarks[0].title, "Rust & Stuff");
        assert_eq!(browser.bookmarks[1].url, "https://example.org/path?a=1&b=2");
        assert_eq!(import_bookmarks(&mut browser, sample).unwrap().added, 0);
    }
    #[test]
    fn limit_input_and_capacity_without_discarding_existing() {
        let mut browser = BrowserData::default();
        for number in 0..MAX_BOOKMARKS {
            browser.toggle_bookmark(&format!("https://example.com/{number}"), "test");
        }
        let stats =
            import_bookmarks(&mut browser, r#"<a href="https://new.example/">New</a>"#).unwrap();
        assert_eq!(stats.skipped_capacity, 1);
        assert_eq!(browser.bookmarks.len(), MAX_BOOKMARKS);
        assert!(import_bookmarks(&mut browser, &"x".repeat(MAX_IMPORT_BYTES + 1)).is_err());
    }
}
