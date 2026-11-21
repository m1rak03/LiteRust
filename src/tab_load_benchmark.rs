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
    let deadline = Instant::now() + Duration::from_secs(40);
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
            dispatch_browser_event(&mut browser, event);
            if opening_tab {
                ready.store(0, Ordering::Relaxed);
            }
        }
        if let Some(pending) = receiver.as_ref() {
            let pending: &mpsc::Receiver<webview_resources::Sample> = pending;
            if let Ok(measured) = pending.try_recv() {
                sample = Some(measured);
                *flow = ControlFlow::Exit;
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
                settle_at = Some(now + Duration::from_secs(2));
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
        sample,
    };
    std::fs::write(root.join("tab-load.json"), serde_json::to_vec(&report)?)?;
    Ok(())
}
