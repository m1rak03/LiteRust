//! Workspace metadata and tab sessions within a single browser profile.

use std::{collections::HashSet, rc::Rc};

use serde::{Deserialize, Serialize};

use crate::{
    home::Dashboard, split_view::SplitView, storage::Tab, tab_groups::TabGroups,
    window_session::clean_tabs,
};

const MAX_WORKSPACES: usize = 12;
pub type Id = u32;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSession {
    pub tabs: Vec<Tab>,
    pub tab_groups: TabGroups,
    pub split: Option<SplitView>,
    pub active: usize,
}

impl TabSession {
    pub fn clean(&mut self) {
        validate_tabs(
            &mut self.tabs,
            &mut self.active,
            &mut self.tab_groups,
            &mut self.split,
        );
    }

    pub fn assign_ids(&mut self, next: &mut u64) {
        // An unattached session can contain IDs that another space in this window
        // already uses. Reassign them without breaking saved Split View references.
        let layout = self.split;
        for tab in &mut self.tabs {
            if let Some(split) = &mut self.split {
                if let Some(pane) = layout.and_then(|layout| layout.pane(tab.id)) {
                    split.panes[pane.index()] = *next;
                }
            }
            tab.id = *next;
            *next += 1;
        }
    }
}

pub(crate) fn validate_tabs(
    tabs: &mut Vec<Tab>,
    active: &mut usize,
    groups: &mut TabGroups,
    split: &mut Option<SplitView>,
) {
    clean_tabs(tabs, active);
    groups.clean(tabs, active);
    *split = split.and_then(|value| value.validate(tabs, active));
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowWorkspaces {
    pub current: Option<Id>,
    pub ordinary: Option<TabSession>,
}

impl WindowWorkspaces {
    pub fn clean(&mut self, catalog: &Catalog, active: &mut HashSet<Id>) {
        self.current = self
            .current
            .filter(|id| catalog.get(*id).is_some() && active.insert(*id));
        if let Some(session) = &mut self.ordinary {
            session.clean();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.ordinary.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub accent: Option<u32>,
    pub compact_tabs: Option<bool>,
}

impl Display {
    fn validated(mut self) -> Self {
        self.accent = self.accent.filter(|value| *value <= 0xffffff);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub display: Display,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Rc<Dashboard>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Rc<TabSession>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Catalog(Vec<Workspace>);

impl Catalog {
    pub fn items(&self) -> &[Workspace] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, id: Id) -> Option<&Workspace> {
        self.0.iter().find(|item| item.id == id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Workspace> {
        self.0.iter_mut().find(|item| item.id == id)
    }

    pub fn create(&mut self, name: &str) -> Option<Id> {
        let name = self.available_name(None, name)?;
        let id = (1..=MAX_WORKSPACES as Id).find(|id| self.get(*id).is_none())?;
        self.0.push(Workspace {
            id,
            name: name.into(),
            display: Display::default(),
            home: None,
            session: None,
        });
        Some(id)
    }

    pub fn update(&mut self, id: Id, name: &str, display: Display) -> bool {
        let Some(name) = self.available_name(Some(id), name) else {
            return false;
        };
        let Some(workspace) = self.get_mut(id) else {
            return false;
        };
        workspace.name = name.into();
        workspace.display = display.validated();
        true
    }

    fn available_name<'a>(&self, excluded: Option<Id>, name: &'a str) -> Option<&'a str> {
        let name = valid_name(name)?;
        let key = name.to_lowercase();
        (!self
            .0
            .iter()
            .any(|item| Some(item.id) != excluded && item.name.to_lowercase() == key))
        .then_some(name)
    }

    pub fn clean(&mut self) {
        let mut ids = HashSet::new();
        self.0.retain_mut(|item| {
            let Some(name) = valid_name(&item.name) else {
                return false;
            };
            if !(1..=MAX_WORKSPACES as Id).contains(&item.id) || !ids.insert(item.id) {
                return false;
            }
            item.name = name.into();
            item.display = std::mem::take(&mut item.display).validated();
            if item.home.as_ref().is_some_and(|home| !home.valid()) {
                item.home = None;
            }
            if let Some(session) = &mut item.session {
                Rc::make_mut(session).clean();
            }
            true
        });
    }
}

fn valid_name(name: &str) -> Option<&str> {
    let value = name.trim();
    (!value.is_empty() && value.chars().count() <= 48 && !value.chars().any(char::is_control))
        .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{split_view::Pane, storage::BrowserData};

    #[test]
    fn validates_named_spaces_and_keeps_them_profile_local() {
        let mut first = BrowserData::default();
        let mut second = BrowserData::default();
        let id = first.workspaces.create("  Work ").unwrap();
        assert!(first.workspaces.create("work").is_none());
        assert!(first.workspaces.create("bad\nname").is_none());
        assert!(first.workspaces.update(
            id,
            "Projects",
            Display {
                accent: Some(0xff0000),
                compact_tabs: Some(true)
            }
        ));
        assert_eq!(second.workspaces.create("Work"), Some(id));
        assert_eq!(first.workspaces.get(id).unwrap().name, "Projects");
        assert_eq!(second.workspaces.get(id).unwrap().name, "Work");
    }

    #[test]
    fn two_windows_restore_separate_workspaces_and_startup_keeps_named_sessions() {
        let mut data = BrowserData::default();
        let work = data.workspaces.create("Work").unwrap();
        let study = data.workspaces.create("Study").unwrap();
        data.tabs = vec![Tab::web("https://work.example/")];
        data.workspace.current = Some(work);
        data.workspace.ordinary = Some(TabSession {
            tabs: vec![Tab::web("https://ordinary.example/")],
            ..Default::default()
        });
        data.additional_windows
            .push(crate::window_session::WindowSession {
                tabs: vec![Tab::web("https://study.example/")],
                workspace: WindowWorkspaces {
                    current: Some(study),
                    ordinary: None,
                },
                ..Default::default()
            });
        let saved = serde_json::to_vec(&data).unwrap();
        let mut restored = serde_json::from_slice::<BrowserData>(&saved)
            .unwrap()
            .clean();
        assert_eq!(restored.workspace.current, Some(work));
        assert_eq!(
            restored.additional_windows[0].workspace.current,
            Some(study)
        );
        assert_eq!(
            restored.workspace.ordinary.as_ref().unwrap().tabs[0].url,
            "https://ordinary.example/"
        );
        restored.browser_settings.startup = crate::storage::Startup::NewTab;
        restored.apply_startup(false);
        assert_eq!(restored.tabs[0].url, crate::storage::NEW_TAB);
        assert_eq!(
            restored
                .workspaces
                .get(work)
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .tabs[0]
                .url,
            "https://work.example/"
        );
        assert_eq!(
            restored
                .workspaces
                .get(study)
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .tabs[0]
                .url,
            "https://study.example/"
        );
    }

    #[test]
    fn migrates_regular_tabs_and_preserves_split_on_attachment() {
        let mut data: BrowserData = serde_json::from_str(
            r#"{"tabs":[{"id":1,"url":"https://old.example/","title":"Old site"}],"active":0}"#,
        )
        .unwrap();
        data = data.clean();
        assert!(data.workspaces.is_empty());
        assert_eq!(data.tabs[0].url, "https://old.example/");
        let mut session = TabSession {
            tabs: vec![
                Tab::web("https://one.example/"),
                Tab::web("https://two.example/"),
            ],
            ..Default::default()
        };
        session.clean();
        session.split = Some(SplitView::new(session.tabs[0].id, session.tabs[1].id));
        session.split.as_mut().unwrap().focus = Pane::Second;
        session.assign_ids(&mut 40);
        session.clean();
        assert_eq!(session.split.unwrap().panes, [40, 41]);
        assert_eq!(session.active, 1);
        assert_eq!(session.tabs[1].url, "https://two.example/");
    }
}
