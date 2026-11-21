//! Local profile notes. Field-level changes keep separate edits independent.

use std::{collections::HashSet, rc::Rc};

use serde::{Deserialize, Serialize};

use crate::storage::{compact_title, valid_web_url};

const MAX_NOTES: usize = 64;
const MAX_BODY: usize = 20_000;
const MAX_NOTE_BYTES: usize = 768 * 1024;
const MAX_ID: u64 = (1 << 52) - 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub edited_at: u64,
}

impl Note {
    fn bytes(&self) -> usize {
        self.title.len() + self.body.len() + self.url.as_ref().map_or(0, String::len)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Notebook {
    last_id: u64,
    entries: Vec<Rc<Note>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Edit {
    Title { value: String },
    Body { value: String },
}

impl Notebook {
    pub fn entries(&self) -> &[Rc<Note>] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Note> {
        self.entries
            .iter()
            .find(|note| note.id == id)
            .map(Rc::as_ref)
    }

    pub fn create(&mut self, now: u64) -> Option<u64> {
        self.add(String::new(), String::new(), now)
    }

    fn add(&mut self, title: String, body: String, now: u64) -> Option<u64> {
        if self.entries.len() >= MAX_NOTES
            || self.last_id >= MAX_ID
            || self.bytes().saturating_add(title.len() + body.len()) > MAX_NOTE_BYTES
        {
            return None;
        }
        self.last_id += 1;
        self.entries.push(Rc::new(Note {
            id: self.last_id,
            title,
            body,
            url: None,
            edited_at: now,
        }));
        Some(self.last_id)
    }

    pub fn edit(&mut self, id: u64, edit: Edit, now: u64) -> bool {
        let Some(note) = self.get(id) else {
            return false;
        };
        let mut replacement = note.clone();
        match edit {
            Edit::Title { value } => replacement.title = compact_title(value.trim()),
            Edit::Body { value } => replacement.body = value.chars().take(MAX_BODY).collect(),
        }
        self.replace(replacement, now)
    }

    pub fn set_link(&mut self, id: u64, url: Option<&str>, now: u64) -> bool {
        let Some(note) = self.get(id) else {
            return false;
        };
        if url.is_some_and(|url| !valid_web_url(url)) {
            return false;
        }
        let mut replacement = note.clone();
        replacement.url = url.map(str::to_owned);
        self.replace(replacement, now)
    }

    fn replace(&mut self, mut note: Note, now: u64) -> bool {
        let Some(index) = self.entries.iter().position(|item| item.id == note.id) else {
            return false;
        };
        if self.bytes() - self.entries[index].bytes() + note.bytes() > MAX_NOTE_BYTES {
            return false;
        }
        if self.entries[index].as_ref() != &note {
            note.edited_at = now;
            self.entries[index] = Rc::new(note);
        }
        true
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let old_len = self.entries.len();
        self.entries.retain(|note| note.id != id);
        self.entries.len() != old_len
    }

    fn bytes(&self) -> usize {
        self.entries.iter().map(|note| note.bytes()).sum()
    }

    pub fn clean(&mut self, legacy: &mut String) {
        let mut ids = HashSet::new();
        self.entries.retain_mut(|item| {
            if !(1..=MAX_ID).contains(&item.id) || !ids.insert(item.id) {
                return false;
            }
            let note = Rc::make_mut(item);
            note.title = compact_title(note.title.trim());
            note.body = note.body.chars().take(MAX_BODY).collect();
            note.url = note.url.take().filter(|url| valid_web_url(url));
            true
        });
        self.last_id = self
            .last_id
            .min(MAX_ID)
            .max(ids.into_iter().max().unwrap_or(0));
        if !legacy.is_empty() {
            let body: String = legacy.chars().take(MAX_BODY).collect();
            if self.entries.iter().any(|note| note.body == body)
                || self.add(String::new(), body, 0).is_some()
            {
                legacy.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_keeps_exact_legacy_content_once() {
        let mut notebook = Notebook::default();
        let mut legacy = "  Старые заметки\nstripped?  ".into();
        let expected = String::clone(&legacy);
        notebook.clean(&mut legacy);
        assert!(legacy.is_empty());
        assert_eq!(notebook.entries()[0].body, expected);
        legacy = expected;
        notebook.clean(&mut legacy);
        assert!(legacy.is_empty());
        assert_eq!(notebook.entries().len(), 1);
    }

    #[test]
    fn field_edits_keep_other_fields_and_ids_are_never_reused() {
        let mut notebook = Notebook::default();
        let id = notebook.create(1).unwrap();
        assert!(notebook.edit(
            id,
            Edit::Body {
                value: "Body".into()
            },
            2
        ));
        assert!(notebook.set_link(id, Some("https://site.example/"), 3));
        assert!(notebook.edit(
            id,
            Edit::Title {
                value: "Title".into()
            },
            4
        ));
        assert!(!notebook.set_link(id, Some("file:///invalid"), 5));
        assert_eq!(notebook.get(id).unwrap().body, "Body");
        assert_eq!(
            notebook.get(id).unwrap().url.as_deref(),
            Some("https://site.example/")
        );
        assert_eq!(notebook.get(id).unwrap().edited_at, 4);
        assert!(notebook.remove(id));
        assert!(!notebook.edit(
            id,
            Edit::Body {
                value: "Stale".into()
            },
            6
        ));
        assert_ne!(notebook.create(7), Some(id));
    }
}
