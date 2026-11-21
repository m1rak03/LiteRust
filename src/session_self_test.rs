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
