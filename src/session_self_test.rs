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
use wry::{PageLoadEvent, WebContext, WebViewBuilder};
#[derive(Debug)]
enum TestEvent {
    Loaded,
    Result(String),
}
pub fn run(phase: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (id, write, expected) = match phase {
        "write-a" => ("a", true, "alpha"),
        "read-a" => ("a", false, "alpha"),
        "empty-b" => ("b", false, ""),
        "write-b" => ("b", true, "beta"),
        "read-b" => ("b", false, "beta"),
        _ => return Err("Unknown session test phase".into()),
    };
    let root =
        std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA missing")?)
            .join("LiteRustSessionTest");
    std::fs::create_dir_all(&root)?;
    let mut events = EventLoopBuilder::<TestEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("LiteRust storage self-test")
        .build(&events)?;
    let mut context = WebContext::new(Some(root.join(id)));
    let proxy = events.create_proxy();
    let titles = proxy.clone();
    let view = WebViewBuilder::new_with_web_context(&mut context)
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
        .build(&window)?;
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
