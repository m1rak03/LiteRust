//! Window-local tabs and placement. Profile data is stored independently.
use serde::{Deserialize, Serialize};

use crate::storage::{compact_title, valid_tab_url, Tab, MAX_TABS};

pub const MAX_WINDOWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Placement {
    pub fn valid(self) -> bool {
        (200..=32_768).contains(&self.width) && (150..=32_768).contains(&self.height)
    }

    pub fn overlaps(self, x: i32, y: i32, width: u32, height: u32) -> bool {
        let (l, t, ml, mt) = (
            i64::from(self.x),
            i64::from(self.y),
            i64::from(x),
            i64::from(y),
        );
        let overlap_width = (l + i64::from(self.width)).min(ml + i64::from(width)) - l.max(ml);
        let overlap_height = (t + i64::from(self.height)).min(mt + i64::from(height)) - t.max(mt);
        // Leave enough of the window available to move it.
        overlap_width >= 80 && overlap_height >= 80
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSession {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub placement: Option<Placement>,
}

impl WindowSession {
    pub fn clean(&mut self) {
        clean_tabs(&mut self.tabs, &mut self.active);
        self.placement = self.placement.filter(|placement| placement.valid());
    }
}

pub(crate) fn clean_tabs(tabs: &mut Vec<Tab>, active: &mut usize) {
    tabs.retain(|t| valid_tab_url(&t.url));
    tabs.truncate(MAX_TABS);
    if tabs.is_empty() {
        tabs.push(Tab::new());
    }
    *active = (*active).min(tabs.len() - 1);
    // Pre-v0.5 tabs had no stable ID. Also repair duplicates or zero IDs
    // so callbacks from closed/suspended WebViews never affect other tabs.
    // IDs identify a running session only. Reassign on startup to avoid
    // collisions / saturating arithmetic from hostile persisted u64::MAX.
    for (index, tab) in tabs.iter_mut().enumerate() {
        tab.id = index as u64 + 1;
    }
    for tab in tabs {
        tab.title = compact_title(&tab.title);
        tab.back.retain(|url| valid_tab_url(url));
        tab.forward.retain(|url| valid_tab_url(url));
        if tab.back.len() > 40 {
            tab.back.drain(..tab.back.len() - 40);
        }
        if tab.forward.len() > 40 {
            tab.forward.drain(..tab.forward.len() - 40);
        }
    }
}

/// Reorder tabs without recreating their WebViews or changing the selected tab.
pub fn move_tab(
    tabs: &mut [Tab],
    active: &mut usize,
    source: u64,
    target: u64,
    after: bool,
) -> bool {
    let (Some(from), Some(to)) = (
        tabs.iter().position(|tab| tab.id == source),
        tabs.iter().position(|tab| tab.id == target),
    ) else {
        return false;
    };
    if from == to {
        return false;
    }
    let new_index = to + usize::from(after) - usize::from(from < to);
    if new_index == from {
        return false;
    }
    let selected = tabs[*active].id;
    if new_index > from {
        tabs[from..=new_index].rotate_left(1);
    } else {
        tabs[new_index..=from].rotate_right(1);
    }
    *active = tabs
        .iter()
        .position(|tab| tab.id == selected)
        .expect("the selected tab still exists");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tab_preserves_selection_and_identity() {
        let mut tabs = vec![Tab::new(), Tab::new(), Tab::new(), Tab::new()];
        let mut active = 2;
        clean_tabs(&mut tabs, &mut active);
        assert!(move_tab(&mut tabs, &mut active, 1, 3, true));
        assert_eq!(
            tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [2, 3, 1, 4]
        );
        assert_eq!(active, 1);
        assert!(move_tab(&mut tabs, &mut active, 4, 3, false));
        assert_eq!(
            tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [2, 4, 3, 1]
        );
        assert_eq!(active, 2);
        assert!(!move_tab(&mut tabs, &mut active, 5, 3, true));
    }

    #[test]
    fn placement_accepts_a_monitor_left_of_the_primary() {
        let position = Placement {
            x: -1400,
            y: 100,
            width: 1100,
            height: 700,
            maximized: false,
        };
        assert!(position.valid());
        assert!(position.overlaps(-1920, 0, 1920, 1080));
        assert!(!position.overlaps(0, 0, 1920, 1080));
    }

    #[test]
    fn profile_format_migrates_existing_tabs_and_roundtrips_multiple_windows() {
        let mut data: crate::storage::BrowserData = serde_json::from_str(
            r#"{"tabs":[{"url":"https://old.example/","title":"Old tab"}],"active":0,"notes":"existing"}"#
        ).unwrap();
        data.additional_windows.push(WindowSession {
            tabs: vec![Tab::web("https://second.example/")],
            ..Default::default()
        });
        let restored: crate::storage::BrowserData =
            serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
        let restored = restored.clean();
        assert_eq!(restored.tabs[0].url, "https://old.example/");
        assert_eq!(restored.notes, "existing");
        assert_eq!(
            restored.additional_windows[0].tabs[0].url,
            "https://second.example/"
        );
    }
}
