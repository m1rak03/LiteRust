//! LiteRust desktop browser shell. Web rendering is handled by Edge WebView2.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use literust_browser::{
    google_passwords, import, layout,
    local_profiles::{self, Catalog, ProfileDetails},
    network_block, security, split_view, storage, tab_groups, vpn, window_session,
};
#[cfg(windows)]
mod session_self_test;
#[cfg(windows)]
mod windows_platform;

#[cfg(test)]
use literust_browser::navigation::destination;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use storage::{
    escape_html, valid_web_url, BrowserData, Tab, BOOKMARKS, DOWNLOADS, HISTORY, MAX_TABS, NEW_TAB,
};
use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize as WebLogicalSize},
    NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, Rect, WebContext,
    WebView, WebViewBuilder,
};
#[cfg(windows)]
use wry::{MemoryUsageLevel, WebViewBuilderExtWindows, WebViewExtWindows};

const TOOLBAR: &str = include_str!("../ui/toolbar.html");
const LIBRARY: &str = include_str!("../ui/library.html");
const COSMETIC_FILTER: &str = include_str!("../ui/cosmetic-filter.js");
const APPEARANCE_SCRIPT: &str = include_str!("../ui/appearance.js");
const DOCK: &str = include_str!("../ui/dock.html");
const TOOLBAR_HEIGHT: f64 = 108.0;
const SPLIT_TOOLBAR_HEIGHT: f64 = 64.0;
// All views sharing a WebView2 environment must use identical proxy options,
// including private toolbar/dock views (whose default context is shared).
fn routed_builder(builder: WebViewBuilder<'_>, port: Option<u16>) -> WebViewBuilder<'_> {
    #[cfg(windows)]
    {
        let mut args = String::from("--disable-gpu --site-per-process");
        if let Some(port) = port {
            args.push(' ');
            args.push_str(&vpn::browser_args(port));
        }
        builder
            .with_devtools(false)
            .with_additional_browser_args(args)
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        builder
    }
}
// Each mode caps the number of live WebViews, not the number of saved tabs.

fn lru_victim(entries: impl IntoIterator<Item = (u64, u64)>, protected_id: u64) -> Option<u64> {
    entries
        .into_iter()
        .filter(|(id, _)| *id != protected_id)
        .min_by_key(|(_, last_used)| *last_used)
        .map(|(id, _)| id)
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum UiCommand {
    SaveHome {
        value: literust_browser::home::Dashboard,
    },
    SetSidebar {
        value: layout::Sidebar,
    },
    DockExpanded {
        open: bool,
        #[serde(default)]
        animate: bool,
        #[serde(default)]
        peek: bool,
    },
    ProfileCreate {
        name: String,
    },
    ProfileSwitch {
        id: String,
    },
    ProfileUpdate {
        id: String,
        name: String,
        avatar: String,
    },
    ProfileDelete {
        id: String,
    },
    ProfileRestore {
        id: String,
    },
    ProfilePurge {
        id: String,
    },
    ProfileDetails,
    ProfileBackup,
    Shortcut {
        name: String,
    },
    SetShortcuts {
        value: literust_browser::shortcuts::Shortcuts,
    },
    ShortcutRecording {
        active: bool,
    },
    VpnScope {
        system: bool,
    },
    SetCustomIcon {
        target: String,
        value: String,
        #[serde(default)]
        rgba: String,
    },
    SetCustomization {
        value: storage::Customization,
    },
    SetBrowserSettings {
        value: storage::BrowserSettings,
    },
    SetBackground {
        value: String,
    },
    SetSearchEngine {
        engine: literust_browser::navigation::SearchEngine,
    },
    Ready,
    WindowMinimize,
    WindowMaximize,
    WindowClose,
    WindowDrag,
    DockReady,
    ShowTools,
    DuplicateTab,
    ShowAccounts,
    SetAccountPrompt {
        always: bool,
    },
    OpenAccount {
        provider: String,
    },
    SetBackgroundStyle {
        value: storage::BackgroundStyle,
    },
    SetNotes {
        value: String,
    },
    PrintPage,
    PrintNative,
    OpenCertificates,
    PrintPdf {
        header: String,
        footer: String,
        captions: bool,
        backgrounds: bool,
    },
    Suggest {
        query: String,
    },
    SuggestionPanel {
        rows: u8,
    },
    ShowSettings,
    ShowSecurity,
    ShowAppearance,
    ShowVpn,
    VpnProbe,
    SetDock {
        position: layout::DockPosition,
    },
    VpnImport {
        value: String,
        #[serde(default)]
        kind: vpn::ImportKind,
    },
    VpnApply {
        #[serde(default)]
        system: bool,
        enabled: bool,
        selected: usize,
    },
    GoogleAccountExternal,
    Navigate {
        value: String,
    },
    Back,
    Forward,
    Reload,
    Home,
    NewTab,
    NewWindow,
    NextWindow,
    Split {
        operation: split_view::Operation,
    },
    TabGroup {
        operation: tab_groups::Operation,
    },
    MoveTab {
        source: u64,
        target: u64,
        after: bool,
    },
    SelectTab {
        index: usize,
    },
    CloseTab {
        index: usize,
    },
    ToggleBookmark,
    ShowBookmarks,
    ShowHistory,
    ShowDownloads,
    DownloadRefresh,
    DownloadControl {
        id: u64,
        operation: storage::DownloadAction,
    },
    DownloadFolder {
        id: u64,
    },
    BookmarkUpdate {
        url: String,
        title: String,
        folder: Option<String>,
    },
    BookmarkRemove {
        url: String,
    },
    BookmarkFolder {
        previous: Option<String>,
        name: Option<String>,
    },
    ClearHistory,
    ExportBookmarks,
    ClearDownloadHistory,
    ImportBookmarks {
        html: String,
    },
    ClearSiteData,
    OpenGooglePasswords,
    OpenGoogleAccount,
    SetAppearance {
        design: u8,
        theme: u16,
    },
    AppearancePanel {
        open: bool,
        #[serde(default)]
        name: Option<String>,
    },
    DiscardBackground,
    NewPrivateWindow,
    ToggleCosmeticFilter,
    ToggleNetworkBlocking,
    ToggleSiteAdblock,
    CycleMemoryMode,
    ToggleAutoDiscard,
}

impl std::fmt::Debug for UiCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiCommand(<redacted>)")
    }
}
#[derive(Debug)]
enum AppEvent {
    InternalCommand {
        tab_id: u64,
        generation: u64,
        command: UiCommand,
    },
    Ui(UiCommand),
    VpnImported(Result<Vec<vpn::Node>, &'static str>),
    VpnProbed(Result<u128, &'static str>),
    ProfilesScanned {
        revision: u64,
        result: std::io::Result<(Catalog, ProfileDetails)>,
    },
    #[cfg(windows)]
    PdfFinished {
        path: PathBuf,
        success: bool,
    },
    NewTabAppearance {
        tab_id: u64,
        generation: u64,
        accounts: bool,
    },
    #[cfg(windows)]
    PageFocused {
        tab_id: u64,
        generation: u64,
    },
    Page {
        tab_id: u64,
        generation: u64,
        url: String,
        loading: bool,
    },
    Title {
        tab_id: u64,
        generation: u64,
        title: String,
    },
    Popup {
        tab_id: u64,
        generation: u64,
        url: String,
    },
    DownloadStarted {
        id: u64,
        url: String,
        file: String,
    },
    DownloadState {
        id: u64,
        file: PathBuf,
        status: storage::DownloadStatus,
        progress: storage::DownloadProgress,
    },
    #[cfg(windows)]
    DownloadLimit,
}

#[derive(Debug)]
enum WindowMessage {
    Browser { window: WindowId, event: AppEvent },
}

#[derive(Clone)]
struct BrowserProxy {
    window: WindowId,
    event_loop: EventLoopProxy<WindowMessage>,
}

impl BrowserProxy {
    fn send_event(&self, event: AppEvent) -> bool {
        self.event_loop
            .send_event(WindowMessage::Browser {
                window: self.window,
                event,
            })
            .is_ok()
    }
}

#[derive(Serialize)]
struct ToolbarState<'a> {
    sidebar: &'a layout::Sidebar,
    local_profiles: Catalog,
    profile_details: Option<&'a ProfileDetails>,
    profile_scan_pending: bool,
    current_profile: &'a str,
    maximized: bool,
    shortcuts: &'a literust_browser::shortcuts::Shortcuts,
    shortcut_catalog: &'a [(&'static str, String, &'static str)],
    background_style: storage::BackgroundStyle,
    account_prompt_always: bool,
    notes: &'a str,
    customization: storage::Customization,
    browser_settings: storage::BrowserSettings,
    search_engine: literust_browser::navigation::SearchEngine,
    tabs: Vec<TabSummary<'a>>,
    tab_groups: &'a [tab_groups::TabGroup],
    split: Option<SplitSummary<'a>>,
    appearance: storage::Appearance,
    dock_position: layout::DockPosition,
    vpn_enabled: bool,
    vpn_system_scope: bool,
    vpn_system_running: bool,
    vpn_running: bool,
    vpn_nodes: Vec<&'a str>,
    vpn_details: Vec<String>,
    vpn_probe: &'a str,
    vpn_probing: bool,
    vpn_selected: usize,
    vpn_importing: bool,
    active: usize,
    url: &'a str,
    loading: bool,
    bookmarked: bool,
    bookmarks: usize,
    history: usize,
    downloads: usize,
    can_back: bool,
    can_forward: bool,
    live_ids: Vec<u64>,
    private_mode: bool,
    cosmetic_filter: bool,
    network_blocking: bool,
    site_host: Option<String>,
    site_adblock_exception: bool,
    live_tab_budget: u8,
    url_warning: Option<String>,
    auto_discard: bool,
}

#[derive(Serialize)]
struct TabSummary<'a> {
    id: u64,
    group: Option<u32>,
    title: &'a str,
}

#[derive(Serialize)]
struct SplitSummary<'a> {
    #[serde(flatten)]
    layout: split_view::SplitView,
    urls: [&'a str; 2],
}

struct LivePage {
    allow_internal_navigation: std::rc::Rc<std::cell::Cell<bool>>,
    pending_html_navigation: std::rc::Rc<std::cell::Cell<bool>>,
    downloads: std::rc::Rc<std::cell::Cell<usize>>,
    #[cfg(windows)]
    printing: std::rc::Rc<std::cell::Cell<bool>>,
    view: WebView,
    last_used: u64,
    generation: u64,
    loading: bool,
    foreground: bool,
}

impl LivePage {
    fn busy(&self) -> bool {
        self.downloads.get() > 0 || self.printing()
    }

    fn printing(&self) -> bool {
        #[cfg(windows)]
        {
            self.printing.get()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

struct DrawerAnimation {
    from: f64,
    started: std::time::Instant,
}

struct Browser {
    dock_animation: Option<DrawerAnimation>,
    dock_width: std::cell::Cell<f64>,
    dock_expanded: bool,
    dock_peek: bool,
    suggestion_rows: u8,
    panel: Option<String>,
    dock_state_cache: std::cell::RefCell<String>,
    local_profiles: Catalog,
    profile_details: Option<ProfileDetails>,
    profile_revision: u64,
    profile_scan_pending: bool,
    shortcuts: std::rc::Rc<std::cell::RefCell<literust_browser::shortcuts::Runtime>>,
    closed_tabs: Vec<String>,
    system_vpn_active: bool,
    app_icon_applied: bool,
    // Drop child controllers before the context and native parent window.
    pages: HashMap<u64, LivePage>,
    toolbar: WebView,
    dock: WebView,
    #[cfg(windows)]
    page_environment:
        Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment>,
    context: Option<WebContext>,
    window: Window,
    vpn_vault: std::rc::Rc<vpn::Vault>,
    vpn_port: Option<u16>,
    vpn_running: bool,
    vpn_importing: bool,
    vpn_probe: String,
    vpn_probing: bool,
    restart_requested: bool,
    data: BrowserData,
    ui_language: storage::Language,
    shortcut_catalog: Vec<(&'static str, String, &'static str)>,
    dirty: std::cell::Cell<bool>,
    backup_requested: bool,
    shared_home_changed: bool,
    new_window_requested: bool,
    next_window_requested: bool,
    proxy: BrowserProxy,
    private_mode: bool,
    #[cfg(windows)]
    downloads: windows_platform::Downloads,
    next_tab_id: u64,
    next_generation: u64,
    usage_clock: u64,
    close_requested: bool,
    appearance_open: bool,
    account_prompt_checked: bool,
    ready: bool,
}

impl Browser {
    #[allow(clippy::too_many_arguments)]
    fn new(
        window: Window,
        toolbar: WebView,
        dock: WebView,
        context: Option<WebContext>,
        data: BrowserData,
        proxy: BrowserProxy,
        private_mode: bool,
        local_profiles: Catalog,
        #[cfg(windows)] downloads: windows_platform::Downloads,
    ) -> Self {
        let next_tab_id = data
            .tabs
            .iter()
            .map(|tab| tab.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        Self {
            dock_animation: None,
            dock_width: std::cell::Cell::new(54.0),
            dock_expanded: false,
            dock_peek: false,
            suggestion_rows: 0,
            panel: None,
            dock_state_cache: std::cell::RefCell::new(String::new()),
            local_profiles,
            profile_details: None,
            profile_revision: 0,
            profile_scan_pending: false,
            shortcuts: std::rc::Rc::new(std::cell::RefCell::new(
                literust_browser::shortcuts::Runtime {
                    bindings: data.shortcuts.clone(),
                    recording: false,
                },
            )),
            closed_tabs: Vec::new(),
            app_icon_applied: false,
            system_vpn_active: false,
            window,
            toolbar,
            dock,
            vpn_vault: Default::default(),
            vpn_port: None,
            vpn_running: false,
            vpn_importing: false,
            vpn_probe: String::new(),
            vpn_probing: false,
            restart_requested: false,
            context,
            #[cfg(windows)]
            page_environment: None,
            pages: HashMap::new(),
            ui_language: data.browser_settings.language,
            shortcut_catalog: literust_browser::shortcuts::ACTIONS
                .iter()
                .map(|&(id, label, keys)| {
                    (
                        id,
                        literust_browser::i18n::template(label, data.browser_settings.language),
                        keys,
                    )
                })
                .collect(),
            data,
            dirty: std::cell::Cell::new(false),
            backup_requested: false,
            shared_home_changed: false,
            new_window_requested: false,
            next_window_requested: false,
            proxy,
            private_mode,
            #[cfg(windows)]
            downloads,
            next_tab_id,
            next_generation: 1,
            usage_clock: 0,
            close_requested: false,
            appearance_open: false,
            account_prompt_checked: false,
            ready: false,
        }
    }

    fn run_shortcut(&mut self, name: &str) {
        if !literust_browser::shortcuts::ACTIONS
            .iter()
            .any(|(id, _, _)| *id == name)
        {
            return;
        }
        startup_log(&format!("Shortcut dispatched: {name}"));
        let command = match name {
            "new_tab" => UiCommand::NewTab,
            "new_window" => UiCommand::NewWindow,
            "close_tab" => UiCommand::CloseTab {
                index: self.data.active,
            },
            "next_tab" => UiCommand::SelectTab {
                index: (self.data.active + 1) % self.data.tabs.len(),
            },
            "previous_tab" => UiCommand::SelectTab {
                index: (self.data.active + self.data.tabs.len() - 1) % self.data.tabs.len(),
            },
            "reload" => UiCommand::Reload,
            "back" => UiCommand::Back,
            "forward" => UiCommand::Forward,
            "home" => UiCommand::Home,
            "private_window" => UiCommand::NewPrivateWindow,
            "bookmark" => UiCommand::ToggleBookmark,
            "bookmarks" => UiCommand::ShowBookmarks,
            "history" => UiCommand::ShowHistory,
            "downloads" => UiCommand::ShowDownloads,
            "print" => UiCommand::PrintPage,
            "settings" => UiCommand::ShowSettings,
            "vpn" => UiCommand::ShowVpn,
            "tools" => UiCommand::ShowTools,
            "accounts" => UiCommand::ShowAccounts,
            "duplicate" => UiCommand::DuplicateTab,
            "address" => {
                let _ = self.toolbar.focus();
                let _ = self
                    .toolbar
                    .evaluate_script("window.openPanel?.(null);window.focusAddress?.();");
                return;
            }
            "reopen_tab" => {
                if let Some(url) = self.closed_tabs.pop() {
                    let before = self.data.tabs.len();
                    self.new_tab(Some(&url));
                    if self.data.tabs.len() > before {
                        self.handle(UiCommand::SelectTab {
                            index: self.data.tabs.len() - 1,
                        });
                    }
                }
                return;
            }
            "zoom_in" | "zoom_out" | "zoom_reset" => {
                let current = self.data.browser_settings.zoom_percent;
                let value = match name {
                    "zoom_in" => current.saturating_add(10).min(200),
                    "zoom_out" => current.saturating_sub(10).max(50),
                    _ => 100,
                };
                self.data.browser_settings.zoom_percent = value;
                if let Some(page) = self.pages.get(&self.active_id()) {
                    let _ = page.view.zoom(f64::from(value) / 100.0);
                }
                self.persist();
                self.sync_toolbar();
                return;
            }
            "find" => {
                #[cfg(windows)]
                if let Some(page) = self.pages.get(&self.active_id()) {
                    if windows_platform::find(&page.view).is_err() {
                        self.notice_localized(
                            "Поиск требует обновления Microsoft WebView2 Runtime.",
                        );
                    }
                }
                return;
            }
            _ => {
                if let Some(number) = name
                    .strip_prefix("tab_")
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|n| (1..=9).contains(n))
                {
                    let index = if number == 9 {
                        self.data.tabs.len() - 1
                    } else {
                        number - 1
                    };
                    self.handle(UiCommand::SelectTab { index });
                }
                return;
            }
        };
        self.handle(command);
    }

    fn update_profiles(&mut self, result: std::io::Result<Catalog>) {
        match result {
            Ok(catalog) => {
                self.local_profiles = catalog;
                self.invalidate_profile_details();
            }
            Err(error) => self.notice_localized(&error.to_string()),
        }
    }

    fn invalidate_profile_details(&mut self) {
        self.profile_revision = self.profile_revision.wrapping_add(1);
        self.profile_details = None;
        self.scan_profiles();
        self.sync_toolbar();
    }

    fn scan_profiles(&mut self) {
        if self.private_mode || self.profile_scan_pending {
            return;
        }
        let revision = self.profile_revision;
        let proxy = self.proxy.clone();
        self.profile_scan_pending = true;
        self.sync_toolbar();
        std::thread::spawn(move || {
            let base = app_root();
            let result = Catalog::load(&base).and_then(|catalog| {
                let details = catalog.details(&base)?;
                Ok((catalog, details))
            });
            let _ = proxy.send_event(AppEvent::ProfilesScanned { revision, result });
        });
    }

    fn on_profiles_scanned(
        &mut self,
        revision: u64,
        result: std::io::Result<(Catalog, ProfileDetails)>,
    ) {
        self.profile_scan_pending = false;
        if revision != self.profile_revision {
            self.scan_profiles();
            return;
        }
        match result {
            Ok((catalog, details)) => {
                self.local_profiles = catalog;
                self.profile_details = Some(details);
            }
            Err(error) => self.notice_localized(&error.to_string()),
        }
        self.sync_toolbar();
    }

    fn write_profile_backup(&self, snapshot: &BrowserData) {
        if self.private_mode {
            return;
        }
        let path = profile_root().join("Backups");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = path.join(format!("browser-data-{timestamp}.json"));
        match fs::create_dir_all(path.parent().expect("backup directory"))
            .and_then(|()| snapshot.backup_metadata(&path))
        {
            Ok(()) => self.notice(&format!(
                "{}: {}",
                self.ui_language
                    .pick("Резервная копия сохранена", "Backup saved"),
                path.display()
            )),
            Err(error) => self.notice(&format!(
                "{}: {error}",
                self.ui_language.pick(
                    "Не удалось создать резервную копию",
                    "Unable to create backup"
                )
            )),
        }
    }

    fn system_vpn_running(&self) -> bool {
        self.system_vpn_active
    }

    fn refresh_newtabs(&mut self) {
        let active = self.active_id();
        let ids: Vec<_> = self
            .data
            .tabs
            .iter()
            .filter(|t| t.url == NEW_TAB && t.id != active)
            .map(|t| t.id)
            .collect();
        self.pages.retain(|id, _| !ids.contains(id));
        if self.current_url() == NEW_TAB {
            self.load_active();
        }
    }
    fn sync_icons(&self) {
        if let Ok(json) = serde_json::to_string(&self.data.custom_icons) {
            let script = format!("window.setCustomIcons?.({json});");
            let _ = self.toolbar.evaluate_script(&script);
            let _ = self.dock.evaluate_script(&script);
        }
    }
    fn update_app_icon(&self) {
        use base64::Engine;
        let rgba = base64::engine::general_purpose::STANDARD
            .decode(&self.data.app_icon_rgba)
            .ok();
        self.window.set_window_icon(
            rgba.and_then(|bytes| tao::window::Icon::from_rgba(bytes, 64, 64).ok())
                .or_else(|| {
                    tao::window::Icon::from_rgba(
                        include_bytes!("../packaging/default-icon.rgba").to_vec(),
                        64,
                        64,
                    )
                    .ok()
                }),
        );
        #[cfg(windows)]
        if !self.private_mode {
            let root = profile_root();
            let icon = root.join("custom.ico");
            let icon_path = if let Some(bytes) = self
                .data
                .custom_icons
                .get("app")
                .and_then(|value| literust_browser::icons::ico(value))
            {
                if fs::write(&icon, bytes).is_err() {
                    return;
                }
                icon
            } else {
                let _ = fs::remove_file(&icon);
                match std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.join("literust.ico")))
                {
                    Some(p) => p,
                    None => return,
                }
            };
            // Fixed script; user-supplied image bytes never become shell code.
            let script = r#"$s=New-Object -ComObject WScript.Shell;foreach($p in @((Join-Path ([Environment]::GetFolderPath('Desktop')) 'LiteRust.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'LiteRust/LiteRust.lnk'))){if(Test-Path $p){$l=$s.CreateShortcut($p);if($l.TargetPath -eq $env:LITERUST_EXE){$l.IconLocation=$env:LITERUST_ICON;$l.Save()}}}"#;
            use std::os::windows::process::CommandExt;
            let Ok(powershell) =
                security::windows_system_binary("WindowsPowerShell/v1.0/powershell.exe")
            else {
                return;
            };
            let _ = Command::new(powershell)
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("LITERUST_ICON", icon_path)
                .env("LITERUST_EXE", std::env::current_exe().unwrap_or_default())
                .creation_flags(0x08000000)
                .spawn();
        }
    }

    fn persist(&self) {
        if !self.private_mode {
            self.dirty.set(true);
        }
    }

    fn session(&self) -> window_session::WindowSession {
        let size = self.window.inner_size();
        let placement = self
            .window
            .outer_position()
            .ok()
            .filter(|_| !self.window.is_minimized())
            .and_then(|p| {
                let current = window_session::Placement {
                    x: p.x,
                    y: p.y,
                    width: size.width,
                    height: size.height,
                    maximized: self.window.is_maximized(),
                };
                current.valid().then_some(current)
            });
        window_session::WindowSession {
            tabs: self.data.tabs.clone(),
            tab_groups: self.data.tab_groups.clone(),
            split: self.data.split,
            active: self.data.active,
            placement: placement.or(self.data.placement),
        }
    }

    fn use_shared_profile(&mut self, profile: std::rc::Rc<storage::ProfileData>) {
        let zoom_changed =
            profile.browser_settings.zoom_percent != self.data.browser_settings.zoom_percent;
        let rules_changed = profile.network_blocking != self.data.network_blocking
            || profile.adblock_exceptions != self.data.adblock_exceptions;
        let icon_changed = profile.custom_icons != self.data.custom_icons;
        let sidebar_changed = profile.dock_position != self.data.dock_position
            || profile.sidebar.width != self.data.sidebar.width
            || profile.sidebar.mode != self.data.sidebar.mode;
        let theme_changed = profile.appearance != self.data.appearance;
        if profile.shortcuts.0 != self.data.shortcuts.0 {
            self.shortcuts.borrow_mut().bindings = profile.shortcuts.clone();
        }
        self.data.profile = profile;
        if zoom_changed {
            let zoom = f64::from(self.data.browser_settings.zoom_percent) / 100.0;
            for page in self.pages.values() {
                let _ = page.view.zoom(zoom);
            }
        }
        if theme_changed {
            let appearance = serde_json::to_string(&self.data.appearance).unwrap_or_default();
            for tab in self
                .data
                .tabs
                .iter()
                .filter(|tab| tab.url.starts_with("literust://"))
            {
                if let Some(page) = self.pages.get(&tab.id) {
                    let _ = page
                        .view
                        .evaluate_script(&format!("window.applyAppearance?.({appearance});"));
                }
            }
        }
        if rules_changed {
            self.update_filter_for_live_pages();
        }
        if icon_changed {
            self.sync_icons();
        }
        if sidebar_changed {
            self.dock_animation = None;
            self.dock_expanded = false;
            self.dock_peek = false;
            self.resize();
        }
        self.enforce_page_budget();
    }

    fn sync_home(&self, persisted: bool) {
        let Some(value) = self.data.home.as_ref() else {
            return;
        };
        let json = literust_browser::home::script_json(value);
        for tab in self.data.tabs.iter().filter(|tab| tab.url == NEW_TAB) {
            if let Some(page) = self.pages.get(&tab.id) {
                let _ = page
                    .view
                    .evaluate_script(&format!("window.homeSync?.({json},{persisted});"));
            }
        }
    }

    fn active_id(&self) -> u64 {
        self.data.tabs[self.data.active].id
    }
    fn current_url(&self) -> &str {
        &self.data.tabs[self.data.active].url
    }

    fn sync_toolbar(&self) {
        let url = self.current_url();
        let state = ToolbarState {
            sidebar: &self.data.sidebar,
            local_profiles: self.local_profiles.for_ui(if self.private_mode {
                ""
            } else {
                current_profile_id()
            }),
            profile_details: self.profile_details.as_ref(),
            profile_scan_pending: self.profile_scan_pending,
            current_profile: current_profile_id(),
            maximized: self.window.is_maximized(),
            background_style: self.data.background_style,
            account_prompt_always: self.data.account_prompt_always,
            notes: &self.data.notes,
            tabs: self
                .data
                .tabs
                .iter()
                .map(|t| TabSummary {
                    id: t.id,
                    group: t.group,
                    title: if t.url == NEW_TAB {
                        self.ui_language.pick("Новая вкладка", "New tab")
                    } else {
                        &t.title
                    },
                })
                .collect(),
            tab_groups: self.data.tab_groups.entries(),
            split: self.data.split.map(|layout| SplitSummary {
                layout,
                urls: layout.panes.map(|id| {
                    self.data
                        .tabs
                        .iter()
                        .find(|tab| tab.id == id && !tab.url.starts_with("literust://"))
                        .map_or("", |tab| tab.url.as_str())
                }),
            }),
            customization: self.data.customization,
            shortcuts: &self.data.shortcuts,
            shortcut_catalog: &self.shortcut_catalog,
            browser_settings: self.data.browser_settings.clone(),
            appearance: self.data.appearance,
            search_engine: self.data.search_engine,
            dock_position: self.data.dock_position,
            vpn_enabled: self.vpn_vault.enabled,
            vpn_system_scope: self.vpn_vault.system_scope,
            vpn_system_running: self.system_vpn_running(),
            vpn_running: self.vpn_running,
            vpn_nodes: self
                .vpn_vault
                .nodes
                .iter()
                .map(|n| n.name.as_str())
                .collect(),
            vpn_details: self.vpn_vault.nodes.iter().map(vpn::description).collect(),
            vpn_probe: &self.vpn_probe,
            vpn_probing: self.vpn_probing,
            vpn_selected: self.vpn_vault.selected,
            vpn_importing: self.vpn_importing,
            active: self.data.active,
            url: if url.starts_with("literust://") {
                ""
            } else {
                url
            },
            loading: self
                .pages
                .get(&self.active_id())
                .is_some_and(|page| page.loading),
            bookmarked: self.data.bookmarked(url),
            bookmarks: self.data.bookmarks.len(),
            history: self.data.history.len(),
            downloads: self.data.downloads.len(),
            can_back: !self.data.tabs[self.data.active].back.is_empty(),
            can_forward: !self.data.tabs[self.data.active].forward.is_empty(),
            live_ids: self.pages.keys().copied().collect(),
            private_mode: self.private_mode,
            cosmetic_filter: self.data.cosmetic_filter,
            network_blocking: self.data.network_blocking,
            site_host: storage::site_host(url),
            site_adblock_exception: self.data.adblock_exception_for(url),
            live_tab_budget: self.data.live_tab_budget,
            url_warning: security::hostname_warning(url)
                .map(|warning| literust_browser::i18n::template(warning, self.ui_language)),
            auto_discard: self.data.auto_discard,
        };
        if let Ok(payload) = serde_json::to_string(&state) {
            let dock_state = serde_json::json!({"sidebar":self.data.sidebar,"peek":self.dock_peek,"manual":self.dock_expanded,"expanded":self.dock_peek||self.dock_expanded||self.data.sidebar.mode=="pinned","appearance":self.data.appearance,"position":self.data.dock_position,"customization":self.data.customization,"animations":self.data.customization.animations,"vpn_enabled":self.vpn_vault.enabled,"vpn_running":self.vpn_running && (!self.vpn_vault.system_scope || self.system_vpn_running())});
            let dock_state = dock_state.to_string();
            let mut previous = self.dock_state_cache.borrow_mut();
            if *previous != dock_state
                && self
                    .dock
                    .evaluate_script(&format!("window.dockUpdate?.({dock_state});"))
                    .is_ok()
            {
                *previous = dock_state;
            }
            if let Err(err) = self
                .toolbar
                .evaluate_script(&format!("window.browserUpdate?.({payload});"))
            {
                eprintln!("Cannot update toolbar: {err}");
            }
        }
        self.sync_manager();
    }

    fn sync_manager(&self) {
        match self.panel.as_deref() {
            Some("bookmarks-manager") => {
                let bookmarks = serde_json::json!({
                    "items": self.data.bookmarks, "folders": self.data.bookmark_folders
                });
                let _ = self
                    .toolbar
                    .evaluate_script(&format!("window.bookmarkManagerUpdate?.({bookmarks});"));
            }
            Some("downloads-manager") => {
                let entries: Vec<_> = self.data.downloads.iter().rev().map(|entry| {
                    #[cfg(windows)]
                    let live = self.downloads.progress(entry.id);
                    #[cfg(not(windows))]
                    let live: Option<storage::DownloadProgress> = None;
                    let name = entry.file.rsplit(['\\', '/']).next().unwrap_or_default();
                    let name: String = name.chars().take(240).collect();
                    serde_json::json!({
                        "id": entry.id, "name": name, "source": entry.url, "status": entry.state(),
                        "started_at": entry.started_at, "has_folder": Path::new(&entry.file).is_absolute(),
                        "progress": live.unwrap_or(storage::DownloadProgress {
                            received: entry.received, total: entry.total, ..Default::default()
                        })
                    })
                }).collect();
                if let Ok(json) = serde_json::to_string(&entries) {
                    let _ = self
                        .toolbar
                        .evaluate_script(&format!("window.downloadManagerUpdate?.({json});"));
                }
            }
            _ => {}
        }
    }

    fn open_manager(&self, panel: &'static str) {
        debug_assert!(matches!(panel, "bookmarks-manager" | "downloads-manager"));
        let _ = self
            .toolbar
            .evaluate_script(&format!("window.openPanel?.('{panel}');"));
    }

    fn notice_localized(&self, message: &str) {
        self.notice(&literust_browser::i18n::template(message, self.ui_language));
    }

    fn notice(&self, message: &str) {
        if let Ok(text) = serde_json::to_string(message) {
            let _ = self
                .toolbar
                .evaluate_script(&format!("window.browserNotice?.({text});"));
        }
    }

    fn filter_enabled_for(&self, url: &str) -> bool {
        self.data.network_blocking && !self.data.adblock_exception_for(url)
    }

    fn update_filter_for_live_pages(&self) {
        #[cfg(windows)]
        for (tab_id, page) in &self.pages {
            if let Some(tab) = self.data.tabs.iter().find(|tab| tab.id == *tab_id) {
                if let Err(err) =
                    network_block::apply(&page.view, self.filter_enabled_for(&tab.url))
                {
                    eprintln!("Cannot update network filtering for tab {tab_id}: {err}");
                }
            }
        }
    }

    fn frame_inset(&self) -> f64 {
        if self.window.is_maximized() {
            0.0
        } else {
            6.0
        }
    }

    fn content_top(&self) -> f64 {
        TOOLBAR_HEIGHT
            + if self.data.split.is_some() {
                SPLIT_TOOLBAR_HEIGHT
            } else {
                0.0
            }
    }

    fn toolbar_height(&self) -> f64 {
        let height = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor())
            .height;
        if self.appearance_open {
            (height - 2.0 * self.frame_inset()).max(self.content_top())
        } else {
            (self.content_top()
                + f64::from(self.suggestion_rows) * 34.0
                + if self.suggestion_rows > 0 { 8.0 } else { 0.0 })
            .min((height - 2.0 * self.frame_inset()).max(self.content_top()))
        }
    }

    fn themed_html(&self, html: String) -> String {
        let settings = serde_json::to_string(&self.data.appearance).unwrap_or_default();
        let customization = serde_json::to_string(&self.data.customization).unwrap_or_default();
        html.replace(
            "</head>",
            &format!(
                "<script>{APPEARANCE_SCRIPT}\nwindow.applyAppearance({settings});window.applyCustomization({customization});</script></head>"
            ),
        )
    }

    fn layout_bounds(&self) -> (Rect, Rect) {
        let scale = self.window.scale_factor();
        let inset = self.frame_inset();
        let size = self.window.inner_size().to_logical::<f64>(scale);
        let (mut dock, page) = layout::arrange_sidebar(
            size.width - 2.0 * inset,
            size.height - 2.0 * inset,
            self.content_top(),
            self.data.dock_position,
            &self.data.sidebar,
            self.dock_expanded,
        );
        if self.dock_peek
            && !self.dock_expanded
            && self.data.dock_position == layout::DockPosition::Left
        {
            dock = layout::peek_sidebar(dock, size.width - 2.0 * inset);
        }
        (
            physical_rect(
                dock.x + inset,
                dock.y + inset,
                dock.width,
                dock.height,
                scale,
            ),
            physical_rect(
                page.x + inset,
                page.y + inset,
                page.width,
                page.height,
                scale,
            ),
        )
    }

    fn make_view(&mut self, tab_id: u64) -> wry::Result<LivePage> {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        let load_proxy = self.proxy.clone();
        let title_proxy = self.proxy.clone();
        let popup_proxy = self.proxy.clone();
        let internal_proxy = self.proxy.clone();
        let bounds = self.page_bounds(self.layout_bounds().1, tab_id);
        let builder = if self.private_mode {
            // WebContext must not be supplied to an incognito view.
            WebViewBuilder::new().with_incognito(true)
        } else {
            WebViewBuilder::new_with_web_context(self.context.as_mut().expect("normal WebContext"))
        };
        let builder =
            routed_builder(builder, self.vpn_port).with_background_color((16, 20, 33, 255));
        #[cfg(windows)]
        let builder = if let Some(environment) = &self.page_environment {
            builder.with_environment(environment.clone())
        } else {
            builder
        };
        let allow_internal_navigation = std::rc::Rc::new(std::cell::Cell::new(true));
        let navigation_mode = allow_internal_navigation.clone();
        let pending_html_navigation = std::rc::Rc::new(std::cell::Cell::new(false));
        let pending_html = pending_html_navigation.clone();
        let mut builder = builder
            .with_visible(false)
            .with_bounds(bounds)
            .with_navigation_handler(move |url| {
                // NavigateToString uses a runtime-specific navigation URI. Only the
                // next application-requested HTML load receives this allowance.
                if pending_html.replace(false)
                    && (url.is_empty() || url == "about:blank" || url.starts_with("data:text/html"))
                {
                    return true;
                }
                if navigation_mode.get()
                    && url.starts_with("literust://home/?")
                    && url.len() <= 100_000
                {
                    if let Ok(parsed) = url::Url::parse(&url) {
                        if let Some((_, payload)) =
                            parsed.query_pairs().find(|(key, _)| key == "data")
                        {
                            if let Ok(command) = serde_json::from_str::<UiCommand>(&payload) {
                                if matches!(
                                    command,
                                    UiCommand::SaveHome { .. } | UiCommand::Navigate { .. }
                                ) {
                                    let _ = internal_proxy.send_event(AppEvent::InternalCommand {
                                        tab_id,
                                        generation,
                                        command,
                                    });
                                }
                            }
                        }
                    }
                    return false;
                }
                if navigation_mode.get()
                    && matches!(
                        url.as_str(),
                        "literust://appearance" | "literust://accounts"
                    )
                {
                    let _ = internal_proxy.send_event(AppEvent::NewTabAppearance {
                        tab_id,
                        generation,
                        accounts: url == "literust://accounts",
                    });
                    return false;
                }
                let allowed = permitted_page_navigation(&url)
                    && (url != "about:blank" || navigation_mode.get());
                if allowed && valid_web_url(&url) {
                    navigation_mode.set(false);
                }
                allowed
            })
            .with_permission_handler(site_permission_policy)
            .with_new_window_req_handler(move |url, _| {
                if valid_web_url(&url) {
                    let _ = popup_proxy.send_event(AppEvent::Popup {
                        tab_id,
                        generation,
                        url,
                    });
                }
                NewWindowResponse::Deny
            })
            .with_on_page_load_handler(move |event, url| {
                let _ = load_proxy.send_event(AppEvent::Page {
                    tab_id,
                    generation,
                    url,
                    loading: matches!(event, PageLoadEvent::Started),
                });
            })
            .with_document_title_changed_handler(move |title| {
                if matches!(title.as_str(), "Новая вкладка" | "New tab") {
                    startup_log("New tab document ready");
                }
                let _ = title_proxy.send_event(AppEvent::Title {
                    tab_id,
                    generation,
                    title,
                });
            });
        if self.data.cosmetic_filter {
            builder = builder.with_initialization_script(COSMETIC_FILTER);
        }
        let view = builder.build_as_child(&self.window)?;
        #[cfg(windows)]
        if self.private_mode {
            windows_platform::verify_private(&view)
                .map_err(|error| wry::Error::Io(std::io::Error::other(error)))?;
        }
        #[cfg(windows)]
        if self.page_environment.is_none() {
            let _ = windows_platform::color_scheme(&view, self.dark_theme());
            self.page_environment = Some(view.environment());
        }
        view.zoom(f64::from(self.data.browser_settings.zoom_percent) / 100.0)?;
        #[cfg(windows)]
        if let Some(tab) = self.data.tabs.iter().find(|tab| tab.id == tab_id) {
            if self.filter_enabled_for(&tab.url) {
                if let Err(err) = network_block::apply(&view, true) {
                    eprintln!("Network block rules failed to install: {err}");
                }
            }
        }
        #[cfg(windows)]
        let downloads = windows_platform::configure(
            &view,
            self.proxy.clone(),
            self.shortcuts.clone(),
            &self.downloads,
        )
        .map_err(|error| wry::Error::Io(std::io::Error::other(error.to_string())))?;
        #[cfg(not(windows))]
        let downloads = std::rc::Rc::new(std::cell::Cell::new(0));
        #[cfg(windows)]
        windows_platform::page_focus(&view, self.proxy.clone(), tab_id, generation)
            .map_err(|error| wry::Error::Io(std::io::Error::other(error.to_string())))?;
        Ok(LivePage {
            allow_internal_navigation,
            pending_html_navigation,
            downloads,
            #[cfg(windows)]
            printing: Default::default(),
            view,
            last_used: self.usage_clock,
            generation,
            loading: false,
            foreground: false,
        })
    }

    fn page_bounds(&self, base: Rect, id: u64) -> Rect {
        let Some(split) = self.data.split else {
            return base;
        };
        let Some(pane) = split.pane(id) else {
            return base;
        };
        let scale = self.window.scale_factor();
        let position = base.position.to_logical::<f64>(scale);
        let size = base.size.to_logical::<f64>(scale);
        let pane = split.bounds(
            layout::Bounds {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            },
            pane,
        );
        physical_rect(pane.x, pane.y, pane.width, pane.height, scale)
    }

    fn is_visible_tab(&self, id: u64) -> bool {
        self.data
            .split
            .map_or_else(|| id == self.active_id(), |split| split.pane(id).is_some())
    }

    fn window_page_budget(&self) -> usize {
        usize::from(self.data.live_tab_budget).max(if self.data.split.is_some() { 2 } else { 1 })
    }

    fn discard_one_background_page(&mut self) -> bool {
        let victim = lru_victim(
            self.pages
                .iter()
                .filter(|(id, page)| !self.is_visible_tab(**id) && !page.busy() && !page.loading)
                .map(|(id, page)| (*id, page.last_used)),
            self.active_id(),
        );
        victim.and_then(|id| self.pages.remove(&id)).is_some()
    }

    fn enforce_page_budget(&mut self) -> bool {
        if !self.data.auto_discard {
            return false;
        }
        let before = self.pages.len();
        while self.pages.len() > self.window_page_budget() && self.discard_one_background_page() {}
        self.pages.len() != before
    }

    fn ensure_page(&mut self, id: u64) -> bool {
        if self.pages.contains_key(&id) {
            return true;
        }
        while self.data.auto_discard
            && self.pages.len() >= self.window_page_budget()
            && self.discard_one_background_page()
        {}
        match self.make_view(id) {
            Ok(page) => {
                self.pages.insert(id, page);
                self.load_page(id);
                true
            }
            Err(err) => {
                self.notice(&format!("Не удалось создать WebView: {err}"));
                false
            }
        }
    }

    // Both panes stay active. Cached background tabs keep DOM and scroll
    // position until evicted and reloaded through the same creation path.
    fn show_active(&mut self) {
        let mut active = self.active_id();
        if let Some(split) = &mut self.data.split {
            split.select(active);
        }
        self.usage_clock = self.usage_clock.saturating_add(1);
        let current_ready = self.ensure_page(active);
        if let Some(split) = self.data.split {
            let peer = split.tab(split.focus.other());
            let peer_ready = self.ensure_page(peer);
            if !current_ready || !peer_ready {
                self.data.split = None;
                if !current_ready && peer_ready {
                    self.data.active = self
                        .data
                        .tabs
                        .iter()
                        .position(|tab| tab.id == peer)
                        .unwrap_or(self.data.active);
                    active = peer;
                }
                self.persist();
            }
        }
        let split = self.data.split;
        for (id, page) in &mut self.pages {
            let shown = *id == active || split.is_some_and(|split| split.pane(*id).is_some());
            if shown {
                page.last_used = self.usage_clock;
            }
            if shown != page.foreground {
                page.foreground = shown;
                let _ = page.view.set_visible(shown && !self.appearance_open);
                #[cfg(windows)]
                let _ = page.view.set_memory_usage_level(if shown {
                    MemoryUsageLevel::Normal
                } else {
                    MemoryUsageLevel::Low
                });
            }
        }
        self.resize();
        if self.ready && !self.appearance_open && self.window.is_focused() {
            if let Some(page) = self.pages.get(&active) {
                let _ = page.view.focus();
            }
        }
        self.sync_toolbar();
    }

    fn load_active(&mut self) {
        self.load_page(self.active_id());
        self.sync_toolbar();
    }

    fn load_page(&mut self, id: u64) {
        let Some(url) = self
            .data
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.url.clone())
        else {
            return;
        };
        let html = match url.as_str() {
            NEW_TAB => Some(literust_browser::newtab::render_with_profile_localized(
                &self.data,
                self.private_mode,
                self.local_profiles
                    .profiles
                    .iter()
                    .find(|p| p.id == current_profile_id()),
                self.ui_language,
            )),
            BOOKMARKS | HISTORY | DOWNLOADS => {
                Some(render_library(&url, &self.data, self.ui_language))
            }
            _ => None,
        };
        let html = html.map(|html| self.themed_html(html));
        let block_for_url = self.filter_enabled_for(&url);
        if let Some(page) = self.pages.get_mut(&id) {
            // Adjust the CDP rule list before starting a toolbar-initiated navigation.
            // CDP completion is async: this is best-effort and not a security boundary.
            #[cfg(windows)]
            if let Err(err) = network_block::apply(&page.view, block_for_url) {
                eprintln!("Cannot apply per-site network rules: {err}");
            }
            #[cfg(not(windows))]
            let _ = block_for_url;
            if html.is_some() {
                let _ = page.view.set_background_color((16, 20, 33, 255));
            }
            page.loading = valid_web_url(&url);
            page.allow_internal_navigation.set(html.is_some());
            page.pending_html_navigation.set(html.is_some());
            let result = match html {
                Some(html) => page.view.load_html(&html),
                None => page.view.load_url(&url),
            };
            if let Err(err) = result {
                page.loading = false;
                eprintln!("Navigation failed: {err}");
            }
        }
    }

    fn dark_theme(&self) -> bool {
        !matches!(self.data.appearance.theme % 5, 2 | 3)
    }

    fn print_page(&self) {
        if let Some(page) = self
            .pages
            .get(&self.active_id())
            .filter(|page| !page.printing())
        {
            #[cfg(windows)]
            if windows_platform::print_preview(&page.view).is_ok() {
                return;
            }
            let _ = page.view.print();
        }
    }

    #[cfg(windows)]
    fn print_pdf(&self, header: &str, footer: &str, captions: bool, backgrounds: bool) {
        let Some(page) = self.pages.get(&self.active_id()) else {
            return;
        };
        if page.loading || page.printing.get() {
            self.notice_localized("Дождитесь завершения текущей операции.");
            return;
        }
        if header.contains('\0')
            || footer.contains('\0')
            || header.len() > 1024
            || footer.len() > 1024
        {
            self.notice_localized("Сократите текст колонтитулов.");
            return;
        }
        let result = windows_platform::choose_pdf(&page.view).and_then(|path| {
            if let Some(path) = path {
                windows_platform::print_pdf(
                    &page.view,
                    path,
                    windows_platform::PdfOptions {
                        header,
                        footer,
                        captions,
                        backgrounds,
                    },
                    page.printing.clone(),
                    self.proxy.clone(),
                )?;
            }
            Ok(())
        });
        if let Err(err) = result {
            self.notice(&format!(
                "{}: {err}",
                self.ui_language
                    .pick("Не удалось сохранить PDF", "Unable to save PDF")
            ));
        }
    }

    #[cfg(not(windows))]
    fn print_pdf(&self, _header: &str, _footer: &str, _captions: bool, _backgrounds: bool) {
        self.print_page();
    }

    fn navigate(&mut self, input: &str) {
        if security::reject_address_input(input) {
            self.notice_localized(
                "Адрес отклонён: опасная схема, встроенные учётные данные или превышение длины.",
            );
            return;
        }
        let url = literust_browser::navigation::destination_with_theme(
            input,
            self.data.search_engine,
            self.dark_theme(),
        );
        self.navigate_internal(&url);
    }

    fn navigate_internal(&mut self, url: &str) {
        if !storage::valid_tab_url(url) {
            return;
        }
        if valid_web_url(url) && !security::safe_top_level_navigation(url) {
            self.notice_localized(
                "Опасный адрес: логин или пароль встроен прямо в URL. Переход отменён.",
            );
            return;
        }
        if let Some(warning) = security::hostname_warning(url) {
            self.notice_localized(warning);
        }
        self.data.tabs[self.data.active].navigate(url);
        self.load_active();
        self.persist();
    }

    fn run_split(&mut self, operation: split_view::Operation) {
        use split_view::{Operation, SplitView};
        match operation {
            Operation::Open { tab } => {
                let active = self.active_id();
                if tab == active || !self.data.tabs.iter().any(|item| item.id == tab) {
                    return;
                }
                if let Some(split) = &mut self.data.split {
                    if split.pane(tab).is_some() {
                        return;
                    }
                    split.panes[split.focus.other().index()] = tab;
                } else {
                    self.data.split = Some(SplitView::new(active, tab));
                }
                self.show_active();
                self.persist();
            }
            Operation::Focus { pane } => self.focus_split_pane(pane),
            Operation::Choose { pane, tab } => {
                let Some(index) = self.data.tabs.iter().position(|item| item.id == tab) else {
                    return;
                };
                let Some(split) = &mut self.data.split else {
                    return;
                };
                if split.tab(pane.other()) == tab {
                    return;
                }
                split.focus = pane;
                split.select(tab);
                self.data.active = index;
                self.show_active();
                self.persist();
            }
            Operation::Navigate { pane, value } => {
                if self.data.split.is_some() {
                    self.focus_split_pane(pane);
                    self.navigate(&value);
                    if let Some(page) = self.pages.get(&self.active_id()) {
                        let _ = page.view.focus();
                    }
                }
            }
            Operation::Axis { axis } => {
                if let Some(split) = &mut self.data.split {
                    split.axis = axis;
                    self.resize();
                    self.sync_toolbar();
                    self.persist();
                }
            }
            Operation::Resize { percent } => {
                if let Some(split) = &mut self.data.split {
                    split.resize(percent);
                    self.resize();
                    self.sync_toolbar();
                    self.persist();
                }
            }
            Operation::Swap => {
                if let Some(split) = &mut self.data.split {
                    split.swap();
                    self.resize();
                    self.sync_toolbar();
                    self.persist();
                }
            }
            Operation::Close { pane } => {
                let Some(split) = self.data.split else { return };
                if self.pages.get(&split.tab(pane)).is_some_and(LivePage::busy) {
                    self.notice_localized("Завершите печать или загрузку перед закрытием области.");
                    return;
                }
                self.focus_split_pane(pane.other());
                self.data.split = None;
                self.pages.remove(&split.tab(pane));
                self.show_active();
                self.persist();
            }
            Operation::Exit => {
                self.data.split = None;
                self.show_active();
                if self.enforce_page_budget() {
                    self.sync_toolbar();
                }
                self.persist();
            }
        }
    }

    fn focus_split_pane(&mut self, pane: split_view::Pane) {
        let Some(split) = &mut self.data.split else {
            return;
        };
        if split.focus == pane {
            return;
        }
        if let Some(index) = self
            .data
            .tabs
            .iter()
            .position(|tab| tab.id == split.tab(pane))
        {
            split.focus = pane;
            self.data.active = index;
            self.persist();
            self.sync_toolbar();
        }
    }

    fn open_library(&mut self, url: &str) {
        if let Some(index) = self.data.tabs.iter().position(|tab| tab.url == url) {
            self.data.active = index;
            self.show_active();
            self.load_active(); // Internal library pages are snapshots.
        } else if self.data.tabs.len() < MAX_TABS {
            self.new_tab(Some(url));
        } else {
            self.navigate_internal(url);
        }
        self.persist();
    }

    fn new_tab(&mut self, url: Option<&str>) {
        if self.data.tabs.len() >= MAX_TABS {
            self.notice_localized("Достигнут лимит 24 вкладок.");
            return;
        }
        if let Some(url) = url {
            if valid_web_url(url) && !security::safe_top_level_navigation(url) {
                self.notice_localized(
                    "Новая вкладка заблокирована: подозрительные данные пользователя в URL.",
                );
                return;
            }
        }
        let background =
            url.is_some_and(valid_web_url) && self.data.browser_settings.background_links;
        let mut tab = match url {
            Some(url) if valid_web_url(url) => Tab::web(url),
            Some(url) if storage::valid_tab_url(url) => Tab::internal(url),
            _ => Tab::new(),
        };
        tab.id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.data.tabs.push(tab);
        if !background {
            self.data.active = self.data.tabs.len() - 1;
        }
        if background {
            self.sync_toolbar();
        } else {
            self.show_active();
        }
        self.persist();
    }

    fn handle(&mut self, command: UiCommand) {
        if self.appearance_open
            && self.panel.as_deref() != Some("settings")
            && matches!(
                &command,
                UiCommand::NewTab
                    | UiCommand::DuplicateTab
                    | UiCommand::SelectTab { .. }
                    | UiCommand::Home
                    | UiCommand::ShowBookmarks
                    | UiCommand::ShowHistory
                    | UiCommand::ShowDownloads
            )
        {
            self.appearance_open = false;
            self.panel = None;
            self.resize();
            let _ = self.toolbar.evaluate_script("window.openPanel?.(null);");
        }
        match command {
            UiCommand::ShowSecurity => {
                self.handle(UiCommand::ShowSettings);
                let _ = self
                    .toolbar
                    .evaluate_script("window.selectSettingsCategory?.('privacy');");
            }
            UiCommand::SaveHome { value } => {
                if !value.valid() {
                    self.notice_localized(
                        "Некорректная компоновка или ссылка. Изменения не сохранены.",
                    );
                    return;
                }
                self.data.home = Some(value);
                self.shared_home_changed = true;
                self.persist();
            }
            UiCommand::SetSidebar { value } => {
                self.dock_animation = None;
                self.data.sidebar = value.validated();
                self.dock_expanded = false;
                self.dock_peek = false;
                self.persist();
                self.resize();
                self.sync_toolbar();
            }
            UiCommand::DockExpanded {
                open,
                animate,
                peek,
            } => {
                if peek {
                    if self.data.dock_position != layout::DockPosition::Left
                        || self.data.sidebar.mode != "click"
                        || self.dock_expanded
                    {
                        return;
                    }
                    self.dock_peek = open;
                } else {
                    self.dock_peek = false;
                    self.dock_expanded = open;
                }
                let (bounds, _) = self.layout_bounds();
                let target = bounds
                    .size
                    .to_logical::<f64>(self.window.scale_factor())
                    .width;
                if animate
                    && self.data.customization.animations
                    && matches!(
                        self.data.dock_position,
                        layout::DockPosition::Left | layout::DockPosition::Right
                    )
                {
                    self.dock_animation = Some(DrawerAnimation {
                        from: self.dock_width.get(),
                        started: std::time::Instant::now(),
                    });
                } else {
                    self.dock_animation = None;
                    self.dock_width.set(target);
                    let _ = self.dock.set_bounds(bounds);
                }
                #[cfg(windows)]
                {
                    let _ = windows_platform::raise(if self.appearance_open {
                        &self.toolbar
                    } else {
                        &self.dock
                    });
                }
                self.sync_toolbar();
            }
            UiCommand::ProfileCreate { name } => {
                if !self.private_mode {
                    self.update_profiles(Catalog::create_profile(&app_root(), &name));
                }
            }
            UiCommand::ProfileUpdate { id, name, avatar } => {
                if !self.private_mode {
                    self.update_profiles(Catalog::edit(&app_root(), |catalog| {
                        catalog.update(&id, &name, &avatar)
                    }));
                    self.refresh_newtabs();
                }
            }
            UiCommand::ProfileDelete { id } => {
                if !self.private_mode {
                    self.update_profiles(Catalog::delete(&app_root(), &id, current_profile_id()));
                }
            }
            UiCommand::ProfileRestore { id } => {
                if !self.private_mode {
                    self.update_profiles(Catalog::restore(&app_root(), &id));
                }
            }
            UiCommand::ProfilePurge { id } => {
                if !self.private_mode {
                    match Catalog::purge(&app_root(), &id) {
                        Ok(()) => self.invalidate_profile_details(),
                        Err(error) => {
                            self.invalidate_profile_details();
                            self.notice_localized(&error.to_string());
                        }
                    }
                }
            }
            UiCommand::ProfileDetails => {
                self.scan_profiles();
            }
            UiCommand::ProfileBackup => {
                self.backup_requested = !self.private_mode;
            }
            UiCommand::ProfileSwitch { id } => {
                if self.private_mode || id == current_profile_id() {
                    return;
                }
                match Catalog::load(&app_root()) {
                    Ok(catalog) if catalog.profiles.iter().any(|p| p.id == id) => {
                        #[cfg(windows)]
                        if local_profiles::root(&app_root(), &id)
                            .is_ok_and(|root| windows_platform::activate_profile_window(&root))
                        {
                            return;
                        }
                        self.persist();
                        if let Err(error) = std::env::current_exe()
                            .and_then(|exe| Command::new(exe).arg("--profile").arg(&id).spawn())
                        {
                            self.notice(&format!(
                                "{}: {error}",
                                self.ui_language
                                    .pick("Не удалось открыть профиль", "Unable to open profile")
                            ));
                        }
                    }
                    _ => self.notice_localized("Профиль не найден"),
                }
            }
            UiCommand::Shortcut { name } => self.run_shortcut(&name),
            UiCommand::ShortcutRecording { active } => {
                self.shortcuts.borrow_mut().recording = active;
            }
            UiCommand::SetShortcuts { value } => {
                if !value.validate() {
                    self.notice_localized(
                        "Сочетание занято или недопустимо. Изменения не сохранены.",
                    );
                    return;
                }
                self.shortcuts.borrow_mut().bindings = value.clone();
                self.data.shortcuts = value;
                self.persist();
                self.sync_toolbar();
            }
            UiCommand::ShowTools
            | UiCommand::ShowAccounts
            | UiCommand::ShowSettings
            | UiCommand::ShowAppearance
            | UiCommand::ShowVpn => {
                let panel = match command {
                    UiCommand::ShowTools => "tools",
                    UiCommand::ShowAccounts => "accounts",
                    UiCommand::ShowVpn => "vpn",
                    UiCommand::ShowAppearance => "appearance",
                    _ => "settings",
                };
                let _ = self
                    .toolbar
                    .evaluate_script(&format!("window.openPanel?.('{panel}');"));
            }
            UiCommand::SetCustomIcon {
                target,
                value,
                rgba,
            } => {
                if self.private_mode {
                    self.notice_localized("Меняйте значки в обычном окне.");
                    return;
                }
                if !literust_browser::icons::valid_target(&target)
                    || (!value.is_empty() && literust_browser::icons::png_bytes(&value).is_none())
                {
                    self.notice_localized("Некорректный значок");
                    return;
                }
                if !value.is_empty()
                    && !self.data.custom_icons.contains_key(&target)
                    && self.data.custom_icons.len() >= 32
                {
                    self.notice_localized("Можно сохранить до 32 своих значков.");
                    return;
                }
                if target == "app" {
                    use base64::Engine;
                    if !value.is_empty()
                        && !base64::engine::general_purpose::STANDARD
                            .decode(&rgba)
                            .is_ok_and(|v| v.len() == 64 * 64 * 4)
                    {
                        self.notice_localized("Некорректный значок приложения");
                        return;
                    }
                    self.data.app_icon_rgba = rgba;
                }
                if value.is_empty() {
                    self.data.custom_icons.remove(&target);
                } else {
                    self.data.custom_icons.insert(target.clone(), value);
                }
                self.persist();
                self.sync_icons();
                if target == "app" {
                    self.update_app_icon();
                }
                self.refresh_newtabs();
                self.notice_localized("Значок сохранён");
            }
            UiCommand::SetBrowserSettings { mut value } => {
                value.home_url = value.home_url.trim().to_owned();
                if !value.valid_home() {
                    self.notice_localized(
                        "Для домашней страницы нужен HTTP(S)-адрес без логина и пароля.",
                    );
                    return;
                }
                let language_changed = self.ui_language != value.language;
                self.data.browser_settings = value.validated();
                for page in self.pages.values() {
                    let _ = page
                        .view
                        .zoom(f64::from(self.data.browser_settings.zoom_percent) / 100.0);
                }
                self.persist();
                self.sync_toolbar();
                self.notice(if language_changed {
                    "Язык интерфейса изменится после перезапуска LiteRust. / Restart LiteRust to change the interface language."
                } else {
                    self.ui_language.pick("Настройки браузера сохранены. Режим запуска применяется при следующем обычном запуске.", "Browser settings saved. Startup settings apply the next time LiteRust opens.")
                });
            }
            UiCommand::SetCustomization { value } => {
                self.data.customization = value.validated();
                self.persist();
                self.refresh_newtabs();
                self.sync_toolbar();
            }
            UiCommand::SetBackground { value } => {
                if !literust_browser::newtab::valid_background(&value) {
                    self.notice_localized("Выберите JPEG или HTTPS-ссылку на изображение.");
                    return;
                }
                self.data.background = value;
                self.persist();
                self.refresh_newtabs();
                self.notice_localized("Фон сохранён. Он появится на новой вкладке.");
            }
            UiCommand::SetSearchEngine { engine } => {
                self.data.search_engine = engine;
                self.persist();
                self.refresh_newtabs();
                self.sync_toolbar();
            }
            UiCommand::SetDock { position } => {
                self.data.dock_position = position;
                self.dock_peek = false;
                self.dock_animation = None;
                self.resize();
                self.persist();
                self.sync_toolbar();
            }
            UiCommand::VpnProbe => {
                if !self.vpn_running || self.vpn_probing {
                    return;
                }
                if let Some(port) = self.vpn_port {
                    self.vpn_probing = true;
                    self.vpn_probe = "Проверяем HTTPS через VPN…".into();
                    self.sync_toolbar();
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let result = vpn::probe(port);
                        let _ = proxy.send_event(AppEvent::VpnProbed(result));
                    });
                }
            }
            UiCommand::VpnScope { system } => {
                if self.private_mode || self.vpn_vault.enabled {
                    return;
                }
                std::rc::Rc::make_mut(&mut self.vpn_vault).system_scope = system;
                #[cfg(windows)]
                if literust_browser::vpn_platform::save(&profile_root(), &self.vpn_vault).is_err() {
                    self.notice_localized("Не удалось сохранить область VPN");
                }
                self.sync_toolbar();
            }
            UiCommand::VpnImport { value, kind } => {
                if self.private_mode || self.vpn_importing || self.vpn_vault.enabled {
                    self.notice_localized("Импорт доступен в обычном окне при выключенном VPN.");
                    return;
                }
                if value.len() > vpn::MAX_IMPORT {
                    self.notice_localized("Размер ключей превышает 1 MiB");
                    return;
                }
                self.vpn_importing = true;
                self.sync_toolbar();
                let proxy = self.proxy.clone();
                let port = self.vpn_port;
                std::thread::spawn(move || {
                    let result = vpn::import_with_kind(&value, port, kind);
                    let _ = proxy.send_event(AppEvent::VpnImported(result));
                });
            }
            UiCommand::VpnApply {
                enabled,
                selected,
                system,
            } => {
                if self.private_mode || self.vpn_importing {
                    self.notice_localized(
                        "Изменение VPN доступно в обычном окне после завершения импорта",
                    );
                    return;
                }
                if enabled && selected >= self.vpn_vault.nodes.len() {
                    self.notice_localized("Сначала импортируйте и выберите сервер");
                    return;
                }
                if self.pages.values().any(LivePage::busy) {
                    self.notice_localized("Дождитесь завершения скачиваний перед изменением VPN.");
                    return;
                }
                let previous = self.vpn_vault.clone();
                let vault = std::rc::Rc::make_mut(&mut self.vpn_vault);
                vault.system_scope = system;
                vault.enabled = enabled;
                vault.selected = selected;
                #[cfg(windows)]
                if literust_browser::vpn_platform::save(&profile_root(), &self.vpn_vault).is_err() {
                    self.vpn_vault = previous;
                    self.notice_localized("Не удалось сохранить защищённые настройки VPN");
                    return;
                }
                #[cfg(not(windows))]
                {
                    let _ = previous;
                    self.notice_localized("VPN-хаб поддерживается в Windows-сборке");
                    return;
                }
                #[cfg(windows)]
                {
                    self.restart_requested = true;
                }
            }
            UiCommand::GoogleAccountExternal => {
                if !self.private_mode && google_passwords::open_official_account().is_err() {
                    self.notice_localized("Не удалось открыть системный браузер");
                }
            }
            UiCommand::WindowMinimize => self.window.set_minimized(true),
            UiCommand::WindowMaximize => {
                self.window.set_maximized(!self.window.is_maximized());
                self.sync_toolbar();
            }
            UiCommand::WindowClose => {
                self.persist();
                self.close_requested = true;
            }
            UiCommand::WindowDrag => {
                let _ = self.window.drag_window();
            }
            UiCommand::DockReady => {
                self.dock_state_cache.get_mut().clear();
                self.sync_toolbar();
            }
            UiCommand::DuplicateTab => {
                let url = self.data.tabs[self.data.active].url.clone();
                let before = self.data.tabs.len();
                self.new_tab(Some(&url));
                if self.data.tabs.len() > before {
                    self.data.active = self.data.tabs.len() - 1;
                    self.show_active();
                    self.persist();
                }
            }
            UiCommand::SetAccountPrompt { always } => {
                self.data.account_prompt_seen = true;
                self.data.account_prompt_always = always;
                self.persist();
                self.sync_toolbar();
            }
            UiCommand::OpenAccount { provider } => {
                if let Some(url) = literust_browser::accounts::url(&provider) {
                    self.navigate_internal(url);
                    self.persist();
                }
            }
            UiCommand::SetBackgroundStyle { value } => {
                self.data.background_style = value.validated();
                self.persist();
                self.refresh_newtabs();
                self.sync_toolbar();
            }
            UiCommand::SetNotes { value } => {
                self.data.notes = value.chars().take(20_000).collect();
                self.persist();
            }
            UiCommand::PrintPage => {
                let _ = self.toolbar.evaluate_script("window.openPanel?.('print');");
            }
            UiCommand::PrintNative => self.print_page(),
            UiCommand::OpenCertificates =>
            {
                #[cfg(windows)]
                if let Err(err) = windows_platform::open_certificates() {
                    self.notice(&format!(
                        "{}: {err}",
                        self.ui_language.pick(
                            "Не удалось открыть сертификаты Windows",
                            "Unable to open Windows certificates"
                        )
                    ));
                }
            }
            UiCommand::PrintPdf {
                header,
                footer,
                captions,
                backgrounds,
            } => {
                self.print_pdf(&header, &footer, captions, backgrounds);
            }
            UiCommand::Suggest { query } => {
                let matches = if self.private_mode {
                    Vec::new()
                } else {
                    literust_browser::navigation::suggestions(&self.data, &query)
                };
                let query = literust_browser::home::script_json(&query);
                let matches = literust_browser::home::script_json(&matches);
                let _ = self
                    .toolbar
                    .evaluate_script(&format!("window.showSuggestions?.({query},{matches});"));
            }
            UiCommand::SuggestionPanel { rows } => {
                let rows = if self.appearance_open { 0 } else { rows.min(6) };
                if self.suggestion_rows != rows {
                    self.suggestion_rows = rows;
                    self.resize();
                }
            }
            UiCommand::Ready => {
                self.sync_icons();
                if !self.app_icon_applied && self.data.custom_icons.contains_key("app") {
                    self.app_icon_applied = true;
                    self.update_app_icon();
                }
                if self.vpn_vault.enabled
                    && self.vpn_vault.system_scope
                    && !self.system_vpn_running()
                    && !self.private_mode
                {
                    self.notice_localized("VPN всего компьютера не запущен. Защищены только вкладки браузера. Откройте VPN-хаб и подтвердите запрос Windows.");
                }
                if !self.account_prompt_checked {
                    self.account_prompt_checked = true;
                    if !self.private_mode
                        && !std::env::args().any(|a| a == "--resume-after-restart")
                        && (!self.data.account_prompt_seen || self.data.account_prompt_always)
                    {
                        let _ = self
                            .toolbar
                            .evaluate_script("window.openPanel?.('accounts');");
                    }
                }
                self.ready = true;
                if self.data.split.is_some() && self.window.is_focused() && !self.appearance_open {
                    if let Some(page) = self.pages.get(&self.active_id()) {
                        let _ = page.view.focus();
                    }
                }
                startup_log("Toolbar JavaScript ready");
                self.window.set_visible(true);
                let _ = self.toolbar.set_visible(true);
                let _ = self.dock.set_visible(true);
                self.resize();
                if self.appearance_open {
                    let _ = self.toolbar.focus();
                } else if let Some(page) = self.pages.get(&self.active_id()) {
                    let _ = page.view.focus();
                }
                self.window.request_redraw();
                self.sync_toolbar();
            }
            UiCommand::Navigate { value } => self.navigate(&value),
            UiCommand::Back => {
                if self.data.tabs[self.data.active].go_back() {
                    self.load_active();
                    self.persist();
                }
            }
            UiCommand::Forward => {
                if self.data.tabs[self.data.active].go_forward() {
                    self.load_active();
                    self.persist();
                }
            }
            UiCommand::Reload => {
                if !valid_web_url(self.current_url()) {
                    self.load_active();
                } else if let Some(page) = self.pages.get(&self.active_id()) {
                    if let Err(err) = page.view.reload() {
                        eprintln!("Reload: {err}");
                    }
                }
            }
            UiCommand::Home => {
                let home = self.data.browser_settings.home().to_owned();
                self.navigate_internal(&home);
            }
            UiCommand::NewTab => self.new_tab(Some(NEW_TAB)),
            UiCommand::NewWindow => {
                if self.private_mode {
                    if let Err(error) = std::env::current_exe().and_then(|executable| {
                        Command::new(executable)
                            .arg("--profile")
                            .arg(current_profile_id())
                            .arg("--new-window")
                            .spawn()
                    }) {
                        self.notice(&error.to_string());
                    }
                } else {
                    self.new_window_requested = true;
                }
            }
            UiCommand::NextWindow => self.next_window_requested = true,
            UiCommand::Split { operation } => self.run_split(operation),
            UiCommand::TabGroup { operation } => {
                if self
                    .data
                    .tab_groups
                    .apply(&mut self.data.tabs, &mut self.data.active, operation)
                {
                    self.persist();
                    self.sync_toolbar();
                } else {
                    self.notice_localized("Проверьте имя группы и выбранные вкладки.");
                }
            }
            UiCommand::MoveTab {
                source,
                target,
                after,
            } => {
                if self.data.tab_groups.move_tab(
                    &mut self.data.tabs,
                    &mut self.data.active,
                    source,
                    target,
                    after,
                ) {
                    self.persist();
                    self.sync_toolbar();
                }
            }
            UiCommand::SelectTab { index } => {
                if index < self.data.tabs.len() && index != self.data.active {
                    self.data.active = index;
                    self.show_active();
                    self.persist();
                }
            }
            UiCommand::CloseTab { index } => {
                if index >= self.data.tabs.len() {
                    return;
                }
                let closed_id = self.data.tabs[index].id;
                if self.pages.get(&closed_id).is_some_and(LivePage::busy) {
                    self.notice_localized("Завершите печать или загрузку перед закрытием вкладки.");
                    return;
                }
                let split_peer = self
                    .data
                    .split
                    .and_then(|split| split.pane(closed_id).map(|pane| split.tab(pane.other())));
                if split_peer.is_some() {
                    self.data.split = None;
                }
                let closed_active = index == self.data.active;
                self.closed_tabs.push(self.data.tabs[index].url.clone());
                if self.closed_tabs.len() > 25 {
                    self.closed_tabs.remove(0);
                }
                self.pages.remove(&closed_id);
                self.data.tabs.remove(index);
                if self.data.tabs.is_empty() {
                    let mut tab = Tab::new();
                    tab.id = self.next_tab_id;
                    self.next_tab_id = self.next_tab_id.saturating_add(1);
                    self.data.tabs.push(tab);
                    self.data.active = 0;
                    self.data.tab_groups = Default::default();
                    self.close_requested = true;
                    self.persist();
                    return;
                } else if closed_active {
                    self.data.active = index.min(self.data.tabs.len() - 1);
                } else if index < self.data.active {
                    self.data.active -= 1;
                }
                if let Some(peer) = split_peer {
                    if let Some(active) = self.data.tabs.iter().position(|tab| tab.id == peer) {
                        self.data.active = active;
                    }
                }
                self.data
                    .tab_groups
                    .organize(&mut self.data.tabs, &mut self.data.active);
                self.show_active();
                self.persist();
            }
            UiCommand::ToggleBookmark => {
                if self.private_mode {
                    self.notice_localized("В приватном окне закладки не записываются на диск.");
                    return;
                }
                let tab = &self.data.tabs[self.data.active];
                let (url, title) = (tab.url.clone(), tab.title.clone());
                if !self.data.bookmarked(&url)
                    && self.data.bookmarks.len() >= storage::MAX_BOOKMARKS
                {
                    self.notice_localized(
                        "Достигнут лимит закладок. Удалите ненужную закладку перед добавлением.",
                    );
                    return;
                }
                self.data.toggle_bookmark(&url, &title);
                self.persist();
                if self.current_url() == BOOKMARKS {
                    self.load_active();
                } else {
                    self.sync_toolbar();
                }
            }
            UiCommand::ShowBookmarks => self.open_manager("bookmarks-manager"),
            UiCommand::ShowHistory => self.open_library(HISTORY),
            UiCommand::ShowDownloads => self.open_manager("downloads-manager"),
            UiCommand::DownloadRefresh => {
                if self.panel.as_deref() == Some("downloads-manager") {
                    self.sync_manager();
                }
            }
            UiCommand::DownloadControl { id, operation } => {
                #[cfg(windows)]
                if !self.downloads.control(id, operation) {
                    self.notice_localized("Действие для этой загрузки недоступно.");
                    self.sync_manager();
                }
                #[cfg(not(windows))]
                let _ = (id, operation);
            }
            UiCommand::DownloadFolder { id } => {
                #[cfg(windows)]
                {
                    use tao::platform::windows::WindowExtWindows;
                    let opened = self
                        .data
                        .downloads
                        .iter()
                        .find(|entry| entry.id == id)
                        .is_some_and(|entry| {
                            windows_platform::open_download_folder(
                                windows::Win32::Foundation::HWND(self.window.hwnd() as _),
                                Path::new(&entry.file),
                            )
                        });
                    if !opened {
                        self.notice_localized("Папка загрузки недоступна.");
                    }
                }
                #[cfg(not(windows))]
                let _ = id;
            }
            UiCommand::BookmarkUpdate { url, title, folder } => {
                if !self.private_mode {
                    if !self.data.update_bookmark(&url, &title, folder) {
                        self.notice_localized("Закладка или папка недоступна.");
                    }
                    self.persist();
                    self.sync_toolbar();
                }
            }
            UiCommand::BookmarkRemove { url } => {
                if !self.private_mode {
                    self.data.remove_bookmark(&url);
                    self.persist();
                    self.sync_toolbar();
                }
            }
            UiCommand::BookmarkFolder { previous, name } => {
                if !self.private_mode {
                    if !self
                        .data
                        .update_bookmark_folder(previous.as_deref(), name.as_deref())
                    {
                        self.notice_localized(
                            "Проверьте имя папки. Имена папок должны быть уникальными.",
                        );
                    }
                    self.persist();
                    self.sync_toolbar();
                }
            }
            UiCommand::ExportBookmarks => {
                if self.private_mode {
                    self.notice_localized("Экспорт закладок недоступен в приватном окне.");
                } else if self.data.bookmarks.is_empty() {
                    self.notice_localized("Нет закладок для экспорта.");
                } else {
                    match export_bookmarks(&self.data) {
                        Ok(path) => self.notice(&format!(
                            "Экспортировано {} закладок: {}",
                            self.data.bookmarks.len(),
                            path.display()
                        )),
                        Err(err) => {
                            self.notice(&format!("Не удалось экспортировать закладки: {err}"))
                        }
                    }
                }
            }
            UiCommand::ClearDownloadHistory => {
                #[cfg(windows)]
                self.data
                    .downloads
                    .retain(|entry| self.downloads.contains(entry.id));
                #[cfg(not(windows))]
                self.data.downloads.clear();
                self.persist();
                if self.current_url() == DOWNLOADS {
                    self.load_active();
                } else {
                    self.sync_toolbar();
                }
                self.notice_localized("История завершённых загрузок очищена.");
            }
            UiCommand::ClearHistory => {
                self.data.history.clear();
                for tab in &mut self.data.tabs {
                    tab.back.clear();
                    tab.forward.clear();
                }
                self.persist();
                if self.current_url() == HISTORY {
                    self.load_active();
                } else {
                    self.sync_toolbar();
                }
                self.notice_localized("История LiteRust удалена. Cookies и кэш не затронуты.");
            }
            UiCommand::ImportBookmarks { html } => {
                if self.private_mode {
                    self.notice_localized("Импорт закладок недоступен в приватном окне.");
                    return;
                }
                match import::import_bookmarks(&mut self.data, &html) {
                    Ok(stats) => {
                        self.persist();
                        if self.current_url() == BOOKMARKS {
                            self.load_active();
                        } else {
                            self.sync_toolbar();
                        }
                        self.notice(&format!(
                            "Импорт завершён: +{} закладок, {} повторов, {} неподдерживаемых, {} сверх лимита.",
                            stats.added, stats.duplicates, stats.rejected, stats.skipped_capacity,
                        ));
                    }
                    Err(err) => self.notice_localized(err),
                }
            }
            UiCommand::AppearancePanel { open, name } => {
                self.appearance_open = open;
                self.panel = if open { name } else { None };
                self.suggestion_rows = 0;
                if !open {
                    self.shortcuts.borrow_mut().recording = false;
                }
                if open {
                    let _ = self.toolbar.focus();
                }
                for (id, page) in &self.pages {
                    let _ = page.view.set_visible(!open && self.is_visible_tab(*id));
                }
                self.resize();
                self.sync_manager();
            }
            UiCommand::SetAppearance { design, theme } => {
                if design >= 30 || theme >= 330 {
                    return;
                }
                self.data.appearance = storage::Appearance { design, theme };
                self.persist();
                self.sync_toolbar();
                #[cfg(windows)]
                if let Some(page) = self.pages.get(&self.active_id()) {
                    let _ = windows_platform::color_scheme(&page.view, self.dark_theme());
                }
                let settings = serde_json::to_string(&self.data.appearance).unwrap_or_default();
                for (id, page) in &self.pages {
                    if self
                        .data
                        .tabs
                        .iter()
                        .any(|t| t.id == *id && t.url.starts_with("literust://"))
                    {
                        let _ = page
                            .view
                            .evaluate_script(&format!("window.applyAppearance?.({settings});"));
                    }
                }
            }
            UiCommand::DiscardBackground => {
                let active = self.active_id();
                let split = self.data.split;
                let before = self.pages.len();
                self.pages.retain(|id, page| {
                    *id == active
                        || split.is_some_and(|split| split.pane(*id).is_some())
                        || page.loading
                        || page.busy()
                });
                self.sync_toolbar();
                self.notice(&format!("Выгружено вкладок: {}. Активная вкладка, загружающиеся страницы и скачивания сохранены.", before - self.pages.len()));
            }
            UiCommand::OpenGoogleAccount => {
                if self.private_mode {
                    self.notice_localized(
                        "Для постоянного входа откройте Google в обычном окне LiteRust.",
                    );
                    return;
                }
                if let Some(index) = self.data.tabs.iter().position(|t| {
                    url::Url::parse(&t.url).ok().is_some_and(|u| {
                        matches!(
                            u.host_str(),
                            Some("myaccount.google.com" | "accounts.google.com")
                        )
                    })
                }) {
                    self.data.active = index;
                    self.show_active();
                } else if self.data.tabs.len() < MAX_TABS {
                    self.new_tab(Some(google_passwords::OFFICIAL_ACCOUNT_URL));
                } else {
                    self.navigate_internal(google_passwords::OFFICIAL_ACCOUNT_URL);
                }
                self.persist();
            }
            UiCommand::OpenGooglePasswords => {
                if self.private_mode {
                    self.notice_localized("Google Password Manager открывается во внешнем обычном браузере. В приватном окне LiteRust действие отключено.");
                    return;
                }
                match google_passwords::open_official_manager() {
                    Ok(()) => self.notice_localized("Открываем Google Password Manager во внешнем браузере. Автозаполнение Google внутри LiteRust пока недоступно."),
                    Err(err) => self.notice(&format!("Не удалось открыть Google Password Manager: {err}")),
                }
            }
            UiCommand::ClearSiteData => {
                if let Some(page) = self.pages.get(&self.active_id()) {
                    match page.view.clear_all_browsing_data() {
                        Ok(()) => self.notice_localized(
                            "Запрошена очистка всех данных сайтов WebView2, включая cookies и кэш.",
                        ),
                        Err(err) => {
                            self.notice(&format!("Не удалось очистить данные сайтов: {err}"))
                        }
                    }
                }
            }
            UiCommand::NewPrivateWindow => {
                match std::env::current_exe().and_then(|exe| Command::new(exe).arg("--profile").arg(current_profile_id()).arg("--private").arg(self.ui_language.pick("--ui-ru", "--ui-en")).spawn()) {
                    Ok(_) => self.notice_localized("Открыто отдельное приватное окно. Файлы загрузок всё равно сохраняются на диск."),
                    Err(err) => self.notice(&format!("Не удалось открыть приватное окно: {err}")),
                }
            }
            UiCommand::ToggleNetworkBlocking => {
                self.data.network_blocking = !self.data.network_blocking;
                self.update_filter_for_live_pages();
                self.persist();
                self.sync_toolbar();
                if valid_web_url(self.current_url()) {
                    if let Some(page) = self.pages.get(&self.active_id()) {
                        if let Err(err) = page.view.reload() {
                            eprintln!("Filter reload: {err}");
                        }
                    }
                }
                self.notice(if self.data.network_blocking {
                    "Экспериментальный сетевой фильтр включён. Блокирует ограниченный список рекламных хостов, некоторые сайты могут работать неправильно."
                } else {
                    "Сетевой фильтр выключен. Правила отменены, активная страница обновлена."
                });
            }
            UiCommand::ToggleAutoDiscard => {
                self.data.auto_discard = !self.data.auto_discard;
                self.enforce_page_budget();
                self.persist();
                self.sync_toolbar();
            }
            UiCommand::CycleMemoryMode => {
                let budget = self.data.cycle_live_tab_budget();
                self.enforce_page_budget();
                self.persist();
                self.sync_toolbar();
                self.notice(&format!("Бюджет: {budget}. Он применяется только при включённой автовыгрузке; формы и медиа при выгрузке теряются."));
            }
            UiCommand::ToggleSiteAdblock => {
                let url = self.current_url().to_owned();
                if !self.data.network_blocking {
                    self.notice_localized(
                        "Включите сетевой AdBlock, чтобы управлять исключениями для сайтов.",
                    );
                    return;
                }
                let Some(host) = storage::site_host(&url) else {
                    self.notice_localized(
                        "Исключение доступно только для открытого HTTP(S)-сайта.",
                    );
                    return;
                };
                match self.data.toggle_adblock_exception(&url) {
                    Some(is_excepted) => {
                        self.update_filter_for_live_pages();
                        self.persist();
                        self.sync_toolbar();
                        if let Some(page) = self.pages.get(&self.active_id()) {
                            if let Err(err) = page.view.reload() {
                                eprintln!("Filter reload: {err}");
                            }
                        }
                        self.notice(&format!(
                            "Сетевой AdBlock для {host}: {}. Активная страница обновлена.",
                            if is_excepted {
                                "выключен"
                            } else {
                                "включён"
                            },
                        ));
                    }
                    None => self.notice_localized(
                        "Не удалось изменить исключение (возможно, достигнут лимит 256 сайтов).",
                    ),
                }
            }
            UiCommand::ToggleCosmeticFilter => {
                if self.pages.values().any(LivePage::busy) {
                    self.notice_localized(
                        "Дождитесь завершения загрузок перед пересозданием вкладок.",
                    );
                    return;
                }
                self.data.cosmetic_filter = !self.data.cosmetic_filter;
                // Initialization scripts are fixed at WebView construction time.
                // Rebuild all live WebViews for reliable toggle semantics.
                self.pages.clear();
                self.show_active();
                self.persist();
                self.notice_localized("Косметический фильтр переключён. Вкладки перезагрузятся. Он скрывает часть баннеров, но НЕ блокирует рекламные запросы.");
            }
        }
    }

    fn matching_view(&self, tab_id: u64, generation: u64) -> bool {
        self.pages
            .get(&tab_id)
            .is_some_and(|p| p.generation == generation)
    }

    fn on_page_event(&mut self, tab_id: u64, generation: u64, url: String, loading: bool) {
        if !self.matching_view(tab_id, generation) {
            return;
        }
        // Initial NavigateToString callbacks can arrive after load_url. They
        // must not mark the later network navigation as finished.
        if url == "about:blank"
            && self
                .pages
                .get(&tab_id)
                .is_some_and(|p| !p.allow_internal_navigation.get())
        {
            return;
        }
        let enabled_for_url = self.filter_enabled_for(&url);
        if let Some(page) = self.pages.get_mut(&tab_id) {
            page.loading = loading;
            if !loading && valid_web_url(&url) {
                // Preserve readable default HTML on sites that do not specify a background.
                let _ = page.view.set_background_color((255, 255, 255, 255));
            }
            // Handles in-page/cross-site navigation and redirects, possibly after
            // the first request. Do not present this as comprehensive blocking.
            #[cfg(windows)]
            if loading {
                if let Err(err) = network_block::apply(&page.view, enabled_for_url) {
                    eprintln!("Cannot adjust filter on page load: {err}");
                }
            }
            #[cfg(not(windows))]
            let _ = enabled_for_url;
        }
        if valid_web_url(&url) {
            let title = self
                .data
                .tabs
                .iter_mut()
                .find(|tab| tab.id == tab_id)
                .map(|tab| {
                    if tab.url != url {
                        tab.navigate(&url);
                    }
                    tab.title.clone()
                });
            if !loading && !self.private_mode {
                if let Some(title) = title {
                    self.data.record_visit(&url, &title);
                }
            }
        }
        let mut discarded = false;
        if !loading {
            discarded = self.enforce_page_budget();
            self.persist();
        }
        if self.is_visible_tab(tab_id) || discarded {
            self.sync_toolbar();
        }
    }

    fn on_title(&mut self, tab_id: u64, generation: u64, title: String) {
        if !self.matching_view(tab_id, generation) {
            return;
        }
        let title = storage::compact_title(title.trim());
        if title.is_empty() {
            return;
        }
        if let Some(tab) = self
            .data
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id && valid_web_url(&tab.url))
        {
            tab.title = title.clone();
            let url = tab.url.clone();
            self.data.update_history_title(&url, &title);
            self.persist();
            if self.active_id() == tab_id {
                self.sync_toolbar();
            }
        }
    }

    fn on_download_start(&mut self, id: u64, url: String, file: String) {
        let source = download_source(&url);
        self.data.record_download_start(id, source, file);
        if self.data.downloads.len() > storage::MAX_DOWNLOADS {
            #[cfg(windows)]
            let evict = self
                .data
                .downloads
                .iter()
                .position(|entry| !self.downloads.contains(entry.id));
            #[cfg(not(windows))]
            let evict = Some(0);
            if let Some(index) = evict {
                self.data.downloads.remove(index);
            }
        }
        self.persist();
        self.sync_toolbar();
    }
    fn on_download_state(
        &mut self,
        id: u64,
        file: &Path,
        status: storage::DownloadStatus,
        progress: storage::DownloadProgress,
    ) {
        let terminal = !progress.can_cancel;
        self.data.record_download_state(id, file, status, progress);
        if terminal {
            self.enforce_page_budget();
        }
        self.persist();
        if self.current_url() == DOWNLOADS {
            self.load_active();
        } else {
            self.sync_toolbar();
        }
    }

    fn animate_drawer(&mut self) {
        let Some(animation) = self.dock_animation.as_ref() else {
            return;
        };
        let progress = animation.started.elapsed().as_secs_f64() / 0.2;
        let (bounds, _) = self.layout_bounds();
        let scale = self.window.scale_factor();
        let size = bounds.size.to_logical::<f64>(scale);
        // Snap to the same bounds as the page layout, including after resize or DPI changes.
        if progress >= 1.0 {
            let _ = self.dock.set_bounds(bounds);
            self.dock_width.set(size.width);
            self.dock_animation = None;
            return;
        }
        let width =
            animation.from + (size.width - animation.from) * (1.0 - (1.0 - progress).powi(3));
        let mut position = bounds.position.to_logical::<f64>(scale);
        if matches!(self.data.dock_position, layout::DockPosition::Right) {
            position.x += size.width - width;
        }
        if (width - self.dock_width.get()).abs() >= 0.5 {
            let _ = self.dock.set_bounds(physical_rect(
                position.x,
                position.y,
                width,
                size.height,
                scale,
            ));
            self.dock_width.set(width);
        }
    }

    fn resize(&self) {
        let _ = self.toolbar.evaluate_script(&format!(
            "window.updateWindowState?.({});",
            self.window.is_maximized()
        ));
        let scale = self.window.scale_factor();
        let size = self.window.inner_size().to_logical::<f64>(scale);
        let _ = self.toolbar.set_bounds(physical_rect(
            self.frame_inset(),
            self.frame_inset(),
            size.width - 2.0 * self.frame_inset(),
            self.toolbar_height(),
            scale,
        ));
        let (dock_bounds, bounds) = self.layout_bounds();
        if self.dock_animation.is_none() {
            self.dock_width.set(
                dock_bounds
                    .size
                    .to_logical::<f64>(self.window.scale_factor())
                    .width,
            );
            let _ = self.dock.set_bounds(dock_bounds);
        }
        for (id, page) in &self.pages {
            let _ = page.view.set_bounds(self.page_bounds(bounds, *id));
        }
        #[cfg(windows)]
        {
            let _ = windows_platform::raise(if self.appearance_open || self.suggestion_rows > 0 {
                &self.toolbar
            } else {
                &self.dock
            });
        }
    }
}

/// Do not grant top-level access to file:// or executable URL schemes.
fn download_source(input: &str) -> String {
    if !valid_web_url(input) || input.len() > 4096 {
        return String::new();
    }
    let Ok(mut url) = url::Url::parse(input) else {
        return String::new();
    };
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

fn permitted_page_navigation(url: &str) -> bool {
    security::safe_top_level_navigation(url) && (valid_web_url(url) || url == "about:blank")
}

/// Explicitly ask on sensitive permissions, deny silent high-risk API access.
/// The native WebView2 permission prompt is used; no fake permission UI.
fn site_permission_policy(kind: PermissionKind) -> PermissionResponse {
    match kind {
        PermissionKind::Camera
        | PermissionKind::Microphone
        | PermissionKind::Geolocation
        | PermissionKind::DisplayCapture => PermissionResponse::Default,
        PermissionKind::Notifications
        | PermissionKind::ClipboardRead
        | PermissionKind::Midi
        | PermissionKind::Sensors
        | PermissionKind::AutomaticDownloads
        | PermissionKind::FileSystemAccess => PermissionResponse::Deny,
        PermissionKind::Autoplay => PermissionResponse::Default,
        _ => PermissionResponse::Deny,
    }
}

fn library_row(href: &str, title: &str, subtitle: &str, badge: &str) -> String {
    let label = if title.is_empty() { href } else { title };
    format!(
        "<a class=\"entry\" href=\"{}\" title=\"{}\"><span class=\"entry-text\"><strong>{}</strong><small>{}</small></span><span class=\"tag\">{}</span></a>",
        escape_html(href), escape_html(href), escape_html(label),
        escape_html(subtitle), escape_html(badge)
    )
}

fn render_library(kind: &str, data: &BrowserData, language: storage::Language) -> String {
    let (title, lead, entries): (&str, &str, Vec<String>) = match kind {
        BOOKMARKS => ("Закладки", "Избранные сайты. Звёздочка в адресной строке добавляет или удаляет закладку.",
            data.bookmarks.iter().rev().map(|b| library_row(&b.url, &b.title, &b.url, "★")).collect()),
        HISTORY => ("История", "Сохранённые посещения (до 1500). Историю можно очистить через меню ⋯ вверху.",
            data.history.iter().rev().map(|h| library_row(&h.url, &h.title, &h.url, "↗")).collect()),
        DOWNLOADS => ("Загрузки", "Список загрузок. Файлы сохраняет движок WebView2; здесь отображается их статус.",
            data.downloads.iter().rev().take(150).map(|d| {
                // This page must not expose local file:// links or paths as hrefs.
                format!("<div class=\"entry\"><span class=\"entry-text\"><strong>{}</strong><small>{}</small></span><span class=\"tag\">{}</span></div>",
                    escape_html(d.file.rsplit(['\\', '/']).next().unwrap_or(&d.file)),
                    escape_html(&d.url), escape_html(&literust_browser::i18n::template(&d.status, language)))
            }).collect()),
        _ => ("LiteRust", "", vec![]),
    };
    let content = if entries.is_empty() {
        literust_browser::i18n::template(
            "<div class=\"empty\">Здесь пока ничего нет</div>",
            language,
        )
    } else {
        entries.join("\n")
    };
    literust_browser::i18n::template(LIBRARY, language)
        .replace(
            "__TITLE__",
            &literust_browser::i18n::template(title, language),
        )
        .replace(
            "__LEAD__",
            &literust_browser::i18n::template(lead, language),
        )
        .replace("<!-- ENTRIES -->", &content)
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: WebLogicalSize::new(width.max(1.0), height.max(1.0)).into(),
    }
}

fn physical_rect(x: f64, y: f64, width: f64, height: f64, scale: f64) -> Rect {
    let (x, width) = layout::physical_axis(x, width, scale);
    let (y, height) = layout::physical_axis(y, height, scale);
    Rect {
        position: wry::dpi::PhysicalPosition::new(x, y).into(),
        size: wry::dpi::PhysicalSize::new(width, height).into(),
    }
}

/// Export a Chrome-compatible Netscape bookmarks HTML file. Only browser
/// metadata is exported; passwords and cookies are never touched.
fn export_bookmarks(data: &BrowserData) -> std::io::Result<PathBuf> {
    let downloads = std::env::var_os("USERPROFILE")
        .map(|home| PathBuf::from(home).join("Downloads"))
        .filter(|path| path.is_dir());
    let dir = downloads.unwrap_or_else(|| profile_root().join("Exports"));
    fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let content = data.bookmarks_as_html();
    // create_new is intentional: exports never silently replace an older file.
    for suffix in 0..100u32 {
        let name = if suffix == 0 {
            format!("literust-bookmarks-{stamp}.html")
        } else {
            format!("literust-bookmarks-{stamp}-{suffix}.html")
        };
        let path = dir.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(err) = file
                    .write_all(content.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(err);
                }
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "слишком много файлов экспорта за секунду",
    ))
}

fn app_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|home| PathBuf::from(home).join("AppData").join("Local"))
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("LiteRustBrowser")
}

static CURRENT_PROFILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
fn current_profile_id() -> &'static str {
    CURRENT_PROFILE.get().map(String::as_str).unwrap_or("main")
}
fn profile_root() -> PathBuf {
    literust_browser::local_profiles::root(&app_root(), current_profile_id())
        .expect("validated local profile id")
}

static STARTUP_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
fn startup_log(message: &str) {
    let Some(file) = STARTUP_LOG.get() else {
        return;
    };
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(file, "{message}");
    }
}

fn main() {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(pair) = args.windows(2).find(|a| a[0] == "--session-self-test") {
            let result = match pair[1].as_str() {
                "downloads" => session_self_test::downloads(),
                "split" => session_self_test::split(false),
                "split-private" => session_self_test::split(true),
                phase => session_self_test::run(phase),
            };
            if let Err(error) = result {
                eprintln!("Native self-test failed: {error}");
                std::process::exit(1);
            }
            return;
        }
    }

    // Do this before spawning threads or creating WebView2 environments.
    for key in [
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
        "WEBVIEW2_USER_DATA_FOLDER",
        "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    ] {
        std::env::remove_var(key);
    }
    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--tun-self-test") {
        let result = literust_browser::system_vpn::self_test();
        let log = std::env::current_exe()
            .unwrap()
            .with_file_name("tun-self-test.log");
        let _ = fs::write(
            log,
            match &result {
                Ok(_) => "PASS: Windows TUN to SOCKS fixture and owned cleanup".to_string(),
                Err(e) => format!("FAIL: {e}"),
            },
        );
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--check-runtime") {
        std::process::exit(
            if wry::webview_version().is_ok_and(|version| !version.trim().is_empty()) {
                0
            } else {
                1
            },
        );
    }

    if let Err(error) = run() {
        startup_log(&format!("Startup failed: {error}"));
        #[cfg(windows)]
        unsafe {
            use windows::{
                core::HSTRING,
                Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
            };
            let message = HSTRING::from(format!("LiteRust не удалось запустить: {error}"));
            MessageBoxW(
                None,
                &message,
                &HSTRING::from("LiteRust — ошибка запуска"),
                MB_OK | MB_ICONERROR,
            );
        }
        #[cfg(not(windows))]
        eprintln!("LiteRust startup failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let base = app_root();
    let known_profiles = Catalog::load(&base)?;
    let args: Vec<String> = std::env::args().collect();
    let private_mode = args.iter().any(|arg| arg == "--private");
    let restarting = args.iter().any(|arg| arg == "--resume-after-restart");
    let new_window = args.iter().any(|arg| arg == "--new-window");
    let selected = args
        .windows(2)
        .find(|pair| pair[0] == "--profile")
        .map(|pair| pair[1].clone())
        .unwrap_or_else(|| known_profiles.selected.clone());
    if !known_profiles
        .profiles
        .iter()
        .any(|profile| profile.id == selected)
    {
        return Err(std::io::Error::other("Локальный профиль не найден").into());
    }
    let _ = CURRENT_PROFILE.set(selected);
    let (catalog, _profile_lock) = if private_mode {
        (known_profiles, None)
    } else {
        let mut attempts = 0;
        loop {
            match Catalog::activate(&base, current_profile_id()) {
                Ok((catalog, lock)) => break (catalog, Some(lock)),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    #[cfg(windows)]
                    if !restarting {
                        let root = profile_root();
                        let accepted = if new_window {
                            windows_platform::request_profile_window(&root)
                        } else {
                            windows_platform::activate_profile_window(&root)
                        };
                        if accepted {
                            return Ok(());
                        }
                    }
                    // The owner may still be creating its window, or closing for a restart.
                    if attempts >= if restarting { 40 } else { 8 } {
                        return Err(error.into());
                    }
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(error) => return Err(error.into()),
            }
        }
    };

    let diagnostics = if private_mode {
        base.join("diagnostics")
    } else {
        profile_root().join("diagnostics")
    };
    if fs::create_dir_all(&diagnostics).is_ok() {
        let file = diagnostics.join(if private_mode {
            "private-startup.log"
        } else {
            "startup.log"
        });
        let _ = fs::write(
            &file,
            concat!("LiteRust ", env!("CARGO_PKG_VERSION"), " startup\n"),
        );
        let _ = STARTUP_LOG.set(file);
    }
    // Fail before creating any native window: an absent runtime must not leave
    // a blank window while WebView construction is attempted.
    #[cfg(windows)]
    {
        startup_log("Checking WebView2 Runtime");
        let version = wry::webview_version().map_err(|error| {
            std::io::Error::other(format!(
                "Microsoft Edge WebView2 Runtime не найден или недоступен.\n\nУстановите WebView2 Runtime с официального сайта Microsoft:\nhttps://go.microsoft.com/fwlink/p/?LinkId=2124703\n\nПосле установки снова запустите LiteRust. Сам браузер Edge устанавливать не обязательно.\n\nДиагностика: {error}"
            ))
        })?;
        if version.trim().is_empty() {
            return Err(std::io::Error::other("WebView2 вернул пустую версию. Установите Microsoft WebView2 Runtime: https://go.microsoft.com/fwlink/p/?LinkId=2124703").into());
        }
        startup_log(&format!("WebView2 Runtime available: {version}"));
    }
    startup_log("Creating event loop and window");
    let event_loop = EventLoopBuilder::<WindowMessage>::with_user_event().build();
    let (mut data, data_file) = if private_mode {
        // No normal profile opened and no browser-data.json read/written.
        let mut data = BrowserData::default().clean();
        if args.iter().any(|arg| arg == "--ui-en") {
            data.browser_settings.language = storage::Language::En;
        }
        (data, None)
    } else {
        let root = profile_root();
        fs::create_dir_all(&root)?;
        let file = root.join("browser-data.json");
        let mut data = BrowserData::recover(&file)?;
        data.apply_startup(restarting);
        data.save(&file)?; // migrate plaintext metadata before opening site views
        (data, Some(file))
    };
    #[cfg(windows)]
    let mut vpn_vault = literust_browser::vpn_platform::load(&profile_root())?;
    #[cfg(windows)]
    if private_mode {
        vpn_vault.system_scope = false;
    }
    #[cfg(not(windows))]
    let vpn_vault = vpn::Vault::default();
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--vpn-off") {
        vpn_vault.enabled = false;
        if !private_mode {
            literust_browser::vpn_platform::save(&profile_root(), &vpn_vault)?;
        }
    }
    #[cfg(windows)]
    let mut vpn_tunnel = if vpn_vault.enabled {
        Some(literust_browser::vpn_platform::Tunnel::start(&vpn_vault).map_err(|_|std::io::Error::other("VPN не запустился. Прямое соединение не включено. Запустите LiteRust с --vpn-off, чтобы изменить ключ."))?)
    } else {
        None
    };
    #[cfg(windows)]
    let vpn_port = vpn_tunnel.as_ref().map(|t| t.port);
    #[cfg(windows)]
    let mut system_vpn = if vpn_vault.enabled && vpn_vault.system_scope && !private_mode {
        let result = literust_browser::system_vpn::SystemTunnel::start(
            vpn_tunnel.as_ref().unwrap().socks_port,
        );
        if result.is_err() {
            startup_log("System VPN unavailable; browser remains proxied");
        }
        result.ok()
    } else {
        None
    };
    #[cfg(not(windows))]
    let vpn_port: Option<u16> = None;
    #[cfg(windows)]
    let system_vpn_active = system_vpn.is_some();
    #[cfg(not(windows))]
    let system_vpn_active = false;
    #[cfg(windows)]
    windows_platform::Downloads::reserve_ids(data.downloads.len());
    let options = WindowOptions {
        private_mode,
        #[cfg(windows)]
        downloads: windows_platform::Downloads::default(),
        local_profiles: catalog,
        vpn_vault: std::rc::Rc::new(vpn_vault),
        vpn_port,
        vpn_running: vpn_port.is_some(),
        system_vpn_active,
    };
    let mut pending_sessions =
        std::collections::VecDeque::from(std::mem::take(&mut data.additional_windows));
    let mut restore_failed = false;
    let proxy = event_loop.create_proxy();
    let mut windows = vec![create_browser(
        &event_loop,
        &proxy,
        data,
        options.clone(),
        None,
    )?];
    if new_window && !private_mode {
        windows[0].new_window_requested = true;
    }
    let mut focused = windows[0].window.id();
    let mut placement_save_at = None;
    startup_log("Entering event loop");

    event_loop.run(move |event, target, control_flow| {
        let now = std::time::Instant::now();
        let mut handled = None;
        let mut refresh = false;
        #[cfg(windows)]
        {
            if windows[0].vpn_running && vpn_tunnel.as_mut().is_some_and(|tunnel| !tunnel.alive()) {
                for browser in &mut windows {
                    browser.vpn_running = false;
                    browser.notice_localized("VPN остановился. Прямое соединение не включается. Откройте VPN-хаб.");
                }
                refresh = true;
            }
            if system_vpn.as_ref().is_some_and(|tunnel| !tunnel.alive()) {
                system_vpn.take();
                for browser in &mut windows {
                    browser.system_vpn_active = false;
                    browser.notice_localized("VPN всего ПК остановлен. Windows может использовать обычную сеть. Браузер остаётся на своём прокси.");
                }
                refresh = true;
            }
        }
        match event {
            Event::UserEvent(WindowMessage::Browser { window, event }) => {
                if let Some(index) = windows.iter().position(|browser| browser.window.id() == window) {
                    dispatch_browser_event(&mut windows[index], event);
                    handled = Some(index);
                }
            }
            Event::WindowEvent { window_id, event, .. } => {
                if let Some(browser) = windows.iter_mut().find(|browser| browser.window.id() == window_id) {
                    match event {
                        WindowEvent::Focused(true) => {
                            focused = window_id;
                            if browser.ready && browser.window.is_focused() {
                                if browser.appearance_open {
                                    let _ = browser.toolbar.focus();
                                } else if let Some(page) = browser.pages.get(&browser.active_id()) {
                                    let _ = page.view.focus();
                                }
                            }
                            placement_save_at = Some(now + std::time::Duration::from_millis(250));
                        }
                        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                            browser.resize();
                            placement_save_at = Some(now + std::time::Duration::from_millis(250));
                        }
                        WindowEvent::Moved(_) => placement_save_at = Some(now + std::time::Duration::from_millis(250)),
                        WindowEvent::CloseRequested => browser.close_requested = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        let mut home_changed = false;
        if let Some(index) = handled {
            let profile = windows[index].data.profile.clone();
            let vpn_vault = windows[index].vpn_vault.clone();
            let vpn_importing = windows[index].vpn_importing;
            let vpn_probing = windows[index].vpn_probing;
            let vpn_probe = windows[index].vpn_probe.clone();
            home_changed = std::mem::take(&mut windows[index].shared_home_changed);
            for (peer, browser) in windows.iter_mut().enumerate() {
                if peer != index && !std::rc::Rc::ptr_eq(&browser.data.profile, &profile) {
                    browser.use_shared_profile(profile.clone());
                    browser.sync_toolbar();
                }
                if !std::rc::Rc::ptr_eq(&browser.vpn_vault, &vpn_vault)
                    || (browser.vpn_importing, browser.vpn_probing, &browser.vpn_probe) != (vpn_importing, vpn_probing, &vpn_probe)
                {
                    browser.vpn_vault = vpn_vault.clone();
                    browser.vpn_importing = vpn_importing;
                    browser.vpn_probing = vpn_probing;
                    browser.vpn_probe.clone_from(&vpn_probe);
                    browser.sync_toolbar();
                }
            }
        }
        if refresh {
            windows.iter().for_each(Browser::sync_toolbar);
        }

        for browser in windows.iter_mut().filter(|browser| browser.close_requested) {
            if browser.pages.values().any(LivePage::busy) {
                browser.close_requested = false;
                browser.notice_localized("Завершите печать или загрузку перед закрытием окна.");
                if browser.pages.values().any(|page| page.downloads.get() > 0) {
                    browser.open_manager("downloads-manager");
                }
            }
        }
        let mut changed = false;
        if let Some(index) = windows.iter().position(|browser| browser.close_requested) {
            changed = true;
            if windows.len() > 1 {
                let closed = windows.remove(index);
                // The worker events from a closed window are dropped with its controllers.
                if closed.vpn_importing || closed.vpn_probing {
                    for browser in &mut windows {
                        browser.vpn_importing = false;
                        browser.vpn_probing = false;
                        browser.sync_toolbar();
                    }
                }
                if focused == closed.window.id() {
                    let next = &windows[index.min(windows.len()-1)].window;
                    focused = next.id();
                    next.set_focus();
                }
            }
        }
        // Restore children after the preceding shell has processed its own Ready
        // event. WebView2 pumps native messages while creating a controller.
        if !restore_failed && !pending_sessions.is_empty()
            && windows.iter().all(|browser| browser.ready && !browser.close_requested)
        {
            let source = &windows[0];
            let session = pending_sessions.front().expect("pending session").clone();
            let data = next_window_data(source.data.profile.clone(), session);
            let options = window_options(source, &options);
            match create_browser(target, &proxy, data, options, Some(source)) {
                Ok(mut browser) => {
                    browser.account_prompt_checked = true;
                    pending_sessions.pop_front();
                    windows.push(browser);
                    changed = true;
                }
                Err(error) => {
                    restore_failed = true;
                    source.notice(&error.to_string());
                }
            }
        }
        if let Some(index) = windows.iter().position(|browser| browser.new_window_requested) {
            windows[index].new_window_requested = false;
            if windows.len() + pending_sessions.len() < window_session::MAX_WINDOWS {
                let source = &windows[index];
                let data = next_window_data(source.data.profile.clone(), window_session::WindowSession {
                    tabs: vec![Tab::new()], ..Default::default()
                });
                let lens = window_options(source, &options);
                match create_browser(target, &proxy, data, lens, Some(source)) {
                    Ok(mut browser) => {
                        browser.account_prompt_checked = true;
                        focused = browser.window.id();
                        windows.push(browser);
                        changed = true;
                    }
                    Err(error) => windows[index].notice(&format!("{}: {error}", windows[index].ui_language.pick("Не удалось открыть окно", "Unable to open window"))),
                }
            } else {
                windows[index].notice_localized("Достигнут предел открытых окон.");
            }
        }
        if let Some(index) = windows.iter().position(|browser| browser.next_window_requested) {
            windows[index].next_window_requested = false;
            let next = &windows[(index+1)%windows.len()].window;
            next.set_minimized(false);
            next.set_focus();
        }

        let first = windows.iter().position(|browser| browser.window.id() == focused).unwrap_or(0);
        let restart = windows.iter_mut().any(|browser| std::mem::take(&mut browser.restart_requested));
        let dirty = windows.iter().fold(changed, |pending, browser| browser.dirty.replace(false) || pending);
        let placement_due = placement_save_at.is_some_and(|time| time <= now);
        if placement_due { placement_save_at = None; }
        let save = !private_mode && (dirty || placement_due || restart);
        let backup = windows.iter().position(|browser| browser.backup_requested);
        let mut persisted = false;
        if save || backup.is_some() {
            let snapshot = window_snapshot(&windows, first, &pending_sessions);
            if save {
                if let Some(path) = &data_file {
                    match snapshot.save(path) {
                        Ok(()) => { persisted = true; placement_save_at = None; }
                        Err(error) => {
                            eprintln!("Failed to persist browser state: {error}");
                            windows[first].notice_localized("Не удалось сохранить профиль. Проверьте свободное место и права доступа.");
                        }
                    }
                }
            }
            if let Some(index) = backup {
                windows[index].backup_requested = false;
                windows[index].write_profile_backup(&snapshot);
            }
        }
        if home_changed {
            windows.iter().for_each(|browser| browser.sync_home(persisted));
        }
        if restart {
            match std::env::current_exe().and_then(|exe| Command::new(exe).arg("--profile").arg(current_profile_id()).arg("--resume-after-restart").spawn()) {
                Ok(_) => {
                    #[cfg(windows)] { system_vpn.take(); vpn_tunnel.take(); }
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                Err(_) => windows[first].notice_localized("Настройки VPN сохранены. Закройте и заново запустите LiteRust для применения."),
            }
        }
        if windows.len() == 1 && windows[0].close_requested {
            #[cfg(windows)] { system_vpn.take(); vpn_tunnel.take(); }
            *control_flow = ControlFlow::Exit;
            return;
        }
        for browser in &mut windows { browser.animate_drawer(); }
        let animation_at = windows.iter().any(|browser| browser.dock_animation.is_some())
            .then(|| now + std::time::Duration::from_millis(16));
        let vpn_at = vpn_port.is_some().then(|| now + std::time::Duration::from_secs(2));
        *control_flow = [animation_at, placement_save_at, vpn_at].into_iter().flatten().min()
            .map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
    });
}

#[derive(Clone)]
struct WindowOptions {
    private_mode: bool,
    #[cfg(windows)]
    downloads: windows_platform::Downloads,
    local_profiles: Catalog,
    vpn_vault: std::rc::Rc<vpn::Vault>,
    vpn_port: Option<u16>,
    vpn_running: bool,
    system_vpn_active: bool,
}

fn create_browser(
    target: &EventLoopWindowTarget<WindowMessage>,
    event_loop: &EventLoopProxy<WindowMessage>,
    data: BrowserData,
    options: WindowOptions,
    sibling: Option<&Browser>,
) -> Result<Browser, Box<dyn std::error::Error>> {
    let private_mode = options.private_mode;
    let title = if private_mode {
        data.browser_settings
            .language
            .pick("LiteRust — приватное окно", "LiteRust — private window")
    } else {
        concat!("LiteRust ", env!("CARGO_PKG_VERSION"))
    };
    let mut builder = WindowBuilder::new()
        .with_maximized(true)
        .with_decorations(false)
        .with_visible(false)
        .with_background_color((16, 20, 33, 255))
        .with_window_icon(
            tao::window::Icon::from_rgba(
                include_bytes!("../packaging/default-icon.rgba").to_vec(),
                64,
                64,
            )
            .ok(),
        )
        .with_title(title)
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(LogicalSize::new(690.0, 400.0));
    if let Some(place) = data.placement.filter(|place| {
        target.available_monitors().any(|monitor| {
            let (pos, size) = (monitor.position(), monitor.size());
            place.overlaps(pos.x, pos.y, size.width, size.height)
        })
    }) {
        builder = builder
            .with_position(PhysicalPosition::new(place.x, place.y))
            .with_inner_size(PhysicalSize::new(place.width, place.height))
            .with_maximized(place.maximized);
    }
    let window = builder.build(target)?;
    let proxy = BrowserProxy {
        window: window.id(),
        event_loop: event_loop.clone(),
    };
    #[cfg(windows)]
    if !private_mode {
        windows_platform::install_window_request(&window, proxy.clone())?;
        windows_platform::register_profile_window(&window, &profile_root())?;
    }
    let catalog = options.local_profiles;
    let vpn_port = options.vpn_port;
    let context = (!private_mode).then(|| WebContext::new(Some(profile_root().join("WebView2"))));
    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
    let language = data.browser_settings.language;
    let toolbar_proxy = proxy.clone();
    startup_log("Creating toolbar WebView");
    let mut ui_context = WebContext::new(Some(profile_root().join("Interface-v2")));
    let toolbar_builder = if private_mode {
        WebViewBuilder::new()
    } else {
        WebViewBuilder::new_with_web_context(&mut ui_context)
    };
    #[cfg(windows)]
    let toolbar_builder = match sibling {
        Some(browser) => toolbar_builder.with_environment(browser.toolbar.environment()),
        None => toolbar_builder,
    };
    #[cfg(not(windows))]
    let _ = sibling;
    let toolbar_initial_document = std::cell::Cell::new(true);
    let toolbar = routed_builder(toolbar_builder, vpn_port)
        .with_visible(false)
        .with_background_color((16, 20, 33, 255))
        .with_incognito(private_mode)
        .with_permission_handler(|_| PermissionResponse::Deny)
        .with_html(security::control_document(
            &literust_browser::i18n::template(
                &TOOLBAR.replace("/* APPEARANCE_SCRIPT */", APPEARANCE_SCRIPT),
                language,
            ),
        )?)
        .with_bounds(rect(0.0, 0.0, size.width, TOOLBAR_HEIGHT))
        .with_navigation_handler(move |_url| {
            let allowed = toolbar_initial_document.replace(false);
            startup_log(if allowed {
                "Interface navigation allowed"
            } else {
                "Interface navigation rejected"
            });
            allowed
        })
        .with_on_page_load_handler(|event, _| {
            startup_log(if matches!(event, PageLoadEvent::Finished) {
                "Interface document finished"
            } else {
                "Interface document loading"
            })
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.uri() == "about:blank" && request.body().len() <= 2_100_000 {
                if let Ok(command) = serde_json::from_str::<UiCommand>(request.body()) {
                    if matches!(command, UiCommand::Ready) {
                        startup_log("Main toolbar IPC ready");
                    }
                    let _ = toolbar_proxy.send_event(AppEvent::Ui(command));
                }
            }
        })
        .build_as_child(&window)?;

    let dock_proxy = proxy.clone();
    let dock_builder = if private_mode {
        WebViewBuilder::new()
    } else {
        WebViewBuilder::new_with_web_context(&mut ui_context)
    };
    #[cfg(windows)]
    let dock_builder = dock_builder.with_environment(toolbar.environment());
    let dock_initial_document = std::cell::Cell::new(true);
    let dock = routed_builder(dock_builder, vpn_port)
        .with_visible(false)
        .with_background_color((16, 20, 33, 255))
        .with_incognito(private_mode)
        .with_permission_handler(|_| PermissionResponse::Deny)
        .with_html(security::control_document(
            &literust_browser::i18n::template(
                &DOCK.replace("/* APPEARANCE_SCRIPT */", APPEARANCE_SCRIPT),
                language,
            ),
        )?)
        .with_bounds(rect(
            0.0,
            TOOLBAR_HEIGHT,
            54.0,
            (size.height - TOOLBAR_HEIGHT).max(1.0),
        ))
        .with_navigation_handler(move |_url| {
            let allowed = dock_initial_document.replace(false);
            startup_log(if allowed {
                "Interface navigation allowed"
            } else {
                "Interface navigation rejected"
            });
            allowed
        })
        .with_on_page_load_handler(|event, _| {
            startup_log(if matches!(event, PageLoadEvent::Finished) {
                "Interface document finished"
            } else {
                "Interface document loading"
            })
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.uri() == "about:blank" && request.body().len() < 4096 {
                if let Ok(command) = serde_json::from_str::<UiCommand>(request.body()) {
                    // The dock only needs fixed navigation/settings actions.
                    if matches!(
                        command,
                        UiCommand::DockReady
                            | UiCommand::DockExpanded { .. }
                            | UiCommand::SetSidebar { .. }
                            | UiCommand::ShowAccounts
                            | UiCommand::NewTab
                            | UiCommand::NewPrivateWindow
                            | UiCommand::DuplicateTab
                            | UiCommand::PrintPage
                            | UiCommand::ShowTools
                            | UiCommand::Home
                            | UiCommand::ShowSecurity
                            | UiCommand::ShowSettings
                            | UiCommand::ShowAppearance
                            | UiCommand::ShowVpn
                            | UiCommand::ShowBookmarks
                            | UiCommand::ShowHistory
                            | UiCommand::ShowDownloads
                            | UiCommand::DiscardBackground
                    ) {
                        let _ = dock_proxy.send_event(AppEvent::Ui(command));
                    }
                }
            }
        })
        .build_as_child(&window)?;
    let mut browser = Browser::new(
        window,
        toolbar,
        dock,
        context,
        data,
        proxy,
        private_mode,
        catalog,
        #[cfg(windows)]
        options.downloads,
    );
    #[cfg(windows)]
    {
        windows_platform::install_shortcuts(
            &browser.toolbar,
            browser.proxy.clone(),
            browser.shortcuts.clone(),
        )?;
        windows_platform::install_shortcuts(
            &browser.dock,
            browser.proxy.clone(),
            browser.shortcuts.clone(),
        )?;
    }
    #[cfg(windows)]
    {
        browser.page_environment = sibling.and_then(|peer| peer.page_environment.clone());
    }
    browser.vpn_vault = options.vpn_vault;
    browser.vpn_port = options.vpn_port;
    browser.vpn_running = options.vpn_running;
    browser.system_vpn_active = options.system_vpn_active;
    browser.resize();
    startup_log("Toolbar WebView created; creating active page");
    browser.show_active();

    Ok(browser)
}

fn dispatch_browser_event(browser: &mut Browser, event: AppEvent) {
    match event {
        AppEvent::Ui(cmd) => browser.handle(cmd),
        AppEvent::ProfilesScanned { revision, result } => {
            browser.on_profiles_scanned(revision, result)
        }
        AppEvent::InternalCommand {
            tab_id,
            generation,
            command,
        } => {
            if browser.matching_view(tab_id, generation)
                && browser.active_id() == tab_id
                && browser.current_url() == NEW_TAB
            {
                browser.handle(command);
            }
        }
        AppEvent::NewTabAppearance {
            tab_id,
            generation,
            accounts,
        } => {
            if browser.matching_view(tab_id, generation)
                && browser
                    .data
                    .tabs
                    .iter()
                    .any(|t| t.id == tab_id && t.url == NEW_TAB)
            {
                browser.handle(if accounts {
                    UiCommand::ShowAccounts
                } else {
                    UiCommand::ShowSettings
                });
            }
        }
        #[cfg(windows)]
        AppEvent::PdfFinished { path, success } => {
            browser.enforce_page_budget();
            browser.sync_toolbar();
            if success {
                browser.notice(&format!(
                    "{}: {}",
                    browser.ui_language.pick("PDF сохранён", "PDF saved"),
                    path.display()
                ));
            } else {
                browser.notice_localized("WebView2 не смог сохранить PDF.");
            }
        }
        AppEvent::VpnProbed(result) => {
            browser.vpn_probing = false;
            browser.vpn_probe = if !browser.vpn_running {
                "VPN остановился".into()
            } else {
                match result {
                    Ok(ms) => format!("HTTPS через VPN: {ms} мс"),
                    Err(message) => message.into(),
                }
            };
            browser.sync_toolbar();
        }
        AppEvent::VpnImported(result) => {
            browser.vpn_importing = false;
            match result {
                Ok(nodes) => {
                    let system_scope = browser.vpn_vault.system_scope;
                    let previous = std::mem::replace(
                        &mut browser.vpn_vault,
                        std::rc::Rc::new(vpn::Vault {
                            nodes,
                            enabled: false,
                            selected: 0,
                            system_scope,
                        }),
                    );
                    #[cfg(windows)]
                    if literust_browser::vpn_platform::save(&profile_root(), &browser.vpn_vault)
                        .is_err()
                    {
                        browser.vpn_vault = previous;
                        browser.notice_localized("Не удалось сохранить VPN-ключи");
                    } else {
                        browser.notice_localized("Серверы импортированы и защищены учётной записью Windows. Выберите сервер и подключитесь.");
                    }
                    #[cfg(not(windows))]
                    {
                        browser.vpn_vault = previous;
                        browser.notice_localized("VPN-хаб доступен в Windows-сборке");
                    }
                }
                Err(message) => browser.notice_localized(message),
            }
            browser.sync_toolbar();
        }
        #[cfg(windows)]
        AppEvent::PageFocused { tab_id, generation } => {
            if browser.ready
                && !browser.appearance_open
                && browser.pages.get(&tab_id).is_some_and(|page| {
                    page.generation == generation && windows_platform::has_page_focus(&page.view)
                })
            {
                if let Some(pane) = browser.data.split.and_then(|split| split.pane(tab_id)) {
                    browser.focus_split_pane(pane);
                }
            }
        }
        AppEvent::Page {
            tab_id,
            generation,
            url,
            loading,
        } => {
            browser.on_page_event(tab_id, generation, url, loading);
        }
        AppEvent::Title {
            tab_id,
            generation,
            title,
        } => {
            browser.on_title(tab_id, generation, title);
        }
        AppEvent::Popup {
            tab_id,
            generation,
            url,
        } => {
            if browser.matching_view(tab_id, generation) {
                browser.new_tab(Some(&url));
            }
        }
        AppEvent::DownloadStarted { id, url, file } => browser.on_download_start(id, url, file),
        AppEvent::DownloadState {
            id,
            file,
            status,
            progress,
        } => browser.on_download_state(id, &file, status, progress),
        #[cfg(windows)]
        AppEvent::DownloadLimit => {
            browser.notice_localized("Достигнут лимит одновременных загрузок.")
        }
    }
}

fn window_options(source: &Browser, initial: &WindowOptions) -> WindowOptions {
    WindowOptions {
        local_profiles: source.local_profiles.clone(),
        vpn_vault: source.vpn_vault.clone(),
        vpn_running: source.vpn_running,
        system_vpn_active: source.system_vpn_active,
        ..initial.clone()
    }
}

fn window_snapshot(
    windows: &[Browser],
    first: usize,
    pending: &std::collections::VecDeque<window_session::WindowSession>,
) -> BrowserData {
    let initial = windows[first].session();
    BrowserData {
        tabs: initial.tabs,
        tab_groups: initial.tab_groups,
        split: initial.split,
        active: initial.active,
        placement: initial.placement,
        additional_windows: windows
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != first)
            .map(|(_, browser)| browser.session())
            .chain(pending.iter().cloned())
            .collect(),
        profile: windows[first].data.profile.clone(),
    }
}

fn next_window_data(
    profile: std::rc::Rc<storage::ProfileData>,
    session: window_session::WindowSession,
) -> BrowserData {
    BrowserData {
        tabs: session.tabs,
        tab_groups: session.tab_groups,
        split: session.split,
        active: session.active,
        placement: session.placement,
        additional_windows: Vec::new(),
        profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lru_never_evicts_active_tab() {
        let entries = vec![(10, 7), (20, 1), (30, 4)];
        assert_eq!(lru_victim(entries.clone(), 10), Some(20));
        assert_eq!(lru_victim(entries, 20), Some(30));
        assert_eq!(lru_victim([(10, 7)], 10), None);
    }
    #[test]
    fn google_manager_command_is_explicitly_opt_in() {
        assert!(matches!(
            serde_json::from_str::<UiCommand>(r#"{"action":"open_google_passwords"}"#),
            Ok(UiCommand::OpenGooglePasswords)
        ));
    }
    #[test]
    fn memory_mode_command_is_supported() {
        assert!(matches!(
            serde_json::from_str::<UiCommand>(r#"{"action":"cycle_memory_mode"}"#),
            Ok(UiCommand::CycleMemoryMode)
        ));
    }
    #[test]
    fn bookmark_export_and_download_cleanup_commands_are_supported() {
        assert!(matches!(
            serde_json::from_str::<UiCommand>(r#"{"action":"export_bookmarks"}"#),
            Ok(UiCommand::ExportBookmarks)
        ));
        assert!(matches!(
            serde_json::from_str::<UiCommand>(r#"{"action":"clear_download_history"}"#),
            Ok(UiCommand::ClearDownloadHistory)
        ));
    }
    #[test]
    fn site_adblock_toggle_command_is_supported() {
        assert!(matches!(
            serde_json::from_str::<UiCommand>(r#"{"action":"toggle_site_adblock"}"#),
            Ok(UiCommand::ToggleSiteAdblock)
        ));
    }
    #[test]
    fn input_search_or_url() {
        assert_eq!(destination("   "), NEW_TAB);
        assert_eq!(
            destination("https://example.com/a"),
            "https://example.com/a"
        );
        assert_eq!(destination("example.org"), "https://example.org/");
        assert_eq!(destination("localhost:8080"), "http://localhost:8080/");
        assert!(
            destination("mailto:test@example.com").starts_with("https://www.google.com/search?")
        );
        assert!(destination("javascript:alert(1)").starts_with("https://www.google.com/search?"));
        assert_eq!(destination("как работает Rust"), "https://www.google.com/search?q=%D0%BA%D0%B0%D0%BA+%D1%80%D0%B0%D0%B1%D0%BE%D1%82%D0%B0%D0%B5%D1%82+Rust");
    }
    #[test]
    fn block_untrusted_top_level_schemes() {
        assert!(!permitted_page_navigation("javascript:alert(1)"));
        assert!(!permitted_page_navigation("file:///C:/secret.txt"));
        assert!(!permitted_page_navigation("data:text/html,hi"));
        assert!(permitted_page_navigation("https://example.com/login"));
    }
    #[test]
    fn sensitive_permissions_are_not_silently_granted() {
        assert!(matches!(
            site_permission_policy(PermissionKind::Geolocation),
            PermissionResponse::Default
        ));
        assert!(matches!(
            site_permission_policy(PermissionKind::Microphone),
            PermissionResponse::Default
        ));
        assert!(matches!(
            site_permission_policy(PermissionKind::ClipboardRead),
            PermissionResponse::Deny
        ));
    }
    #[test]
    fn escaped_library_data() {
        let mut data = BrowserData::default();
        data.toggle_bookmark(
            "https://example.com/?q=<script>",
            "<script>alert('x')</script>",
        );
        let doc = render_library(BOOKMARKS, &data, storage::Language::Ru);
        assert!(doc.contains("&lt;script&gt;"));
        assert!(!doc.contains("<script>alert('x')</script>"));
    }
}
