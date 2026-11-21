//! COM integrations absent from Wry's portable API. All callbacks run on the UI STA.
use super::{AppEvent, BrowserProxy};
use literust_browser::storage::{DownloadAction, DownloadProgress, DownloadStatus};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use webview2_com::{
    take_pwstr, DownloadStartingEventHandler, Microsoft::Web::WebView2::Win32::*,
    StateChangedEventHandler,
};
use windows::core::{Interface, PWSTR};
use wry::WebViewExtWindows;
static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct Downloads(Rc<RefCell<HashMap<u64, ICoreWebView2DownloadOperation>>>);

impl Downloads {
    pub fn reserve_ids(count: usize) {
        NEXT_DOWNLOAD.fetch_max(count as u64 + 1, Ordering::Relaxed);
    }

    fn operation(&self, id: u64) -> Option<ICoreWebView2DownloadOperation> {
        self.0.borrow().get(&id).cloned()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.0.borrow().contains_key(&id)
    }

    pub fn progress(&self, id: u64) -> Option<DownloadProgress> {
        observation(&self.operation(id)?)
            .ok()
            .map(|(_, progress)| progress)
    }

    pub(super) fn interrupt_reason(&self, id: u64) -> Option<i32> {
        let op = self.operation(id)?;
        let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
        unsafe { op.InterruptReason(&mut reason).ok()? };
        Some(reason.0)
    }

    pub fn control(&self, id: u64, action: DownloadAction) -> bool {
        let Some(op) = self.operation(id) else {
            return false;
        };
        let Ok((_, progress)) = observation(&op) else {
            return false;
        };
        unsafe {
            match action {
                DownloadAction::Pause if progress.can_pause => op.Pause().is_ok(),
                DownloadAction::Resume if progress.can_resume => op.Resume().is_ok(),
                DownloadAction::Cancel if progress.can_cancel => op.Cancel().is_ok(),
                _ => false,
            }
        }
    }
}

fn observation(
    op: &ICoreWebView2DownloadOperation,
) -> windows::core::Result<(DownloadStatus, DownloadProgress)> {
    let mut raw_state = COREWEBVIEW2_DOWNLOAD_STATE::default();
    let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
    let mut resumable = windows::core::BOOL::default();
    let (mut received, mut total) = (0, -1);
    unsafe {
        op.State(&mut raw_state)?;
        let _ = op.BytesReceived(&mut received);
        let _ = op.TotalBytesToReceive(&mut total);
        if raw_state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
            let _ = op.InterruptReason(&mut reason);
            let _ = op.CanResume(&mut resumable);
        }
    }
    let status = match raw_state {
        COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS => DownloadStatus::Running,
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => DownloadStatus::Completed,
        _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED => DownloadStatus::Paused,
        _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED => {
            DownloadStatus::Cancelled
        }
        _ => DownloadStatus::Interrupted,
    };
    let can_pause = status == DownloadStatus::Running;
    let can_resume = resumable.as_bool();
    Ok((
        status,
        DownloadProgress {
            received: received.max(0) as u64,
            total: u64::try_from(total).ok(),
            can_pause,
            can_resume,
            can_cancel: can_pause || can_resume || status == DownloadStatus::Paused,
        },
    ))
}

pub fn open_download_folder(hwnd: windows::Win32::Foundation::HWND, file: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let Some(directory) = file
        .parent()
        .filter(|path| path.is_absolute() && path.is_dir())
    else {
        return false;
    };
    let mut path: Vec<u16> = directory.as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return false;
    }
    path.push(0);
    // Pass the path as a Shell item, never through a command interpreter.
    let result = unsafe {
        ShellExecuteW(
            Some(hwnd),
            w!("explore"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

pub fn configure(
    view: &wry::WebView,
    proxy: BrowserProxy,
    shortcuts: Rc<RefCell<literust_browser::shortcuts::Runtime>>,
    downloads: &Downloads,
) -> windows::core::Result<Rc<Cell<usize>>> {
    install_shortcuts(view, proxy.clone(), shortcuts)?;
    let active = Rc::new(Cell::new(0usize));
    let pending = active.clone();
    let registry = Rc::downgrade(&downloads.0);
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
                let Some(operations) = registry.upgrade() else {
                    return Ok(());
                };
                if operations.borrow().len() >= literust_browser::storage::MAX_DOWNLOADS {
                    args.SetCancel(true)?;
                    let _ = proxy.send_event(AppEvent::DownloadLimit);
                    return Ok(());
                }
                // Keep WebView2's native filename selection and security checks.
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
                let weak = registry.clone();
                let token = Rc::new(Cell::new(0));
                let callback_token = token.clone();
                let mut registration = 0;
                operation.add_StateChanged(
                    &StateChangedEventHandler::create(Box::new(move |op, _| {
                        let Some(op) = op else { return Ok(()) };
                        let (status, progress) = observation(&op)?;
                        if !progress.can_cancel {
                            let _ = op.remove_StateChanged(callback_token.get());
                            if let Some(operations) = weak.upgrade() {
                                if operations.borrow_mut().remove(&id).is_some() {
                                    count.set(count.get().saturating_sub(1));
                                }
                            }
                        }
                        let mut path = PWSTR::null();
                        let _ = op.ResultFilePath(&mut path);
                        let file = PathBuf::from(take_pwstr(path));
                        let _ = events.send_event(AppEvent::DownloadState {
                            id,
                            file,
                            status,
                            progress,
                        });
                        Ok(())
                    })),
                    &mut registration,
                )?;
                token.set(registration);
                operations.borrow_mut().insert(id, operation);
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
    proxy: BrowserProxy,
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

pub fn has_page_focus(view: &wry::WebView) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::{Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::IsChild},
    };
    unsafe {
        let focused = GetFocus();
        let mut parent = HWND::default();
        !focused.is_invalid()
            && view.controller().ParentWindow(&mut parent).is_ok()
            && (focused == parent || IsChild(parent, focused).as_bool())
    }
}

pub fn page_focus(
    view: &wry::WebView,
    proxy: BrowserProxy,
    tab_id: u64,
    generation: u64,
) -> windows::core::Result<()> {
    unsafe {
        view.controller().add_GotFocus(
            &webview2_com::FocusChangedEventHandler::create(Box::new(move |_, _| {
                let _ = proxy.send_event(AppEvent::PageFocused { tab_id, generation });
                Ok(())
            })),
            &mut 0,
        )
    }
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

pub fn verify_private(view: &wry::WebView) -> windows::core::Result<()> {
    unsafe {
        let core: ICoreWebView2_13 = view.webview().cast()?;
        let mut private = windows::core::BOOL::default();
        core.Profile()?.IsInPrivateModeEnabled(&mut private)?;
        if !private.as_bool() {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "WebView2 did not create an InPrivate profile",
            ));
        }
    }
    Ok(())
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
    proxy: BrowserProxy,
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

fn profile_window_property(root: &std::path::Path) -> windows::core::HSTRING {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut hasher = DefaultHasher::new();
    root.to_string_lossy().to_lowercase().hash(&mut hasher);
    windows::core::HSTRING::from(format!("LiteRust.Profile.{:016x}", hasher.finish()))
}

pub fn register_profile_window(
    window: &tao::window::Window,
    root: &std::path::Path,
) -> windows::core::Result<()> {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::{
        Foundation::{HANDLE, HWND},
        UI::WindowsAndMessaging::SetPropW,
    };

    unsafe {
        SetPropW(
            HWND(window.hwnd() as _),
            &profile_window_property(root),
            Some(HANDLE(1 as _)),
        )
    }
}

fn find_profile_window(root: &std::path::Path) -> Option<windows::Win32::Foundation::HWND> {
    use windows::{
        core::{BOOL, HSTRING},
        Win32::{
            Foundation::{HWND, LPARAM},
            UI::WindowsAndMessaging::{EnumWindows, GetPropW},
        },
    };
    struct Search {
        property: HSTRING,
        found: HWND,
    }
    unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        if unsafe { GetPropW(hwnd, &search.property) }.is_invalid() {
            return true.into();
        }
        search.found = hwnd;
        false.into()
    }

    let mut search = Search {
        property: profile_window_property(root),
        found: HWND::default(),
    };
    // EnumWindows invokes the callback synchronously with this stack-owned state.
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(std::ptr::from_mut(&mut search) as isize));
    }
    (!search.found.is_invalid()).then_some(search.found)
}

/// Find the normal window on this desktop. Window markers permit focus only.
pub fn activate_profile_window(root: &std::path::Path) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        FlashWindowEx, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindowAsync, FLASHWINFO,
        FLASHW_TRAY, SW_RESTORE,
    };
    let Some(hwnd) = find_profile_window(root) else {
        return false;
    };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            // Startup is in progress; let Ready expose the window once painted.
            return true;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
        if !SetForegroundWindow(hwnd).as_bool() {
            let _ = FlashWindowEx(&FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dwFlags: FLASHW_TRAY,
                uCount: 3,
                dwTimeout: 0,
            });
        }
        true
    }
}

const NEW_WINDOW_MESSAGE: windows::core::PCWSTR = windows::core::w!("LiteRust.OpenNormalWindow.v1");

pub fn request_profile_window(root: &std::path::Path) -> bool {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW},
    };
    let Some(window) = find_profile_window(root) else {
        return false;
    };
    unsafe {
        let message = RegisterWindowMessageW(NEW_WINDOW_MESSAGE);
        message != 0 && PostMessageW(Some(window), message, WPARAM(0), LPARAM(0)).is_ok()
    }
}

pub fn install_window_request(
    window: &tao::window::Window,
    proxy: BrowserProxy,
) -> windows::core::Result<()> {
    use tao::platform::windows::WindowExtWindows;
    use windows::{
        core::Error,
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            UI::{
                Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
                WindowsAndMessaging::{RegisterWindowMessageW, WM_NCDESTROY},
            },
        },
    };

    struct Relay {
        message: u32,
        proxy: BrowserProxy,
    }
    unsafe extern "system" fn relay(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        let context = data as *mut Relay;
        if message == WM_NCDESTROY {
            // The UI thread owns this context from installation through destruction.
            if unsafe { RemoveWindowSubclass(hwnd, Some(relay), id) }.as_bool() {
                unsafe {
                    drop(Box::from_raw(context));
                }
            }
        } else {
            let context = unsafe { &*context };
            if message == context.message && wparam.0 == 0 && lparam.0 == 0 {
                let _ = context
                    .proxy
                    .send_event(AppEvent::Ui(super::UiCommand::NewWindow));
                return LRESULT(0);
            }
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    let message = unsafe { RegisterWindowMessageW(NEW_WINDOW_MESSAGE) };
    if message == 0 {
        return Err(Error::from_thread());
    }
    let context = Box::into_raw(Box::new(Relay { message, proxy }));
    if !unsafe { SetWindowSubclass(HWND(window.hwnd() as _), Some(relay), 1, context as usize) }
        .as_bool()
    {
        unsafe {
            drop(Box::from_raw(context));
        }
        return Err(Error::from_thread());
    }
    Ok(())
}
