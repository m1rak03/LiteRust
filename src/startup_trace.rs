//! Local startup timing. All recorded phase names are fixed by the application.
use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, OnceLock,
    },
    time::Instant,
};

static ENTERED: OnceLock<(Instant, Option<u64>)> = OnceLock::new();
static LOG: OnceLock<Mutex<File>> = OnceLock::new();
static RECORDED: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
pub enum Phase {
    ProfilesOpened,
    RuntimeChecked,
    SettingsLoaded,
    WindowCreated,
    ToolbarCreated,
    InterfaceCreated,
    InterfaceVisible,
    PageControllerReady,
    FirstDocumentLoaded,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::ProfilesOpened => "profiles_opened",
            Self::RuntimeChecked => "runtime_checked",
            Self::SettingsLoaded => "settings_loaded",
            Self::WindowCreated => "window_created",
            Self::ToolbarCreated => "toolbar_created",
            Self::InterfaceCreated => "interface_created",
            Self::InterfaceVisible => "interface_visible",
            Self::PageControllerReady => "page_controller_ready",
            Self::FirstDocumentLoaded => "first_document_loaded",
        }
    }
}

pub fn enter() {
    let entered = Instant::now();
    let _ = ENTERED.set((entered, process_age_at_entry()));
}

pub fn open(path: &Path) {
    let Ok(mut file) = File::create(path) else {
        return;
    };
    let _ = writeln!(file, "LiteRust {} startup", env!("CARGO_PKG_VERSION"));
    if let Some((_, Some(age))) = ENTERED.get() {
        let _ = writeln!(file, "startup process_created=0us");
        let _ = writeln!(file, "startup rust_entered={age}us");
    } else {
        let _ = writeln!(file, "startup rust_entered=0us");
    }
    let _ = LOG.set(Mutex::new(file));
}

pub fn phase(phase: Phase) {
    let Some((entry, process_age)) = ENTERED.get() else {
        return;
    };
    let bit = 1 << phase as u32;
    if RECORDED.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
        return;
    }
    let elapsed = entry.elapsed().as_micros() + u128::from(process_age.unwrap_or(0));
    note(&format!("startup {}={elapsed}us", phase.name()));
}

pub fn note(message: &str) {
    if let Some(file) = LOG.get() {
        if let Ok(mut file) = file.lock() {
            let _ = writeln!(file, "{message}");
        }
    }
}

fn process_age_at_entry() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::FILETIME,
            System::{
                SystemInformation::GetSystemTimeAsFileTime,
                Threading::{GetCurrentProcess, GetProcessTimes},
            },
        };
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let entered = unsafe { GetSystemTimeAsFileTime() };
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
            .ok()?;
        }
        let ticks = |stamp: FILETIME| {
            (u64::from(stamp.dwHighDateTime) << 32) | u64::from(stamp.dwLowDateTime)
        };
        Some(ticks(entered).checked_sub(ticks(created))? / 10)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
