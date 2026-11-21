//! Runtime process inventory and Win32 sampling. Views may share processes.
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashSet},
    time::{Duration, Instant},
};
use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment8, ICoreWebView2_8};
use windows::{
    core::{Interface, BOOL},
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    },
};
use wry::{WebView, WebViewExtWindows};

#[derive(Debug, Default, Serialize)]
pub struct Sample {
    pub process_count: Option<usize>,
    pub private_bytes: Option<u64>,
    pub memory_coverage: usize,
    pub cpu_percent: Option<f64>,
    pub cpu_coverage: usize,
    pub partial_inventory: bool,
}

#[derive(Default)]
pub struct Inventory {
    ids: BTreeSet<u32>,
    complete: usize,
    missing: usize,
}

pub fn playing_audio(view: &WebView) -> Option<bool> {
    unsafe {
        let core: ICoreWebView2_8 = view.webview().cast().ok()?;
        let mut playing = BOOL::default();
        core.IsDocumentPlayingAudio(&mut playing).ok()?;
        Some(playing.as_bool())
    }
}

fn process_ids(environment: &ICoreWebView2Environment8) -> windows::core::Result<BTreeSet<u32>> {
    let mut ids = BTreeSet::new();
    unsafe {
        let processes = environment.GetProcessInfos()?;
        let mut count = 0;
        processes.Count(&mut count)?;
        for index in 0..count {
            let mut id = 0;
            processes.GetValueAtIndex(index)?.ProcessId(&mut id)?;
            if id > 0 {
                ids.insert(id as u32);
            }
        }
    }
    Ok(ids)
}

// Query the COM environments on their owning UI thread. Only the PID set crosses
// into the sampling worker.
pub fn inventory<'a>(views: impl IntoIterator<Item = &'a WebView>) -> Inventory {
    let mut inventory = Inventory::default();
    let mut environments = HashSet::new();
    for view in views {
        let environment = view.environment();
        if !environments.insert(environment.as_raw()) {
            continue;
        }
        match environment
            .cast()
            .and_then(|environment| process_ids(&environment))
        {
            Ok(ids) => {
                inventory.complete += 1;
                inventory.ids.extend(ids);
            }
            Err(_) => inventory.missing += 1,
        }
    }
    inventory
}

struct Process(HANDLE);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl Process {
    fn open(id: u32) -> Option<Self> {
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, id) }
            .ok()
            .map(Self)
    }

    fn private_bytes(&self) -> Option<u64> {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        unsafe {
            GetProcessMemoryInfo(
                self.0,
                std::ptr::from_mut(&mut counters).cast(),
                counters.cb,
            )
            .ok()?;
        }
        Some(counters.PrivateUsage as u64)
    }

    fn cpu_ticks(&self) -> Option<u64> {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user) }
            .ok()?;
        let ticks =
            |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        Some(ticks(kernel).saturating_add(ticks(user)))
    }
}

pub fn sample(inventory: Inventory, interval: Option<Duration>) -> Sample {
    let handles: Vec<_> = inventory
        .ids
        .iter()
        .copied()
        .filter_map(Process::open)
        .collect();
    let mut result = Sample {
        process_count: (inventory.complete > 0).then_some(inventory.ids.len()),
        partial_inventory: inventory.missing > 0,
        ..Sample::default()
    };
    for bytes in handles.iter().filter_map(Process::private_bytes) {
        result.private_bytes = Some(result.private_bytes.unwrap_or(0).saturating_add(bytes));
        result.memory_coverage += 1;
    }
    if let Some(interval) = interval.filter(|_| !handles.is_empty()) {
        let start = Instant::now();
        let before: Vec<_> = handles.iter().map(Process::cpu_ticks).collect();
        std::thread::sleep(interval);
        let ticks: u64 = handles
            .iter()
            .zip(before)
            .filter_map(|(handle, before)| {
                let delta = handle.cpu_ticks()?.checked_sub(before?)?;
                result.cpu_coverage += 1;
                Some(delta)
            })
            .sum();
        let processors = std::thread::available_parallelism().map_or(1, |count| count.get());
        if result.cpu_coverage != 0 {
            result.cpu_percent = Some(
                (ticks as f64 / (start.elapsed().as_secs_f64() * processors as f64 * 100_000.0))
                    .min(100.0),
            );
        }
    }
    result
}
