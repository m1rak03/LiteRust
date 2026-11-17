//! Bounded home-page model. Grid flow prevents overlap and off-screen positions.
use serde::{Deserialize, Serialize};
pub const WIDGETS: &[&str] = &[
    "logo", "services", "clock", "date", "title", "subtitle", "search", "sites",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub title: String,
    pub url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub span: u8,
    pub hidden: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub widgets: Vec<Widget>,
    pub tiles: Vec<Tile>,
    pub title_text: String,
    pub subtitle_text: String,
}
impl Default for Dashboard {
    fn default() -> Self {
        Self {
            widgets: WIDGETS
                .iter()
                .map(|id| Widget {
                    id: id.to_string(),
                    span: match *id {
                        "logo" | "services" | "clock" | "date" => 6,
                        "search" => 7,
                        "sites" => 5,
                        _ => 12,
                    },
                    hidden: false,
                })
                .collect(),
            tiles: vec![],
            title_text: String::new(),
            subtitle_text: "Начните с вопроса. Продолжите открытием.".into(),
        }
    }
}
impl Dashboard {
    pub fn valid(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.widgets.len() == WIDGETS.len()
            && self.widgets.iter().all(|w| {
                WIDGETS.contains(&w.id.as_str()) && seen.insert(&w.id) && (3..=12).contains(&w.span)
            })
            && self.tiles.len() <= 24
            && self.tiles.iter().all(|t| {
                !t.title.trim().is_empty()
                    && t.title.chars().count() <= 80
                    && t.url.len() <= 2048
                    && crate::storage::valid_web_url(&t.url)
                    && crate::security::safe_top_level_navigation(&t.url)
            })
            && self.title_text.chars().count() <= 120
            && self.subtitle_text.chars().count() <= 240
    }
    pub fn from_legacy(data: &crate::storage::BrowserData) -> Self {
        let tiles = if data.bookmarks.is_empty() {
            vec![
                Tile {
                    title: "Google".into(),
                    url: "https://www.google.com/".into(),
                },
                Tile {
                    title: "YouTube".into(),
                    url: "https://www.youtube.com/".into(),
                },
            ]
        } else {
            data.bookmarks
                .iter()
                .take(24)
                .filter(|b| {
                    b.url.len() <= 2048 && crate::security::safe_top_level_navigation(&b.url)
                })
                .map(|b| Tile {
                    title: if b.title.is_empty() {
                        b.url.chars().take(80).collect()
                    } else {
                        b.title.chars().take(80).collect()
                    },
                    url: b.url.clone(),
                })
                .collect()
        };
        let mut result = Self {
            tiles,
            ..Self::default()
        };
        for w in &mut result.widgets {
            w.hidden = match w.id.as_str() {
                "clock" | "date" => !data.customization.show_clock,
                "title" | "subtitle" => !data.customization.show_greeting,
                "sites" => !data.customization.show_shortcuts,
                "services" => !data.customization.show_services,
                _ => false,
            };
            if data.customization.home_layout == 1 {
                w.span = 12;
            }
        }
        result
    }
}
pub fn script_json(value: &impl Serialize) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "null".into())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validate_grid_and_untrusted_tiles() {
        let mut home = Dashboard::default();
        assert!(home.valid());
        home.widgets[0].span = 255;
        assert!(!home.valid());
        home = Dashboard::default();
        home.widgets[1].id = "logo".into();
        assert!(!home.valid());
        for url in [
            "javascript:alert(1)",
            "file:///C:/secret",
            "https://user:pass@example.com",
        ] {
            home = Dashboard::default();
            home.tiles.push(Tile {
                title: "site".into(),
                url: url.into(),
            });
            assert!(!home.valid());
        }
    }
    #[test]
    fn legacy_and_script_escape() {
        let mut data = crate::storage::BrowserData::default();
        data.toggle_bookmark("https://example.com/", "</script><img src=x>");
        let home = Dashboard::from_legacy(&data);
        assert!(home.valid());
        let json = script_json(&home);
        assert!(!json.contains("</script>"));
        let copy: Dashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(copy.tiles[0].title, "</script><img src=x>");
    }
}
