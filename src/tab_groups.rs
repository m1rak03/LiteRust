//! Window-scoped metadata for the existing tab strip.
use crate::{
    storage::{Tab, MAX_TABS},
    window_session,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    #[default]
    Blue,
    Teal,
    Green,
    Amber,
    Red,
    Violet,
    #[serde(other)]
    Gray,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: u32,
    pub title: String,
    pub color: Color,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TabGroups {
    entries: Vec<TabGroup>,
    last_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Create {
        ids: Vec<u64>,
        title: String,
        color: Color,
    },
    Edit {
        id: u32,
        title: String,
        color: Color,
    },
    Assign {
        ids: Vec<u64>,
        group: Option<u32>,
    },
    Collapse {
        id: u32,
    },
    Pin {
        id: u32,
    },
    Dissolve {
        id: u32,
    },
    Reorder {
        source: u32,
        target: u32,
        after: bool,
    },
}

impl TabGroups {
    pub fn apply(&mut self, tabs: &mut [Tab], active: &mut usize, operation: Operation) -> bool {
        match operation {
            Operation::Create { ids, title, color } => {
                self.create(tabs, active, &ids, &title, color)
            }
            Operation::Edit { id, title, color } => self.edit(id, &title, color),
            Operation::Assign { ids, group } => self.assign(tabs, active, &ids, group),
            Operation::Reorder {
                source,
                target,
                after,
            } => self.move_group(tabs, active, source, target, after),
            Operation::Collapse { id } => {
                self.toggle_collapse(id);
                true
            }
            Operation::Pin { id } => {
                self.toggle_pin(tabs, active, id);
                true
            }
            Operation::Dissolve { id } => {
                self.dissolve(tabs, active, id);
                true
            }
        }
    }

    pub fn entries(&self) -> &[TabGroup] {
        &self.entries
    }

    pub fn clean(&mut self, tabs: &mut [Tab], active: &mut usize) {
        let mut seen = HashSet::with_capacity(MAX_TABS);
        self.entries.truncate(MAX_TABS);
        self.entries.retain_mut(|group| {
            let Some(title) = valid_title(&group.title) else {
                return false;
            };
            group.title = title;
            group.id != 0 && seen.insert(group.id)
        });
        self.organize(tabs, active);
        if let Some(largest) = self.entries.iter().map(|group| group.id).max() {
            self.last_id = self.last_id.max(largest);
        }
    }

    pub fn create(
        &mut self,
        tabs: &mut [Tab],
        active: &mut usize,
        ids: &[u64],
        title: &str,
        color: Color,
    ) -> bool {
        let Some(title) = valid_title(title) else {
            return false;
        };
        if !valid_selection(tabs, ids) {
            return false;
        }
        let mut id = self.last_id.wrapping_add(1).max(1);
        while self.entries.iter().any(|group| group.id == id) {
            id = id.wrapping_add(1).max(1);
        }
        self.last_id = id;
        self.entries.push(TabGroup {
            id,
            title,
            color,
            pinned: false,
            collapsed: false,
        });
        self.assign(tabs, active, ids, Some(id));
        true
    }

    pub fn edit(&mut self, id: u32, title: &str, color: Color) -> bool {
        let Some(title) = valid_title(title) else {
            return false;
        };
        let Some(group) = self.get_mut(id) else {
            return false;
        };
        group.title = title;
        group.color = color;
        true
    }

    pub fn toggle_collapse(&mut self, id: u32) {
        if let Some(group) = self.get_mut(id) {
            group.collapsed = !group.collapsed;
        }
    }

    pub fn toggle_pin(&mut self, tabs: &mut [Tab], active: &mut usize, id: u32) {
        if let Some(group) = self.get_mut(id) {
            group.pinned = !group.pinned;
            self.organize(tabs, active);
        }
    }

    pub fn dissolve(&mut self, tabs: &mut [Tab], active: &mut usize, id: u32) {
        self.entries.retain(|group| group.id != id);
        self.organize(tabs, active);
    }

    pub fn assign(
        &mut self,
        tabs: &mut [Tab],
        active: &mut usize,
        ids: &[u64],
        group: Option<u32>,
    ) -> bool {
        if !valid_selection(tabs, ids)
            || group.is_some_and(|id| !self.entries.iter().any(|entry| entry.id == id))
        {
            return false;
        }
        for tab in tabs.iter_mut().filter(|tab| ids.contains(&tab.id)) {
            tab.group = group;
        }
        self.organize(tabs, active);
        true
    }

    pub fn move_tab(
        &mut self,
        tabs: &mut [Tab],
        active: &mut usize,
        source: u64,
        target: u64,
        after: bool,
    ) -> bool {
        if source == target {
            return false;
        }
        let Some(group) = tabs
            .iter()
            .find(|tab| tab.id == target)
            .map(|tab| tab.group)
        else {
            return false;
        };
        let Some(source_tab) = tabs.iter_mut().find(|tab| tab.id == source) else {
            return false;
        };
        let changed = source_tab.group != group;
        source_tab.group = group;
        let reordered = window_session::move_tab(tabs, active, source, target, after);
        if changed {
            self.organize(tabs, active);
        }
        changed || reordered
    }

    pub fn move_group(
        &mut self,
        tabs: &mut [Tab],
        active: &mut usize,
        source: u32,
        target: u32,
        after: bool,
    ) -> bool {
        let (Some(from), Some(to)) = (
            self.entries.iter().position(|group| group.id == source),
            self.entries.iter().position(|group| group.id == target),
        ) else {
            return false;
        };
        if from == to || self.entries[from].pinned != self.entries[to].pinned {
            return false;
        }
        let new = to + usize::from(after) - usize::from(from < to);
        if new == from {
            return false;
        }
        let group = self.entries.remove(from);
        self.entries.insert(new, group);
        self.organize(tabs, active);
        true
    }

    pub fn organize(&mut self, tabs: &mut [Tab], active: &mut usize) {
        let selected = tabs.get(*active).map(|tab| tab.id);
        self.entries
            .retain(|group| tabs.iter().any(|tab| tab.group == Some(group.id)));
        self.entries.sort_by_key(|group| !group.pinned);
        for tab in tabs.iter_mut() {
            if tab
                .group
                .is_some_and(|id| !self.entries.iter().any(|group| group.id == id))
            {
                tab.group = None;
            }
        }
        tabs.sort_by_key(|tab| {
            self.entries
                .iter()
                .position(|group| Some(group.id) == tab.group)
                .unwrap_or(self.entries.len())
        });
        if let Some(index) = tabs.iter().position(|tab| Some(tab.id) == selected) {
            *active = index;
        }
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut TabGroup> {
        self.entries.iter_mut().find(|group| group.id == id)
    }
}

fn valid_title(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty() && title.chars().count() <= 40 && !title.chars().any(char::is_control))
        .then(|| title.to_owned())
}

fn valid_selection(tabs: &[Tab], ids: &[u64]) -> bool {
    !ids.is_empty()
        && ids.len() <= MAX_TABS
        && ids.iter().all(|id| tabs.iter().any(|tab| tab.id == *id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::BrowserData;

    fn session() -> BrowserData {
        BrowserData {
            tabs: ["one", "two", "three", "four"]
                .into_iter()
                .map(|name| Tab::web(&format!("https://{name}.example/")))
                .collect(),
            active: 1,
            ..BrowserData::default()
        }
        .clean()
    }

    #[test]
    fn tabs_move_between_groups_and_dissolve_without_closing_pages() {
        let mut data = session();
        assert!(data.tab_groups.create(
            &mut data.tabs,
            &mut data.active,
            &[1, 3],
            "One",
            Color::Amber
        ));
        assert!(data.tab_groups.create(
            &mut data.tabs,
            &mut data.active,
            &[2],
            "Two",
            Color::Violet
        ));
        assert!(data
            .tab_groups
            .move_tab(&mut data.tabs, &mut data.active, 2, 3, true));
        assert_eq!(data.tab_groups.entries().len(), 1);
        assert_eq!(data.tabs[data.active].id, 2);
        assert_eq!(
            data.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [1, 3, 2, 4]
        );
        assert!(data
            .tab_groups
            .assign(&mut data.tabs, &mut data.active, &[3], None));
        let pinned = data.tab_groups.entries()[0].id;
        data.tab_groups
            .dissolve(&mut data.tabs, &mut data.active, pinned);
        assert!(data.tab_groups.entries().is_empty());
        assert!(data.tabs.iter().all(|tab| tab.group.is_none()));
        assert_eq!(data.tabs.len(), 4);
        assert_eq!(data.tabs[data.active].id, 2);
    }

    #[test]
    fn order_collapse_color_and_active_page_survive_restore() {
        let mut data = session();
        assert!(data.tab_groups.create(
            &mut data.tabs,
            &mut data.active,
            &[1],
            "First",
            Color::Gray
        ));
        assert!(data.tab_groups.create(
            &mut data.tabs,
            &mut data.active,
            &[2, 3],
            "Second",
            Color::Red
        ));
        assert!(data
            .tab_groups
            .move_group(&mut data.tabs, &mut data.active, 2, 1, false));
        assert!(data.tab_groups.edit(2, "Changed", Color::Teal));
        data.tab_groups.toggle_collapse(2);
        data.tab_groups
            .toggle_pin(&mut data.tabs, &mut data.active, 2);
        assert!(!data
            .tab_groups
            .move_group(&mut data.tabs, &mut data.active, 1, 2, false));
        let saved = serde_json::to_vec(&data).unwrap();
        let restored = serde_json::from_slice::<BrowserData>(&saved)
            .unwrap()
            .clean();
        let group = &restored.tab_groups.entries()[0];
        assert_eq!(
            (group.id, group.title.as_str(), group.color),
            (2, "Changed", Color::Teal)
        );
        assert!(group.pinned && group.collapsed);
        assert_eq!(restored.tabs[0].group, Some(2));
        assert_eq!(restored.tabs[restored.active].url, "https://two.example/");
        assert!(BrowserData::default().tab_groups.entries().is_empty());
    }

    #[test]
    fn malformed_group_membership_preserves_legacy_tabs() {
        let data = serde_json::from_str::<BrowserData>(
            r#"{
            "tabs":[{"url":"https://one.example/","title":"One","group":99},
                    {"url":"https://two.example/","title":"Two"}],"active":1
        }"#,
        )
        .unwrap()
        .clean();
        assert!(data.tabs.iter().all(|tab| tab.group.is_none()));
        assert_eq!(data.tabs[data.active].title, "Two");
        assert!(data.tab_groups.entries().is_empty());
    }
}
