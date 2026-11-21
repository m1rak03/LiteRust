//! Explicit CI-only WebView2 storage test with synthetic data and loopback origin.
use std::{
    io::Write,
    time::{Duration, Instant},
};
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::run_return::EventLoopExtRunReturn,
    window::WindowBuilder,
};
use wry::{PageLoadEvent, WebContext, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};
#[derive(Debug)]
enum TestEvent {
    Loaded,
    Result(String),
}
pub fn run(phase: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (id, private, write, expected) = match phase {
        "write-a" => ("a", false, true, "alpha"),
        "read-a" => ("a", false, false, "alpha"),
        "empty-b" => ("b", false, false, ""),
        "write-b" => ("b", false, true, "beta"),
        "read-b" => ("b", false, false, "beta"),
        "empty-private" => ("a", true, false, ""),
        "write-private" => ("a", true, true, "gamma"),
        _ => return Err("Unknown session test phase".into()),
    };
    let root = if let Some(path) = std::env::var_os("LITERUST_SESSION_TEST_ROOT") {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA missing")?)
            .join("LiteRustSessionTest")
    };
    std::fs::create_dir_all(&root)?;
    let mut events = EventLoopBuilder::<TestEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("LiteRust storage self-test")
        .with_visible(false)
        .build(&events)?;
    let mut context = WebContext::new(Some(root.join(id)));
    let proxy = events.create_proxy();
    let titles = proxy.clone();
    // Exercise InPrivate against the same environment that contains profile A.
    let anchor = if private {
        Some(
            WebViewBuilder::new_with_web_context(&mut context)
                .with_visible(false)
                .build_as_child(&window)?,
        )
    } else {
        None
    };
    let builder = match &anchor {
        Some(anchor) => WebViewBuilder::new()
            .with_incognito(true)
            .with_environment(anchor.environment()),
        None => WebViewBuilder::new_with_web_context(&mut context),
    };
    let view = builder
        .with_url("http://127.0.0.1:37643/")
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = proxy.send_event(TestEvent::Loaded);
            }
        })
        .with_document_title_changed_handler(move |title| {
            if let Some(value) = title.strip_prefix("TEST:") {
                let _ = titles.send_event(TestEvent::Result(value.to_string()));
            }
        })
        .build_as_child(&window)?;
    if private {
        super::windows_platform::verify_private(&view)?;
    }
    let expected = serde_json::to_string(expected)?;
    let script = format!(
        r#"(()=>{{const writing={write},expected={expected};if(writing){{localStorage.setItem('fixture',expected);document.cookie='fixture='+expected+'; Max-Age=86400; Path=/; SameSite=Lax';}}const req=indexedDB.open('fixture',1);req.onupgradeneeded=()=>req.result.createObjectStore('values');req.onerror=()=>document.title='TEST:FAIL';req.onsuccess=()=>{{const db=req.result,tx=db.transaction('values',writing?'readwrite':'readonly'),store=tx.objectStore('values');if(writing)store.put(expected,'fixture');const read=store.get('fixture');let value='';read.onsuccess=()=>value=read.result||'';tx.oncomplete=()=>{{const cookie=document.cookie.split('; ').find(c=>c.startsWith('fixture='))?.slice(8)||'';const ok=(localStorage.getItem('fixture')||'')===expected&&cookie===expected&&value===expected;db.close();document.title='TEST:'+(ok?'PASS':'FAIL');}};tx.onerror=()=>document.title='TEST:FAIL';}};}})();"#
    );
    let started = Instant::now();
    let mut sent = false;
    let mut result = None;
    let mut settled = None;
    events.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        match event {
            Event::UserEvent(TestEvent::Loaded) if !sent => {
                sent = true;
                if view.evaluate_script(&script).is_err() {
                    result = Some(false);
                    *flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(TestEvent::Result(value)) => {
                result = Some(value == "PASS");
                settled = Some(Instant::now());
            }
            _ => {}
        }
        if started.elapsed() > Duration::from_secs(30)
            || settled.is_some_and(|time: Instant| time.elapsed() > Duration::from_secs(2))
        {
            *flow = ControlFlow::Exit;
        }
    });
    drop(view);
    drop(anchor);
    drop(context);
    let pass = result == Some(true);
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("results.log"))?;
    writeln!(log, "{phase}: {}", if pass { "PASS" } else { "FAIL" })?;
    if pass {
        Ok(())
    } else {
        Err("WebView2 cookie/localStorage/IndexedDB check failed".into())
    }
}

pub fn downloads() -> Result<(), Box<dyn std::error::Error>> {
    use super::{windows_platform, AppEvent, BrowserProxy, WindowMessage};
    use literust_browser::storage::{DownloadAction, DownloadStatus};
    use std::{cell::RefCell, rc::Rc};

    let root = std::path::PathBuf::from(
        std::env::var_os("LITERUST_SESSION_TEST_ROOT").ok_or("Download test root missing")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Download test root must be an existing absolute directory".into());
    }
    let port = std::env::var("LITERUST_DOWNLOAD_TEST_PORT")?.parse::<u16>()?;
    let mut log = std::fs::File::create(root.join("results.log"))?;
    let mut events = EventLoopBuilder::<WindowMessage>::with_user_event().build();
    let window = WindowBuilder::new().with_visible(false).build(&events)?;
    let proxy = BrowserProxy {
        window: window.id(),
        event_loop: events.create_proxy(),
    };
    let mut context = WebContext::new(Some(root.join("WebView2")));
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_visible(false)
        .build_as_child(&window)?;
    // The download fixture may only write inside its own directory.
    let download_dir = root.join("files");
    std::fs::create_dir(&download_dir)?;
    let downloads = windows_platform::Downloads::default();
    let active = windows_platform::configure(
        &view,
        proxy,
        Rc::new(RefCell::new(Default::default())),
        &downloads,
    )?;
    unsafe {
        use webview2_com::{
            take_pwstr, DownloadStartingEventHandler,
            Microsoft::Web::WebView2::Win32::ICoreWebView2_4,
        };
        use windows::core::{Interface, HSTRING, PWSTR};
        let directory = download_dir.clone();
        let core: ICoreWebView2_4 = view.webview().cast()?;
        core.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.DownloadOperation()?.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                let file = if uri.ends_with("/complete") {
                    "complete.bin"
                } else {
                    "cancel.bin"
                };
                args.SetCancel(true)?;
                args.SetResultFilePath(&HSTRING::from(directory.join(file).as_os_str()))?;
                args.SetHandled(true)?;
                args.SetCancel(false)
            })),
            &mut 0,
        )?;
    }
    view.load_url(&format!("http://127.0.0.1:{port}/complete"))?;

    let started = Instant::now();
    let mut result = Err("Native download fixture timed out");
    let mut phase = 0;
    events.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(started + Duration::from_secs(30));
        if started.elapsed() >= Duration::from_secs(30) {
            *flow = ControlFlow::Exit;
            return;
        }
        let Event::UserEvent(WindowMessage::Browser { event, .. }) = event else {
            return;
        };
        if let AppEvent::DownloadState {
            id,
            status,
            progress,
            ..
        } = &event
        {
            let _ = writeln!(
                log,
                "transition: {id} {status:?} {progress:?}, reason={:?}",
                downloads.interrupt_reason(*id)
            );
        }
        let next = match event {
            AppEvent::DownloadStarted { id, .. } if phase == 0 => {
                downloads.control(id, DownloadAction::Pause)
            }
            AppEvent::DownloadState {
                id,
                status: DownloadStatus::Paused,
                progress,
                ..
            } if phase == 1 => progress.can_resume && downloads.control(id, DownloadAction::Resume),
            AppEvent::DownloadState {
                status: DownloadStatus::Running,
                ..
            } if phase == 2 => true,
            AppEvent::DownloadState {
                id,
                file,
                status: DownloadStatus::Completed,
                progress,
                ..
            } if phase == 3 => {
                let _ = writeln!(
                    log,
                    "completed: active={}, retained={}, received={}, total={:?}, file={}",
                    active.get(),
                    downloads.contains(id),
                    progress.received,
                    progress.total,
                    file.is_file()
                );
                !downloads.contains(id)
                    && active.get() == 0
                    && progress.total == Some(progress.received)
                    && progress.received > 0
                    && file.starts_with(&download_dir)
                    && file.is_file()
                    && view
                        .load_url(&format!("http://127.0.0.1:{port}/cancel"))
                        .is_ok()
            }
            AppEvent::DownloadStarted { id, .. } if phase == 4 => {
                downloads.control(id, DownloadAction::Cancel)
            }
            AppEvent::DownloadState {
                id,
                status: DownloadStatus::Cancelled,
                ..
            } if phase == 5 => {
                if active.get() == 0 && !downloads.contains(id) {
                    result = Ok(());
                }
                *flow = ControlFlow::Exit;
                true
            }
            _ => false,
        };
        let _ = writeln!(log, "phase {phase}: {next}");
        if !next {
            result = Err("Unexpected native download transition");
            *flow = ControlFlow::Exit;
        }
        phase += 1;
    });
    result?;
    writeln!(
        log,
        "PASS: native pause, resume, completion, cancellation and callback cleanup"
    )?;
    Ok(())
}

pub fn split(private_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    use super::{create_browser, dispatch_browser_event, WindowMessage, WindowOptions};
    use literust_browser::{
        local_profiles::Catalog,
        split_view::{Axis, Operation, Pane},
        storage::{BrowserData, Tab},
        tab_groups::Color,
    };
    use std::sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    };

    let root = std::path::PathBuf::from(
        std::env::var_os("LITERUST_SESSION_TEST_ROOT").ok_or("Split test root missing")?,
    );
    if !root.is_absolute() || !root.is_dir() || !super::profile_root().starts_with(&root) {
        return Err("Split test requires isolated fixture profile storage".into());
    }
    let port = std::env::var("LITERUST_SPLIT_TEST_PORT")?.parse::<u16>()?;
    let urls = ["one", "two", "three"].map(|name| format!("http://127.0.0.1:{port}/{name}"));
    let mut log = std::fs::File::create(root.join("results.log"))?;
    let mut events = EventLoopBuilder::<WindowMessage>::with_user_event().build();
    let mut data = BrowserData {
        tabs: vec![Tab::web(&urls[0]), Tab::web(&urls[1])],
        ..BrowserData::default()
    }
    .clean();
    data.account_prompt_seen = true;
    data.auto_discard = true;
    data.live_tab_budget = 1;
    let ids = [data.tabs[0].id, data.tabs[1].id];
    assert!(data.tab_groups.create(
        &mut data.tabs,
        &mut data.active,
        &ids,
        "Fixture",
        Color::Teal
    ));
    let options = WindowOptions {
        private_mode,
        downloads: Default::default(),
        local_profiles: Catalog::load(&super::app_root())?,
        vpn_vault: Default::default(),
        vpn_port: None,
        vpn_running: false,
        system_vpn_active: false,
    };
    let mut browser = create_browser(&events, &events.create_proxy(), data, options, None)?;
    let started = Instant::now();
    let mut stage = 0;
    let mut since = started;
    let mut result = Err("Split test timed out");
    let scroll = [Arc::new(AtomicI32::new(-1)), Arc::new(AtomicI32::new(-1))];
    let loaded = |browser: &super::Browser, id: u64, url: &str| {
        browser.pages.get(&id).is_some_and(|page| {
            !page.loading && page.view.url().is_ok_and(|current| current == url)
        })
    };

    events.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
        if started.elapsed() >= Duration::from_secs(40) {
            *flow = ControlFlow::Exit;
            return;
        }
        if let Event::UserEvent(WindowMessage::Browser { event, .. }) = event {
            dispatch_browser_event(&mut browser, event);
        }
        if !browser.ready || since.elapsed() < Duration::from_millis(500) {
            return;
        }
        let mut test_stage = || -> Result<bool, &'static str> {
            match stage {
                0 if loaded(&browser, ids[0], &urls[0]) => {
                    log_runtime_memory(&browser.pages[&ids[0]].view, "single", &mut log);
                    browser.run_split(Operation::Open { tab: ids[1] });
                }
                1 if loaded(&browser, ids[1], &urls[1]) => {
                    log_runtime_memory(&browser.pages[&ids[0]].view, "split", &mut log);
                    if browser.pages.len() != 2 || browser.data.split.is_none() {
                        return Err(
                            "The split did not retain both WebViews under the 1-page budget",
                        );
                    }
                    for id in ids {
                        let mut visible = windows::core::BOOL::default();
                        let view = &browser.pages[&id].view;
                        if unsafe { view.controller().IsVisible(&mut visible) }.is_err()
                            || !visible.as_bool()
                        {
                            return Err("A split WebView is not visible");
                        }
                        if private_mode && super::windows_platform::verify_private(view).is_err() {
                            return Err("A pane left the InPrivate profile");
                        }
                    }
                    browser.run_split(Operation::Axis {
                        axis: Axis::Vertical,
                    });
                    browser.run_split(Operation::Resize { percent: 33 });
                    let first = browser.pages[&ids[0]]
                        .view
                        .bounds()
                        .map_err(|_| "Read first bounds")?;
                    let second = browser.pages[&ids[1]]
                        .view
                        .bounds()
                        .map_err(|_| "Read second bounds")?;
                    if first.position.to_physical::<i32>(1.0).y
                        >= second.position.to_physical::<i32>(1.0).y
                        || first.size.to_physical::<u32>(1.0).height
                            >= second.size.to_physical::<u32>(1.0).height
                    {
                        return Err(
                            "Vertical split or resize did not change the controller bounds",
                        );
                    }
                    browser.pages[&ids[1]]
                        .view
                        .focus()
                        .map_err(|_| "Focus second pane")?;
                    browser.pages[&ids[1]]
                        .view
                        .evaluate_script("window.scrollTo(0,900)")
                        .map_err(|_| "Scroll the second WebView")?;
                }
                2 if browser.active_id() == ids[1] => {
                    for (id, value) in ids.into_iter().zip(&scroll) {
                        let value = value.clone();
                        browser.pages[&id]
                            .view
                            .evaluate_script_with_callback("window.scrollY", move |json| {
                                value.store(json.parse::<i32>().unwrap_or(-1), Ordering::Relaxed);
                            })
                            .map_err(|_| "Read independent scroll state")?;
                    }
                    return Ok(true);
                }
                3 => {
                    if scroll[0].load(Ordering::Relaxed) != 0
                        || scroll[1].load(Ordering::Relaxed) < 600
                    {
                        return Err(
                            "Scrolling the second pane affected the first or did not occur",
                        );
                    }
                    browser.run_split(Operation::Navigate {
                        pane: Pane::Second,
                        value: urls[2].clone(),
                    });
                }
                4 if loaded(&browser, ids[1], &urls[2]) => {
                    if !loaded(&browser, ids[0], &urls[0]) {
                        return Err("Navigating the second pane affected the first");
                    }
                    browser.run_split(Operation::Swap);
                    let snapshot = browser.session();
                    if !snapshot.split.is_some_and(|split| {
                        split.panes == [ids[1], ids[0]] && split.focus == Pane::First
                    }) || snapshot.tab_groups.entries().len() != 1
                    {
                        return Err("Swap or session snapshot lost the selected tab or group");
                    }
                    browser.run_split(Operation::Close { pane: Pane::First });
                    if browser.data.split.is_some()
                        || browser.active_id() != ids[0]
                        || browser.pages.contains_key(&ids[1])
                        || browser.data.tabs.len() != 2
                    {
                        return Err(
                            "Closing a pane did not release the WebView and preserve its tab",
                        );
                    }
                    return Ok(true);
                }
                _ => return Ok(false),
            }
            Ok(true)
        };
        match test_stage() {
            Ok(true) => {
                let _ = writeln!(log, "phase {stage}: true");
                stage += 1;
                since = Instant::now();
                if stage == 5 {
                    result = Ok(());
                    *flow = ControlFlow::Exit;
                }
            }
            Ok(false) => {}
            Err(reason) => {
                let _ = writeln!(log, "phase {stage}: {reason}");
                result = Err(reason);
                *flow = ControlFlow::Exit;
            }
        }
    });
    result?;
    writeln!(log,"PASS: separate native WebViews, navigation, scroll, resize, layout, swap, closure and group state")?;
    Ok(())
}

fn log_runtime_memory(view: &wry::WebView, label: &str, log: &mut std::fs::File) {
    use std::collections::BTreeSet;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment8;
    use windows::{
        core::Interface,
        Win32::{
            Foundation::CloseHandle,
            System::{
                ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
                Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
            },
        },
    };
    let sample = || -> windows::core::Result<_> {
        unsafe {
            let environment: ICoreWebView2Environment8 = view.environment().cast()?;
            let processes = environment.GetProcessInfos()?;
            let mut count = 0;
            processes.Count(&mut count)?;
            let mut pids = BTreeSet::new();
            for index in 0..count {
                let mut id = 0;
                processes.GetValueAtIndex(index)?.ProcessId(&mut id)?;
                if id > 0 {
                    pids.insert(id as u32);
                }
            }
            let mut sampled = 0;
            let mut bytes = 0usize;
            for &id in &pids {
                let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, id) else {
                    continue;
                };
                let mut counters = PROCESS_MEMORY_COUNTERS_EX {
                    cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                    ..Default::default()
                };
                if GetProcessMemoryInfo(
                    handle,
                    std::ptr::from_mut(&mut counters).cast(),
                    counters.cb,
                )
                .is_ok()
                {
                    sampled += 1;
                    bytes = bytes.saturating_add(counters.PrivateUsage);
                }
                let _ = CloseHandle(handle);
            }
            Ok((pids.len(), sampled, bytes))
        }
    };
    if let Ok((pids, sampled, bytes)) = sample() {
        let _ = writeln!(
            log,
            "memory {label}: pids={pids} read={sampled} private_bytes={bytes}"
        );
    } else {
        let _ = writeln!(log, "memory {label}: unavailable");
    }
}
