//! COM integrations absent from Wry's portable API. All callbacks run on the UI STA.
use super::AppEvent;
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use tao::event_loop::EventLoopProxy;
use webview2_com::{
    take_pwstr, DownloadStartingEventHandler, Microsoft::Web::WebView2::Win32::*,
    StateChangedEventHandler,
};
use windows::core::{Interface, PWSTR};
use wry::WebViewExtWindows;
static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(1);

pub fn configure(
    view: &wry::WebView,
    proxy: EventLoopProxy<AppEvent>,
    fullscreen: Rc<Cell<bool>>,
) -> windows::core::Result<Rc<Cell<usize>>> {
    install_shortcuts(view, proxy.clone(), fullscreen)?;
    let active = Rc::new(Cell::new(0usize));
    let pending = active.clone();
    // Fail view creation if these settings cannot be applied. Google's vault is
    // not replaced by WebView2's separate local password/autofill store.
    unsafe {
        let core = view.webview();
        let base = core.Settings()?;
        base.SetAreHostObjectsAllowed(false)?;
        base.SetIsWebMessageEnabled(false)?;
        base.SetAreDevToolsEnabled(false)?;
        let reputation: ICoreWebView2Settings8 = base.cast()?;
        reputation.SetIsReputationCheckingRequired(true)?;
        let settings: ICoreWebView2Settings4 = base.cast()?;
        settings.SetIsPasswordAutosaveEnabled(false)?;
        settings.SetIsGeneralAutofillEnabled(false)?;
        let downloads: ICoreWebView2_4 = core.cast()?;
        downloads.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                // Wry's default handler sets Handled=true. Restore the native UI;
                // WebView2 owns filename selection, conflicts, progress and cancel.
                args.SetHandled(false)?;
                let operation = args.DownloadOperation()?;
                let mut uri = PWSTR::null();
                operation.Uri(&mut uri)?;
                let url = take_pwstr(uri);
                let id = NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed);
                let mut path = PWSTR::null();
                args.ResultFilePath(&mut path)?;
                let file = take_pwstr(path);
                let events = proxy.clone();
                let count = pending.clone();
                let in_progress = Cell::new(true);
                let mut token = 0;
                operation.add_StateChanged(
                    &StateChangedEventHandler::create(Box::new(move |op, _| {
                        let Some(op) = op else {
                            return Ok(());
                        };
                        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                        op.State(&mut state)?;
                        let running = state == COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
                        if running != in_progress.replace(running) {
                            if running {
                                count.set(count.get() + 1);
                            } else {
                                count.set(count.get().saturating_sub(1));
                            }
                        }
                        let mut actual_path = PWSTR::null();
                        op.ResultFilePath(&mut actual_path)?;
                        let file = PathBuf::from(take_pwstr(actual_path));
                        let _ = events.send_event(AppEvent::DownloadState {
                            id,
                            file,
                            running,
                            ok: state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
                        });
                        Ok(())
                    })),
                    &mut token,
                )?;
                pending.set(pending.get() + 1);
                let _ = proxy.send_event(AppEvent::DownloadStarted { id, url, file });
                Ok(())
            })),
            &mut 0,
        )?;
    }
    Ok(active)
}

/// Native accelerator events work while a site, not the toolbar, owns focus.
/// Sites retain no access to privileged IPC.
pub fn install_shortcuts(
    view: &wry::WebView,
    proxy: EventLoopProxy<AppEvent>,
    fullscreen: Rc<Cell<bool>>,
) -> windows::core::Result<()> {
    unsafe {
        view.controller().add_AcceleratorKeyPressed(
            &webview2_com::AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut key = 0;
                args.VirtualKey(&mut key)?;
                let control = windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) < 0;
                let command = match key {
                    0x54 if control => Some(super::UiCommand::NewTab),
                    0x7a => Some(super::UiCommand::ToggleFullscreen), // F11
                    0x1b if fullscreen.get() => Some(super::UiCommand::ExitFullscreen),
                    _ => None,
                };
                if let Some(command) = command {
                    args.SetHandled(true)?;
                    let mut physical = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                    args.PhysicalKeyStatus(&mut physical)?;
                    if !physical.WasKeyDown.as_bool() {
                        let _ = proxy.send_event(AppEvent::Ui(command));
                    }
                }
                Ok(())
            })),
            &mut 0,
        )?;
    }
    Ok(())
}
