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
    shortcuts: Rc<std::cell::RefCell<literust_browser::shortcuts::Runtime>>,
) -> windows::core::Result<Rc<Cell<usize>>> {
    install_shortcuts(view, proxy.clone(), shortcuts)?;
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
                // Let WebView2 own filename selection, conflicts, progress and cancellation.
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
                let token = Rc::new(Cell::new(0));
                let callback_token = token.clone();
                let mut registration = 0;
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
                        if !running {
                            let mut can_resume = windows::core::BOOL::default();
                            if op.CanResume(&mut can_resume).is_ok() && !can_resume.as_bool() {
                                let _ = op.remove_StateChanged(callback_token.get());
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
                    &mut registration,
                )?;
                token.set(registration);
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
    shortcuts: Rc<std::cell::RefCell<literust_browser::shortcuts::Runtime>>,
) -> windows::core::Result<()> {
    unsafe {
        let settings: ICoreWebView2Settings3 = view.webview().Settings()?.cast()?;
        settings.SetAreBrowserAcceleratorKeysEnabled(false)?;
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
                let runtime = shortcuts.borrow();
                if runtime.recording {
                    return Ok(());
                }
                use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
                let chord = literust_browser::shortcuts::chord_from_vk(
                    key,
                    GetKeyState(0x11) < 0,
                    GetKeyState(0x12) < 0,
                    GetKeyState(0x10) < 0,
                );
                let command = chord
                    .as_deref()
                    .and_then(|c| runtime.bindings.action(c))
                    .map(|name| super::UiCommand::Shortcut {
                        name: name.to_owned(),
                    });
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

pub fn find(view: &wry::WebView) -> windows::core::Result<()> {
    unsafe {
        let core: ICoreWebView2_28 = view.webview().cast()?;
        let environment: ICoreWebView2Environment15 = view.environment().cast()?;
        let options = environment.CreateFindOptions()?;
        options.SetSuppressDefaultFindDialog(false)?;
        core.Find()?.Start(
            &options,
            &webview2_com::FindStartCompletedHandler::create(Box::new(|_| Ok(()))),
        )?;
    }
    Ok(())
}

pub fn raise(view: &wry::WebView) -> windows::core::Result<()> {
    unsafe {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            },
        };
        let mut hwnd = HWND::default();
        view.controller().ParentWindow(&mut hwnd)?;
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )?;
    }
    Ok(())
}

pub fn color_scheme(view: &wry::WebView, dark: bool) -> windows::core::Result<()> {
    unsafe {
        let core: ICoreWebView2_13 = view.webview().cast()?;
        core.Profile()?.SetPreferredColorScheme(if dark {
            COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
        } else {
            COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
        })
    }
}

pub fn print_preview(view: &wry::WebView) -> windows::core::Result<()> {
    unsafe {
        let core: ICoreWebView2_16 = view.webview().cast()?;
        core.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER)
    }
}

pub fn choose_pdf(view: &wry::WebView) -> windows::core::Result<Option<PathBuf>> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HWND,
            UI::{
                Controls::Dialogs::*,
                WindowsAndMessaging::{GetAncestor, GA_ROOT},
            },
        },
    };
    let mut owner = HWND::default();
    unsafe {
        view.controller().ParentWindow(&mut owner)?;
        let root = GetAncestor(owner, GA_ROOT);
        if !root.is_invalid() {
            owner = root;
        }
    }
    let mut filename = [0u16; 4096];
    let default_name: Vec<u16> = "LiteRust.pdf".encode_utf16().collect();
    filename[..default_name.len()].copy_from_slice(&default_name);
    let filter: Vec<u16> = "PDF (*.pdf)\0*.pdf\0\0".encode_utf16().collect();
    let options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(filename.as_mut_ptr()),
        nMaxFile: filename.len() as u32,
        lpstrDefExt: windows::core::w!("pdf"),
        Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    let mut options = options;
    if !unsafe { GetSaveFileNameW(&mut options) }.as_bool() {
        let error = unsafe { CommDlgExtendedError() };
        if error.0 != 0 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                format!("Save dialog: {}", error.0),
            ));
        }
        return Ok(None);
    }
    let len = filename
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(filename.len());
    Ok(Some(PathBuf::from(OsString::from_wide(&filename[..len]))))
}

pub struct PdfOptions<'a> {
    pub header: &'a str,
    pub footer: &'a str,
    pub captions: bool,
    pub backgrounds: bool,
}

pub fn print_pdf(
    view: &wry::WebView,
    path: PathBuf,
    options: PdfOptions<'_>,
    busy: Rc<Cell<bool>>,
    proxy: EventLoopProxy<AppEvent>,
) -> windows::core::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{HSTRING, PCWSTR};

    unsafe {
        let core: ICoreWebView2_7 = view.webview().cast()?;
        let environment: ICoreWebView2Environment6 = view.environment().cast()?;
        let settings = environment.CreatePrintSettings()?;
        settings.SetScaleFactor(1.0)?;
        settings.SetShouldPrintBackgrounds(options.backgrounds)?;
        settings.SetShouldPrintHeaderAndFooter(options.captions)?;
        if options.captions {
            settings.SetHeaderTitle(&HSTRING::from(options.header))?;
            settings.SetFooterUri(&HSTRING::from(options.footer))?;
        }
        let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let printing = busy.clone();
        let callback =
            webview2_com::PrintToPdfCompletedHandler::create(Box::new(move |result, printed| {
                printing.set(false);
                let _ = proxy.send_event(AppEvent::PdfFinished {
                    path,
                    success: result.is_ok() && printed,
                });
                Ok(())
            }));
        busy.set(true);
        let result = core.PrintToPdf(PCWSTR(destination.as_ptr()), &settings, &callback);
        if result.is_err() {
            busy.set(false);
        }
        result
    }
}

pub fn open_certificates() -> std::io::Result<()> {
    use literust_browser::security::windows_system_binary;
    std::process::Command::new(windows_system_binary("mmc.exe")?)
        .arg(windows_system_binary("certmgr.msc")?)
        .spawn()?;
    Ok(())
}
