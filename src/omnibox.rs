//! Local address-bar ranking. Suggestions never issue network requests.

use std::{
    cmp::Reverse,
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    home::Dashboard,
    navigation::typed_address,
    security::reject_address_input,
    storage::{compact_title, valid_web_url, BrowserData, Language},
};

const LIMIT: usize = 6;
const DAY: u64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Visit { url: String },
    Tab { id: u64 },
    Search { query: String },
    Command { command: Command },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Settings,
    Downloads,
    History,
    NewTab,
    Profiles,
}

impl Command {
    const ALL: [Self; 5] = [
        Self::Settings,
        Self::Downloads,
        Self::History,
        Self::NewTab,
        Self::Profiles,
    ];

    fn names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Settings => (
                "Открыть настройки",
                "Open settings",
                "settings preferences настройки параметры",
            ),
            Self::Downloads => (
                "Открыть загрузки",
                "Open downloads",
                "downloads загрузки скачивания",
            ),
            Self::History => (
                "Открыть историю",
                "Open history",
                "history история посещений",
            ),
            Self::NewTab => (
                "Создать вкладку",
                "New tab",
                "new tab вкладка создать вкладку",
            ),
            Self::Profiles => (
                "Переключить профиль",
                "Switch profile",
                "switch profile profiles аккаунт профили профиль",
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Match {
    pub title: String,
    pub detail: String,
    pub label: String,
    pub completion: String,
    pub action: Action,
}

#[derive(Clone, Copy)]
enum Source {
    Tab(u64),
    Bookmark,
    Pin,
    History(u64),
}

impl Source {
    fn label(self, language: Language) -> &'static str {
        match self {
            Self::Tab(_) => language.pick("Вкладка", "Tab"),
            Self::Bookmark => language.pick("Закладка", "Bookmark"),
            Self::Pin => language.pick("Быстрый сайт", "Quick site"),
            Self::History(_) => language.pick("История", "History"),
        }
    }

    fn bonus(self) -> i32 {
        match self {
            Self::Tab(_) => 32,
            Self::Bookmark => 52,
            Self::Pin => 64,
            Self::History(_) => 0,
        }
    }
}

struct Site {
    title: String,
    source: Source,
    match_score: i32,
    bonus: i32,
    tab: Option<u64>,
    visits: u32,
    last: u64,
}

struct Query<'a> {
    folded: &'a str,
    address: Option<&'a str>,
}

fn text_score(needle: &str, value: &str) -> i32 {
    let value = value.to_lowercase();
    if needle == value {
        225
    } else if value.starts_with(needle) {
        170
    } else if value
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(needle))
    {
        125
    } else if value.contains(needle) {
        94
    } else if needle.contains(' ') && needle.split_whitespace().all(|word| value.contains(word)) {
        64
    } else {
        0
    }
}

fn add_site(
    sites: &mut BTreeMap<String, Site>,
    query: &Query<'_>,
    address: &str,
    title: &str,
    source: Source,
) {
    if !valid_web_url(address) {
        return;
    }
    let Ok(url) = url::Url::parse(address) else {
        return;
    };
    let url = url.to_string();
    let score = if query.address == Some(url.as_str()) {
        1200
    } else {
        text_score(query.folded, title)
            .max(text_score(query.folded, &url))
            .max(text_score(
                query.folded,
                url.split_once("://").map_or(&url, |(_, rest)| rest),
            ))
    };
    let label = if title.is_empty() { address } else { title };
    let site = sites.entry(url).or_insert_with(|| Site {
        title: compact_title(label),
        source,
        match_score: score,
        bonus: source.bonus(),
        tab: None,
        visits: 0,
        last: 0,
    });
    if score + source.bonus() > site.match_score + site.source.bonus() {
        site.title = compact_title(label);
        site.source = source;
    }
    site.match_score = site.match_score.max(score);
    site.bonus = site.bonus.max(source.bonus());
    match source {
        Source::Tab(id) => {
            site.tab.get_or_insert(id);
        }
        Source::History(last) => {
            site.visits += 1;
            site.last = site.last.max(last);
        }
        _ => {}
    }
}

pub fn suggestions(
    data: &BrowserData,
    input: &str,
    language: Language,
    private_mode: bool,
) -> Vec<Match> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    ranked(data, input, language, private_mode, now)
}

fn ranked(
    data: &BrowserData,
    input: &str,
    language: Language,
    private_mode: bool,
    now: u64,
) -> Vec<Match> {
    let raw = input.trim();
    if raw.is_empty() || raw.len() > 512 || reject_address_input(raw) {
        return Vec::new();
    }
    let command_mode = raw.starts_with('>');
    let folded = raw.strip_prefix('>').unwrap_or(raw).trim().to_lowercase();
    let mut matches: Vec<(i32, u64, Match)> = Vec::new();
    for command in Command::ALL {
        let (ru, en, aliases) = command.names();
        let label = language.pick(ru, en);
        let score = if folded.is_empty() {
            1
        } else {
            text_score(&folded, ru)
                .max(text_score(&folded, en))
                .max(text_score(&folded, aliases))
        };
        if score > 0 {
            matches.push((
                score + 22,
                0,
                Match {
                    title: label.into(),
                    detail: String::new(),
                    label: language.pick("Команда", "Command").into(),
                    completion: format!("> {label}"),
                    action: Action::Command { command },
                },
            ));
        }
    }
    let mut fallback = None;
    if !command_mode {
        let address = typed_address(raw);
        let query = Query {
            folded: &folded,
            address: address.as_deref(),
        };
        let mut sites = BTreeMap::new();
        for tab in &data.tabs {
            add_site(
                &mut sites,
                &query,
                &tab.url,
                &tab.title,
                Source::Tab(tab.id),
            );
        }
        if !private_mode {
            for bookmark in &data.bookmarks {
                add_site(
                    &mut sites,
                    &query,
                    &bookmark.url,
                    &bookmark.title,
                    Source::Bookmark,
                );
            }
        }
        let default_home;
        let home = if let Some(home) = data.active_home() {
            home
        } else {
            default_home = Dashboard::from_legacy(data);
            &default_home
        };
        for pin in &home.tiles {
            add_site(&mut sites, &query, &pin.url, &pin.title, Source::Pin);
        }
        let mut past_queries: BTreeMap<String, (String, u32, u64)> = BTreeMap::new();
        if !private_mode && data.browser_settings.history_enabled {
            for item in data.history.iter().rev() {
                let prior = url::Url::parse(&item.url)
                    .ok()
                    .and_then(|url| data.search_engine.query_from(&url))
                    .filter(|text| {
                        !text.is_empty() && text.len() <= 512 && !reject_address_input(text)
                    });
                if let Some(text) = prior {
                    let key = text.to_lowercase();
                    if key != folded && text_score(&folded, &text) > 0 {
                        let record = past_queries.entry(key).or_insert((text, 0, 0));
                        record.1 += 1;
                        record.2 = record.2.max(item.visited_at);
                    }
                } else {
                    add_site(
                        &mut sites,
                        &query,
                        &item.url,
                        &item.title,
                        Source::History(item.visited_at),
                    );
                }
            }
        }
        let mut exact = false;
        for (url, site) in sites {
            if site.match_score == 0 {
                continue;
            }
            exact |= query.address == Some(url.as_str());
            let source = site.tab.map_or(site.source, Source::Tab);
            let action = site.tab.map_or_else(
                || Action::Visit { url: url.clone() },
                |id| Action::Tab { id },
            );
            let score = site.match_score + site.bonus + frecency(site.visits, site.last, now);
            matches.push((
                score,
                site.last,
                Match {
                    title: site.title,
                    detail: url.clone(),
                    label: source.label(language).into(),
                    completion: url,
                    action,
                },
            ));
        }
        let engine = crate::i18n::template(data.search_engine.name(), language);
        for (_, (text, visits, last)) in past_queries {
            let prefix_bonus = if text.to_lowercase().starts_with(&folded) {
                65
            } else {
                0
            };
            let score = text_score(&folded, &text) + frecency(visits, last, now) + prefix_bonus;
            matches.push((score, last, search_match(text, &engine, language)));
        }
        if let Some(url) = address {
            if !exact {
                matches.push((
                    1200,
                    0,
                    Match {
                        title: url.clone(),
                        detail: String::new(),
                        label: language.pick("Адрес", "Address").into(),
                        completion: url.clone(),
                        action: Action::Visit { url },
                    },
                ));
            }
        } else {
            fallback = Some(search_match(raw.into(), &engine, language));
        }
    }
    matches.sort_by(|a, b| {
        (Reverse(a.0), Reverse(a.1), &a.2.title).cmp(&(Reverse(b.0), Reverse(b.1), &b.2.title))
    });
    let mut rows: Vec<_> = matches
        .into_iter()
        .take(LIMIT - usize::from(fallback.is_some()))
        .map(|(_, _, item)| item)
        .collect();
    if let Some(search) = fallback {
        rows.push(search);
    }
    rows
}

fn frecency(visits: u32, last: u64, now: u64) -> i32 {
    if visits == 0 {
        return 0;
    }
    let age = now.saturating_sub(last) / DAY;
    let recent = match age {
        0 => 34,
        1..=6 => 27 - age as i32,
        7..=30 => 13,
        _ => 0,
    };
    recent + (visits.min(30) * 2) as i32
}

fn search_match(query: String, engine: &str, language: Language) -> Match {
    Match {
        title: query.clone(),
        detail: engine.into(),
        label: language.pick("Поиск", "Search").into(),
        completion: query.clone(),
        action: Action::Search { query },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        navigation::SearchEngine,
        storage::{HistoryEntry, Tab},
    };

    fn rows(data: &BrowserData, input: &str, private: bool) -> Vec<Match> {
        ranked(data, input, Language::En, private, 100 * DAY)
    }

    fn visit(data: &mut BrowserData, url: &str, title: &str, days: u64) {
        data.history.push(HistoryEntry {
            url: url.into(),
            title: title.into(),
            visited_at: (100 - days) * DAY,
        });
    }

    #[test]
    fn suggests_exact_addresses_and_reuses_an_open_bookmark() {
        let mut data = BrowserData {
            tabs: vec![Tab::web("https://xn--e1afmkfd.xn--p1ai:8443/docs")],
            ..Default::default()
        };
        data.tabs[0].id = 42;
        data.toggle_bookmark("https://пример.рф:8443/docs", "Pinned manual");
        visit(&mut data, "https://пример.рф:8443/docs", "Earlier title", 2);
        let matches = rows(&data, "пример.рф:8443/docs", false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].action, Action::Tab { id: 42 });
        assert_eq!(
            matches[0].completion,
            "https://xn--e1afmkfd.xn--p1ai:8443/docs"
        );
    }

    #[test]
    fn ranks_equal_text_by_pins_visits_and_recency() {
        let mut data = BrowserData::default();
        data.home = Some(Dashboard::from_legacy(&data));
        data.home.as_mut().unwrap().tiles.clear();
        visit(&mut data, "https://old.example/", "Reference", 90);
        for _ in 0..4 {
            visit(&mut data, "https://popular.example/", "Reference", 5);
        }
        visit(&mut data, "https://recent.example/", "Reference", 0);
        data.toggle_bookmark("https://bookmark.example/", "Reference");
        let matches = rows(&data, "reference", false);
        assert_eq!(
            matches[..4]
                .iter()
                .map(|entry| entry.detail.as_str())
                .collect::<Vec<_>>(),
            [
                "https://bookmark.example/",
                "https://recent.example/",
                "https://popular.example/",
                "https://old.example/"
            ]
        );
    }

    #[test]
    fn local_queries_follow_search_engine_and_exclude_private_history() {
        let mut data = BrowserData {
            profile: std::rc::Rc::new(crate::storage::ProfileData {
                search_engine: SearchEngine::Yandex,
                ..Default::default()
            }),
            ..Default::default()
        };
        visit(
            &mut data,
            "https://yandex.ru/search/?text=rust+browser",
            "Previous search",
            1,
        );
        visit(
            &mut data,
            "https://www.google.com/search?q=rust+app",
            "Other provider",
            1,
        );
        let recent = Action::Search {
            query: "rust browser".into(),
        };
        assert!(rows(&data, "rust", false)
            .iter()
            .any(|row| row.action == recent));
        assert!(!rows(&data, "rust", true)
            .iter()
            .any(|row| row.action == recent));
        assert!(rows(&data, "> settings", true).iter().any(|row| row.action
            == Action::Command {
                command: Command::Settings
            }));
        assert!(rows(&data, "https://user:password@example.com", false).is_empty());
        data.browser_settings.history_enabled = false;
        assert!(!rows(&data, "rust", false)
            .iter()
            .any(|row| row.action == recent));
        data.search_engine = SearchEngine::Bing;
        assert_eq!(rows(&data, "rust", false).last().unwrap().detail, "Bing");
    }
}
