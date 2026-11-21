//! Validated, bounded shortcut assignments shared by every native WebView.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("new_tab", "Новая вкладка", "Ctrl+T"),
    ("new_window", "Новое окно", "Ctrl+N"),
    ("close_tab", "Закрыть вкладку", "Ctrl+W|Ctrl+F4"),
    ("reopen_tab", "Вернуть закрытую вкладку", "Ctrl+Shift+T"),
    ("next_tab", "Следующая вкладка", "Ctrl+Tab|Ctrl+PageDown"),
    (
        "previous_tab",
        "Предыдущая вкладка",
        "Ctrl+Shift+Tab|Ctrl+PageUp",
    ),
    ("address", "Адресная строка", "Ctrl+L|Alt+D|F6"),
    ("reload", "Обновить страницу", "Ctrl+R|F5"),
    ("back", "Назад", "Alt+Left"),
    ("forward", "Вперёд", "Alt+Right"),
    ("home", "Домашняя страница", "Alt+Home"),
    ("private_window", "Приватное окно", "Ctrl+Shift+N"),
    ("bookmark", "Добавить / убрать закладку", "Ctrl+D"),
    ("bookmarks", "Закладки", "Ctrl+Shift+O"),
    ("history", "История", "Ctrl+H"),
    ("downloads", "Загрузки", "Ctrl+J"),
    ("print", "Печать / PDF", "Ctrl+P"),
    ("find", "Найти на странице", "Ctrl+F|F3"),
    (
        "zoom_in",
        "Увеличить страницу",
        "Ctrl+Equal|Ctrl+Shift+Equal",
    ),
    ("zoom_out", "Уменьшить страницу", "Ctrl+Minus"),
    ("zoom_reset", "Масштаб 100%", "Ctrl+Digit0"),
    ("settings", "Настройки", "Ctrl+Comma"),
    ("vpn", "VPN", ""),
    ("tools", "Инструменты", ""),
    ("accounts", "Аккаунты", ""),
    ("duplicate", "Дублировать вкладку", ""),
    ("tab_1", "Вкладка 1", "Ctrl+Digit1"),
    ("tab_2", "Вкладка 2", "Ctrl+Digit2"),
    ("tab_3", "Вкладка 3", "Ctrl+Digit3"),
    ("tab_4", "Вкладка 4", "Ctrl+Digit4"),
    ("tab_5", "Вкладка 5", "Ctrl+Digit5"),
    ("tab_6", "Вкладка 6", "Ctrl+Digit6"),
    ("tab_7", "Вкладка 7", "Ctrl+Digit7"),
    ("tab_8", "Вкладка 8", "Ctrl+Digit8"),
    ("tab_9", "Последняя вкладка", "Ctrl+Digit9"),
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Shortcuts(pub BTreeMap<String, Vec<String>>);
impl Default for Shortcuts {
    fn default() -> Self {
        Self(
            ACTIONS
                .iter()
                .map(|(id, _, keys)| {
                    (
                        id.to_string(),
                        keys.split('|')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}
impl Shortcuts {
    pub fn add_window_shortcut(&mut self) {
        let key = "Ctrl+N";
        let entry = if self.action(key).is_none() {
            vec![key.into()]
        } else {
            Vec::new()
        };
        self.0.entry("new_window".into()).or_insert(entry);
    }

    pub fn validate(&self) -> bool {
        if self.0.len() != ACTIONS.len() {
            return false;
        }
        let mut seen = BTreeSet::new();
        ACTIONS.iter().all(|(id, _, _)| {
            self.0.get(*id).is_some_and(|keys| {
                keys.len() <= 3 && keys.iter().all(|key| valid_chord(key) && seen.insert(key))
            })
        })
    }
    pub fn action(&self, chord: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, keys)| keys.iter().any(|k| k == chord))
            .map(|(id, _)| id.as_str())
    }
}
pub fn valid_chord(chord: &str) -> bool {
    if chord.len() > 48 {
        return false;
    }
    let mut rest = chord;
    let ctrl = rest.starts_with("Ctrl+");
    if ctrl {
        rest = &rest[5..];
    }
    let alt = rest.starts_with("Alt+");
    if alt {
        rest = &rest[4..];
    }
    let shift = rest.starts_with("Shift+");
    if shift {
        rest = &rest[6..];
    }
    let fkey = rest
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    let key = (rest.len() == 1 && rest.as_bytes()[0].is_ascii_uppercase())
        || (rest.starts_with("Digit") && rest.len() == 6 && rest.as_bytes()[5].is_ascii_digit())
        || [
            "Tab", "PageUp", "PageDown", "Left", "Right", "Up", "Down", "Home", "End", "Equal",
            "Minus", "Comma", "Period",
        ]
        .contains(&rest)
        || fkey;
    key && (ctrl || alt || fkey)
        && ![
            "Alt+F4",
            "Ctrl+Alt+Delete",
            "Ctrl+C",
            "Ctrl+V",
            "Ctrl+X",
            "Ctrl+A",
            "Ctrl+Z",
            "Ctrl+Y",
            "Ctrl+Shift+V",
            "Ctrl+Shift+Z",
            "Ctrl+Alt+L",
        ]
        .contains(&chord)
        && !(ctrl && alt)
        && !(shift && !ctrl && !alt && !fkey)
}
pub fn chord_from_vk(key: u32, ctrl: bool, alt: bool, shift: bool) -> Option<String> {
    let key = match key {
        65..=90 => (key as u8 as char).to_string(),
        48..=57 => format!("Digit{}", key - 48),
        112..=123 => format!("F{}", key - 111),
        9 => "Tab".into(),
        33 => "PageUp".into(),
        34 => "PageDown".into(),
        35 => "End".into(),
        36 => "Home".into(),
        37 => "Left".into(),
        38 => "Up".into(),
        39 => "Right".into(),
        40 => "Down".into(),
        187 | 107 => "Equal".into(),
        189 | 109 => "Minus".into(),
        188 => "Comma".into(),
        190 => "Period".into(),
        _ => return None,
    };
    Some(format!(
        "{}{}{}{}",
        if ctrl { "Ctrl+" } else { "" },
        if alt { "Alt+" } else { "" },
        if shift { "Shift+" } else { "" },
        key
    ))
}
#[derive(Default)]
pub struct Runtime {
    pub bindings: Shortcuts,
    pub recording: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_valid_unique_and_exact() {
        let s = Shortcuts::default();
        assert!(s.validate());
        assert_eq!(s.action("Ctrl+T"), Some("new_tab"));
        assert_eq!(s.action("Ctrl+Shift+T"), Some("reopen_tab"));
        assert_eq!(s.action("Alt+T"), None);
    }
    #[test]
    fn edits_roundtrip_and_disable() {
        let mut s = Shortcuts::default();
        s.0.insert("new_tab".into(), vec!["Ctrl+Shift+Y".into()]);
        s.0.insert("print".into(), vec![]);
        assert!(s.validate());
        let copy: Shortcuts = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(copy.action("Ctrl+T"), None);
        assert_eq!(copy.action("Ctrl+Shift+Y"), Some("new_tab"));
        assert_eq!(copy.action("F11"), None);
    }
    #[test]
    fn rejects_conflicts_typing_reserved_unknown() {
        let mut s = Shortcuts::default();
        for key in [
            "Ctrl+W",
            "A",
            "Shift+A",
            "Ctrl+C",
            "Ctrl+Alt+Q",
            "Alt+F4",
            "Escape",
            "Win+L",
        ] {
            s.0.insert("new_tab".into(), vec![key.into()]);
            assert!(!s.validate(), "{key}");
        }
        assert!(!valid_chord("Ctrl+Ctrl+T"));
        s = Shortcuts::default();
        s.0.insert("evil".into(), vec![]);
        assert!(!s.validate());
    }
    #[test]
    fn native_key_normalization() {
        assert_eq!(
            chord_from_vk(84, true, false, true).as_deref(),
            Some("Ctrl+Shift+T")
        );
        assert_eq!(
            chord_from_vk(122, false, false, false).as_deref(),
            Some("F11")
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    #[test]
    fn profile_migration_and_custom_shortcuts_survive() {
        let old: crate::storage::BrowserData = serde_json::from_str("{}").unwrap();
        assert!(old.shortcuts.validate());
        let mut data = crate::storage::BrowserData::default();
        data.shortcuts
            .0
            .insert("new_tab".into(), vec!["Ctrl+Shift+Y".into()]);
        data.shortcuts.0.insert("print".into(), vec![]);
        let restored: crate::storage::BrowserData =
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        let restored = restored.clean();
        assert_eq!(restored.shortcuts.action("Ctrl+Shift+Y"), Some("new_tab"));
        assert_eq!(restored.shortcuts.action("F11"), None);
        data.shortcuts
            .0
            .insert("close_tab".into(), vec!["Ctrl+Shift+Y".into()]);
        assert_eq!(data.clean().shortcuts.action("Ctrl+T"), Some("new_tab"));
    }
}
