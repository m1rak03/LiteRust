//! Opt-in local fixture for comparing builds using the same real tab operations.
use super::{
    create_browser, dispatch_browser_event, webview_resources, AppEvent, UiCommand, WindowMessage,
    WindowOptions,
};
use literust_browser::{local_profiles::Catalog, storage::BrowserData};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::run_return::EventLoopExtRunReturn,
};

#[derive(Serialize)]
struct Report {
    version: &'static str,
    runtime: String,
    tabs: usize,
    live_tabs: usize,
    cycles: usize,
    elapsed_ms: u128,
    sample: webview_resources::Sample,
    checkpoints: Vec<Checkpoint>,
}

#[derive(Serialize)]
struct Checkpoint {
    cycles: usize,
    elapsed_ms: u128,
    live_tabs: usize,
    sample: webview_resources::Sample,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::var_os("LITERUST_SESSION_TEST_ROOT").ok_or("Benchmark fixture is missing")?,
    );
    if !root.is_absolute() || !root.is_dir() || !super::profile_root().starts_with(&root) {
        return Err("Benchmark requires an isolated fixture".into());
    }
    let count = std::env::var("LITERUST_BENCHMARK_TABS")?.parse::<usize>()?;
    if !(1..=literust_browser::storage::MAX_TABS).contains(&count) {
        return Err(format!(
            "Benchmark supports 1 to {} tabs",
            literust_browser::storage::MAX_TABS
        )
        .into());
    }
    let cycles = std::env::var("LITERUST_BENCHMARK_CYCLES")
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    if cycles > 400 || (cycles > 0 && count < 2) {
        return Err("The churn fixture requires at least two tabs and at most 400 cycles".into());
    }
    let started = Instant::now();
    let mut events = EventLoopBuilder::<WindowMessage>::with_user_event().build();
    let options = WindowOptions {
        private_mode: true,
        downloads: Default::default(),
        local_profiles: Catalog::load(&super::app_root())?,
        vpn_vault: Default::default(),
        vpn_port: None,
        vpn_running: false,
        system_vpn_active: false,
    };
    let data = BrowserData {
        profile: std::rc::Rc::new(literust_browser::storage::ProfileData {
            account_prompt_seen: true,
            ..Default::default()
        }),
        ..Default::default()
    }
    .clean();
    let proxy = events.create_proxy();
    let mut browser = create_browser(&events, &proxy, data, options, None)?;
    let runtime = wry::webview_version()?;
    let mut ready = Arc::new(AtomicU8::new(0));
    let mut last_attempt = Instant::now();
    let deadline = started + Duration::from_secs(45 + cycles as u64 * 2);
    let mut closed_cycles = 0;
    let mut checkpoints = Vec::<Checkpoint>::new();
    let mut fixture_error = None;
    let mut settle_at = None;
    let mut receiver = None;
    let mut sample = None;

    events.run_return(|event, _, flow| {
        let now = Instant::now();
        if now >= deadline {
            *flow = ControlFlow::Exit;
            return;
        }
        *flow = ControlFlow::WaitUntil(now + Duration::from_millis(50));
        if let Event::UserEvent(WindowMessage::Browser { event, .. }) = event {
            let opening_tab = matches!(event, AppEvent::Ui(UiCommand::NewTab));
            let closed = match &event {
                AppEvent::Ui(UiCommand::CloseTab { tab_id, .. }) => *tab_id,
                _ => None,
            };
            dispatch_browser_event(&mut browser, event);
            if let Some(id) = closed {
                if browser.pages.contains_key(&id) || browser.data.tabs.len() + 1 != count {
                    fixture_error = Some("A closed tab retained its live WebView or session entry");
                    *flow = ControlFlow::Exit;
                    return;
                }
                closed_cycles += 1;
            }
            if opening_tab || closed.is_some() {
                ready.store(0, Ordering::Relaxed);
            }
        }
        if let Some(pending) = receiver.as_ref() {
            let pending: &mpsc::Receiver<webview_resources::Sample> = pending;
            if let Ok(measured) = pending.try_recv() {
                if closed_cycles >= cycles {
                    sample = Some(measured);
                    *flow = ControlFlow::Exit;
                } else {
                    checkpoints.push(Checkpoint {
                        cycles: closed_cycles,
                        elapsed_ms: started.elapsed().as_millis(),
                        live_tabs: browser.pages.len(),
                        sample: measured,
                    });
                    receiver = None;
                    settle_at = None;
                }
            } else {
                *flow = ControlFlow::WaitUntil(now + Duration::from_millis(200));
            }
            return;
        }
        if let Some(settle_at) = settle_at {
            if now >= settle_at {
                let inventory = webview_resources::inventory(
                    [&browser.toolbar, &browser.dock]
                        .into_iter().chain(browser.pages.values().map(|page| &page.view)),
                );
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(webview_resources::sample(inventory, Some(Duration::from_secs(2))));
                });
                receiver = Some(rx);
                *flow = ControlFlow::WaitUntil(now + Duration::from_secs(2));
            } else {
                *flow = ControlFlow::WaitUntil(settle_at);
            }
            return;
        }
        if !browser.ready {
            return;
        }
        if ready.load(Ordering::Relaxed) == 1 && last_attempt.elapsed() > Duration::from_secs(1) {
            ready = Arc::new(AtomicU8::new(0));
        }
        match ready.load(Ordering::Relaxed) {
            0 => {
                if let Some(page) = browser.pages.get(&browser.active_id()) {
                    last_attempt = now;
                    ready.store(1, Ordering::Relaxed);
                    let result = ready.clone();
                    if page.view.evaluate_script_with_callback(
                        "document.readyState === 'complete' && document.querySelector('#clock') !== null",
                        move |value| result.store(if value == "true" { 2 } else { 0 }, Ordering::Relaxed),
                    ).is_err() {
                        ready.store(0, Ordering::Relaxed);
                    }
                }
            }
            2 if browser.data.tabs.len() == count => {
                let sample_due = [0, cycles / 2, cycles].contains(&closed_cycles)
                    && checkpoints.last().is_none_or(|entry| entry.cycles != closed_cycles);
                if sample_due {
                    settle_at = Some(now + Duration::from_secs(2));
                } else {
                    ready.store(3, Ordering::Relaxed);
                    let _ = proxy.send_event(WindowMessage::Browser {
                        window: browser.window.id(),
                        event: AppEvent::Ui(UiCommand::CloseTab {
                            index: browser.data.active,
                            tab_id: Some(browser.active_id()),
                        }),
                    });
                }
            }
            2 => {
                // Use the production event path for tab creation. Creating a controller
                // inside MainEventsCleared can re-enter Tao's Windows redraw transition.
                ready.store(3, Ordering::Relaxed);
                let _ = proxy.send_event(WindowMessage::Browser {
                    window: browser.window.id(),
                    event: AppEvent::Ui(UiCommand::NewTab),
                });
            }
            _ => {}
        }
    });
    if let Some(error) = fixture_error {
        return Err(error.into());
    }
    let sample = sample.ok_or_else(|| {
        format!(
            "Benchmark timed out at {} tabs, {} live, readiness {}",
            browser.data.tabs.len(),
            browser.pages.len(),
            ready.load(Ordering::Relaxed)
        )
    })?;
    if browser.pages.len() != count || sample.memory_coverage == 0 {
        return Err(format!(
            "Expected {count} live tabs, got {}. Process sample: {sample:?}",
            browser.pages.len()
        )
        .into());
    }
    let report = Report {
        version: env!("CARGO_PKG_VERSION"),
        runtime,
        tabs: count,
        live_tabs: browser.pages.len(),
        cycles,
        elapsed_ms: started.elapsed().as_millis(),
        sample,
        checkpoints,
    };
    std::fs::write(root.join("tab-load.json"), serde_json::to_vec(&report)?)?;
    Ok(())
}
