//! Bounded local diagnostics. Events contain fixed codes and timestamps only.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const JOURNAL_BYTES: u64 = 128 * 1024;
const MAX_RECENT: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    StartupBegun,
    FirstDocumentLoaded,
    StartupFailed,
    RuntimeAvailable,
    RuntimeUnavailable,
    ControllerFailed,
    WindowOpened,
    WindowClosed,
    WindowRestoreFailed,
    TabOpened,
    TabClosed,
    TabReleased,
    NavigationRejected,
    NetworkRulesFailed,
    ProxyStarted,
    ProxyStopped,
    SystemTunnelUnavailable,
    ProfileOpened,
    ProfileSaveFailed,
    DownloadStarted,
    DownloadCompleted,
    DownloadCancelled,
    DownloadInterrupted,
    JournalUnavailable,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Component {
    Startup,
    WebView2,
    Windows,
    Tabs,
    Network,
    Vpn,
    Profiles,
    Downloads,
}

impl Event {
    fn component(self) -> Component {
        use Event::*;
        match self {
            StartupBegun | FirstDocumentLoaded | StartupFailed | JournalUnavailable => {
                Component::Startup
            }
            RuntimeAvailable | RuntimeUnavailable | ControllerFailed => Component::WebView2,
            WindowOpened | WindowClosed | WindowRestoreFailed => Component::Windows,
            TabOpened | TabClosed | TabReleased => Component::Tabs,
            NavigationRejected | NetworkRulesFailed => Component::Network,
            ProxyStarted | ProxyStopped | SystemTunnelUnavailable => Component::Vpn,
            ProfileOpened | ProfileSaveFailed => Component::Profiles,
            DownloadStarted | DownloadCompleted | DownloadCancelled | DownloadInterrupted => {
                Component::Downloads
            }
        }
    }

    fn is_error(self) -> bool {
        use Event::*;
        matches!(
            self,
            StartupFailed
                | RuntimeUnavailable
                | ControllerFailed
                | WindowRestoreFailed
                | NetworkRulesFailed
                | ProxyStopped
                | SystemTunnelUnavailable
                | ProfileSaveFailed
                | DownloadInterrupted
                | JournalUnavailable
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct Entry {
    #[serde(alias = "timestamp")]
    at: u64,
    event: Event,
}

impl Entry {
    fn new(event: Event) -> Self {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self { at, event }
    }
}

impl Serialize for Entry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct LogLine {
            timestamp: u64,
            component: Component,
            event: Event,
            error: bool,
        }
        LogLine {
            timestamp: self.at,
            component: self.event.component(),
            event: self.event,
            error: self.event.is_error(),
        }
        .serialize(serializer)
    }
}

type History = Arc<Mutex<VecDeque<Entry>>>;

pub struct Journal {
    history: History,
    writer: Option<mpsc::SyncSender<Entry>>,
}

impl Journal {
    pub fn open(path: Option<PathBuf>) -> Self {
        let history = History::default();
        let writer = path.and_then(|path| {
            let (tx, rx) = mpsc::sync_channel(MAX_RECENT);
            let recent = history.clone();
            std::thread::Builder::new()
                .name("literust-journal".into())
                .spawn(move || run_writer(&path, &recent, rx))
                .ok()
                .map(|_| tx)
        });
        Self { history, writer }
    }

    pub fn record(&self, event: Event) {
        let entry = Entry::new(event);
        push(&self.history, entry);
        if let Some(writer) = &self.writer {
            let _ = writer.try_send(entry);
        }
    }

    pub fn has_local_writer(&self) -> bool {
        self.writer.is_some()
    }

    pub fn recent(&self) -> Vec<Entry> {
        self.history.lock().map_or_else(
            |_| Vec::new(),
            |history| history.iter().rev().copied().collect(),
        )
    }
}

fn push(history: &History, entry: Entry) {
    if let Ok(mut history) = history.lock() {
        if history.len() == MAX_RECENT {
            history.pop_front();
        }
        history.push_back(entry);
    }
}

fn archived(path: &Path) -> PathBuf {
    path.with_extension("jsonl.previous")
}

fn read_recent(path: &Path) -> VecDeque<Entry> {
    let mut entries = VecDeque::new();
    for file in [archived(path), path.to_path_buf()] {
        let Ok(bytes) = crate::profile::read_limited(&file, JOURNAL_BYTES) else {
            continue;
        };
        for line in bytes.split(|&byte| byte == b'\n') {
            if line.len() > 512 {
                continue;
            }
            if let Ok(mut entry) = serde_json::from_slice::<Entry>(line) {
                // Keep fields under a fixed schema even when an old log has extra data.
                if entries.len() == MAX_RECENT {
                    entries.pop_front();
                }
                let now = Entry::new(entry.event).at;
                entry.at = entry.at.min(now);
                entries.push_back(entry);
            }
        }
    }
    entries
}

fn run_writer(path: &Path, history: &History, records: mpsc::Receiver<Entry>) {
    let mut previous = read_recent(path);
    if let Ok(mut current) = history.lock() {
        previous.append(&mut current);
        previous.drain(..previous.len().saturating_sub(MAX_RECENT));
        *current = previous;
    }
    let mut failed = false;
    for entry in records {
        let error = append(path, entry, JOURNAL_BYTES).is_err();
        if error && !failed {
            push(history, Entry::new(Event::JournalUnavailable));
        }
        failed = error;
    }
}

fn append(path: &Path, entry: Entry, limit: u64) -> io::Result<()> {
    let mut line = serde_json::to_vec(&entry).map_err(io::Error::other)?;
    line.push(b'\n');
    let size = fs::metadata(path).map_or(0, |info| info.len());
    if line.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Diagnostic record too large",
        ));
    }
    if size > limit.saturating_sub(line.len() as u64) {
        if size <= limit {
            let bytes = crate::profile::read_limited(path, limit)?;
            crate::profile::atomic_write(&archived(path), &bytes)?;
        }
        crate::profile::atomic_write(path, &line)?;
    } else {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&line)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_is_bounded_and_export_uses_the_fixed_schema() {
        let root = std::env::temp_dir().join(format!("literust-journal-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("events.jsonl");
        fs::write(&path, b"{broken}\n{\"at\":25,\"event\":\"window_opened\",\"password\":\"SECRET\",\"url\":\"https://private.example/\"}\n").unwrap();
        let entries = read_recent(&path);
        assert_eq!(entries.len(), 1);
        let exported = serde_json::to_string(&entries).unwrap();
        assert!(exported.contains("windows"));
        assert!(!exported.contains("SECRET"));
        assert!(!exported.contains("private.example"));
        for _ in 0..20 {
            append(&path, Entry::new(Event::DownloadInterrupted), 512).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= 512);
        assert!(fs::metadata(archived(&path)).unwrap().len() <= 512);
        assert_eq!(
            read_recent(&path).back().unwrap().event,
            Event::DownloadInterrupted
        );
        fs::remove_file(archived(&path)).unwrap();
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn journal_keeps_working_after_a_write_error() {
        let root =
            std::env::temp_dir().join(format!("literust-missing-journal-{}", std::process::id()));
        let log = root.join("events.jsonl");
        let history = History::default();
        let write = |event| {
            let (tx, rx) = mpsc::channel();
            tx.send(Entry::new(event)).unwrap();
            drop(tx);
            run_writer(&log, &history, rx);
        };
        write(Event::StartupBegun);
        assert_eq!(
            history.lock().unwrap().back().unwrap().event,
            Event::JournalUnavailable
        );
        fs::create_dir_all(&root).unwrap();
        write(Event::RuntimeAvailable);
        assert_eq!(
            read_recent(&log).back().unwrap().event,
            Event::RuntimeAvailable
        );
        fs::remove_file(log).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn in_memory_journal_caps_retention_and_classifies_errors() {
        let journal = Journal::open(None);
        for _ in 0..150 {
            journal.record(Event::TabOpened);
        }
        journal.record(Event::ProxyStopped);
        let recent = journal.recent();
        assert_eq!(recent.len(), MAX_RECENT);
        assert!(recent[0].event.is_error());
        assert_eq!(recent[0].event, Event::ProxyStopped);
        assert!(journal.writer.is_none());
    }
}
