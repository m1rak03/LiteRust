//! Persist only browsing metadata here. Never save passwords or cookies in JSON.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

pub const SETTINGS: &str = "literust://settings";
pub const APPEARANCE: &str = "literust://appearance";
pub const PASSWORDS: &str = "literust://passwords";
pub const ABOUT: &str = "literust://about";
pub const ACCOUNTS: &str = "literust://accounts";
pub fn service_panel(url: &str) -> Option<&'static str> {
    match url {
        SETTINGS => Some("settings"),
        BOOKMARKS => Some("bookmarks-manager"),
        DOWNLOADS => Some("downloads-manager"),
        APPEARANCE => Some("appearance"),
        PASSWORDS => Some("passwords"),
        ABOUT => Some("about-page"),
        ACCOUNTS => Some("accounts"),
        _ => None,
    }
}
pub const NEW_TAB: &str = "literust://newtab";
pub const BOOKMARKS: &str = "literust://bookmarks";
pub const HISTORY: &str = "literust://history";
pub const DOWNLOADS: &str = "literust://downloads";
pub const MAX_TABS: usize = 24;
pub const MAX_BOOKMARKS: usize = 1000;
const MAX_HISTORY: usize = 1500;
pub const MAX_DOWNLOADS: usize = 300;
const MAX_BOOKMARK_FOLDERS: usize = 64;
pub(crate) const MAX_BROWSER_DATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_URL_LEN: usize = 8192;
const MAX_ADBLOCK_EXCEPTIONS: usize = 256;
const DEFAULT_LIVE_TAB_BUDGET: u8 = 3;

fn default_live_tab_budget() -> u8 {
    DEFAULT_LIVE_TAB_BUDGET
}

pub fn valid_web_url(input: &str) -> bool {
    input.len() <= MAX_URL_LEN
        && !input.chars().any(char::is_control)
        && Url::parse(input).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

pub fn valid_tab_url(input: &str) -> bool {
    matches!(
        input,
        NEW_TAB
            | BOOKMARKS
            | HISTORY
            | DOWNLOADS
            | SETTINGS
            | APPEARANCE
            | PASSWORDS
            | ABOUT
            | ACCOUNTS
    ) || valid_web_url(input)
}

/// Host only, never URL userinfo/path. Explicitly no parent-domain inheritance:
/// allowing ads on example.org does NOT whitelist sub.example.org.
pub fn site_host(input: &str) -> Option<String> {
    if !valid_web_url(input) {
        return None;
    }
    Url::parse(input)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

fn normalise_host(input: &str) -> Option<String> {
    // Only hostnames are persisted, not arbitrary URLs (so no path, credentials
    // or wildcard could sneak into the exception list from an edited JSON).
    if input.len() > 253
        || input.is_empty()
        || input
            .chars()
            .any(|c| matches!(c, '/' | '@' | ':' | '*' | '?' | '#' | '\\'))
    {
        return None;
    }
    let candidate = format!("https://{input}/");
    let host = site_host(&candidate)?;
    if host != input.to_ascii_lowercase() {
        return None;
    }
    Some(host)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    #[serde(default)]
    pub id: u64,
    pub url: String,
    pub title: String,
    // History is tab-local. WebView2 native history cannot be shared safely across
    // "sleeping" URL-only tabs that all reuse one live WebView.
    #[serde(default)]
    pub back: Vec<String>,
    #[serde(default)]
    pub forward: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_position: Option<[i32; 2]>,
}

impl Default for Tab {
    fn default() -> Self {
        Self::new()
    }
}

impl Tab {
    pub fn new() -> Self {
        Self {
            id: 0,
            group: None,
            url: NEW_TAB.to_owned(),
            title: "Новая вкладка".into(),
            back: Vec::new(),
            forward: Vec::new(),
            scroll_position: None,
        }
    }
    pub fn web(url: &str) -> Self {
        Self {
            id: 0,
            group: None,
            url: url.into(),
            title: Self::title_for(url),
            back: Vec::new(),
            forward: Vec::new(),
            scroll_position: None,
        }
    }
    pub fn navigate(&mut self, next: &str) {
        if !valid_tab_url(next) || self.url == next {
            return;
        }
        if valid_tab_url(&self.url) {
            self.back.push(self.url.clone());
        }
        if self.back.len() > 40 {
            self.back.remove(0);
        }
        self.forward.clear();
        self.url = next.into();
        self.scroll_position = None;
        self.title = match next {
            NEW_TAB | BOOKMARKS | HISTORY | DOWNLOADS | SETTINGS | APPEARANCE | PASSWORDS
            | ABOUT | ACCOUNTS => Self::internal(next).title,
            _ => Self::title_for(next),
        };
    }
    pub fn go_back(&mut self) -> bool {
        if let Some(previous) = self.back.pop() {
            self.forward.push(self.url.clone());
            self.url = previous;
            self.scroll_position = None;
            self.title = Self::title_for(&self.url);
            true
        } else {
            false
        }
    }
    pub fn go_forward(&mut self) -> bool {
        if let Some(next) = self.forward.pop() {
            self.back.push(self.url.clone());
            self.url = next;
            self.scroll_position = None;
            self.title = Self::title_for(&self.url);
            true
        } else {
            false
        }
    }
    fn title_for(url: &str) -> String {
        match url {
            NEW_TAB | BOOKMARKS | HISTORY | DOWNLOADS | SETTINGS | APPEARANCE | PASSWORDS
            | ABOUT | ACCOUNTS => Self::internal(url).title,
            _ => Url::parse(url)
                .ok()
                .and_then(|value| value.host_str().map(str::to_owned))
                .unwrap_or_else(|| "Сайт".to_owned()),
        }
    }
    pub fn internal(url: &str) -> Self {
        let title = match url {
            SETTINGS => "Настройки",
            APPEARANCE => "Оформление",
            PASSWORDS => "Пароли",
            ABOUT => "О браузере",
            ACCOUNTS => "Аккаунты",
            BOOKMARKS => "Закладки",
            HISTORY => "История",
            DOWNLOADS => "Загрузки",
            _ => "Новая вкладка",
        };
        Self {
            id: 0,
            group: None,
            url: url.into(),
            title: title.into(),
            back: Vec::new(),
            forward: Vec::new(),
            scroll_position: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub added_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    #[serde(default)]
    pub id: u64,
    pub url: String,
    pub file: String,
    pub status: String,
    pub started_at: u64,
    #[serde(default)]
    pub received: u64,
    #[serde(default)]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Running,
    Paused,
    Completed,
    Cancelled,
    #[default]
    Interrupted,
}

impl DownloadStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "Загрузка",
            Self::Paused => "Приостановлено",
            Self::Completed => "Готово",
            Self::Cancelled => "Отменено",
            Self::Interrupted => "Прервано",
        }
    }
}

impl DownloadEntry {
    pub fn state(&self) -> DownloadStatus {
        match self.status.as_str() {
            "Загрузка" => DownloadStatus::Running,
            "Приостановлено" => DownloadStatus::Paused,
            "Готово" => DownloadStatus::Completed,
            "Отменено" => DownloadStatus::Cancelled,
            _ => DownloadStatus::Interrupted,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: Option<u64>,
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_cancel: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadAction {
    Pause,
    Resume,
    Cancel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Customization {
    pub animations: bool,
    pub compact_tabs: bool,
    pub show_clock: bool,
    pub show_greeting: bool,
    pub wallpaper_dim: u8,
    pub accent: Option<u32>,
    pub background_color: Option<u32>,
    pub palette: [Option<u32>; 32],
    pub font_size: u8,
    pub corner_radius: u8,
    pub shortcut_count: u8,
    pub show_shortcuts: bool,
    pub show_services: bool,
    pub home_layout: u8,
    pub wallpaper_contain: bool,
    pub wallpaper_blur: u8,
    pub clock_24: bool,
    pub custom_tab_shape: bool,
}
impl Default for Customization {
    fn default() -> Self {
        Self {
            animations: true,
            compact_tabs: false,
            show_clock: true,
            show_greeting: true,
            wallpaper_dim: 55,
            accent: None,
            background_color: None,
            palette: [None; 32],
            font_size: 13,
            corner_radius: 12,
            shortcut_count: 10,
            show_shortcuts: true,
            show_services: true,
            home_layout: 0,
            wallpaper_contain: false,
            wallpaper_blur: 0,
            clock_24: true,
            custom_tab_shape: false,
        }
    }
}

impl Customization {
    pub fn validated(mut self) -> Self {
        self.wallpaper_dim = self.wallpaper_dim.min(90);
        self.background_color = self.background_color.filter(|v| *v <= 0xffffff);
        self.accent = self.accent.filter(|v| *v <= 0xffffff);
        self.font_size = self.font_size.clamp(12, 16);
        self.corner_radius = self.corner_radius.min(24);
        self.shortcut_count = self.shortcut_count.clamp(4, 20);
        self.home_layout = self.home_layout.min(2);
        self.wallpaper_blur = self.wallpaper_blur.min(20);
        self
    }
}
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Startup {
    #[default]
    Restore,
    NewTab,
    Home,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Ru,
    En,
}

impl Language {
    pub fn pick<'a>(self, russian: &'a str, english: &'a str) -> &'a str {
        match self {
            Self::Ru => russian,
            Self::En => english,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub language: Language,
    pub home_url: String,
    pub startup: Startup,
    pub home_in_new_tabs: bool,
    pub background_links: bool,
    pub history_enabled: bool,
    pub zoom_percent: u16,
}
impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            home_url: String::new(),
            startup: Startup::Restore,
            home_in_new_tabs: false,
            background_links: false,
            history_enabled: true,
            zoom_percent: 100,
        }
    }
}
impl BrowserSettings {
    pub fn valid_home(&self) -> bool {
        self.home_url.is_empty()
            || (valid_web_url(&self.home_url)
                && crate::security::safe_top_level_navigation(&self.home_url))
    }
    pub fn validated(mut self) -> Self {
        self.home_url = self.home_url.trim().to_owned();
        if !self.valid_home() {
            self.home_url.clear();
        }
        self.zoom_percent = self.zoom_percent.clamp(50, 200);
        self
    }
    pub fn home(&self) -> &str {
        if self.home_url.is_empty() {
            NEW_TAB
        } else {
            &self.home_url
        }
    }
}

/// Background colors are typed RGB values; arbitrary CSS is never accepted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct BackgroundStyle {
    pub mode: u8,
    pub color_a: u32,
    pub color_b: u32,
    pub angle: u16,
}
impl Default for BackgroundStyle {
    fn default() -> Self {
        Self {
            mode: 0,
            color_a: 0x182238,
            color_b: 0x5b3c88,
            angle: 135,
        }
    }
}
impl BackgroundStyle {
    pub fn validated(mut self) -> Self {
        self.mode = self.mode.min(3);
        self.color_a &= 0xffffff;
        self.color_b &= 0xffffff;
        self.angle = self.angle.min(360);
        self
    }
    pub fn css(self) -> String {
        let s = self.validated();
        match s.mode {
            1 => format!("background:#{:06x}!important", s.color_a),
            2 => format!(
                "background:linear-gradient({}deg,#{:06x},#{:06x})!important",
                s.angle, s.color_a, s.color_b
            ),
            3 => format!(
                "background:radial-gradient(ellipse at top left,#{:06x},#{:06x})!important",
                s.color_a, s.color_b
            ),
            _ => String::new(),
        }
    }
}

/// Indices refer to the bundled, fixed palette catalog; never arbitrary CSS.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub design: u8,
    pub theme: u16,
}
impl Appearance {
    pub fn validated(self) -> Self {
        Self {
            design: if self.design < 30 { self.design } else { 0 },
            theme: if self.theme < 330 { self.theme } else { 0 },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub background_color: Option<u32>,
    pub accent: Option<u32>,
    pub palette: [Option<u32>; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserData {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub placement: Option<crate::window_session::Placement>,
    pub additional_windows: Vec<crate::window_session::WindowSession>,
    pub tab_groups: crate::tab_groups::TabGroups,
    pub split: Option<crate::split_view::SplitView>,
    #[serde(skip_serializing_if = "crate::workspaces::WindowWorkspaces::is_empty")]
    pub workspace: crate::workspaces::WindowWorkspaces,
    #[serde(flatten)]
    pub profile: std::rc::Rc<ProfileData>,
}

impl Default for BrowserData {
    fn default() -> Self {
        Self {
            tabs: vec![Tab::new()],
            active: 0,
            placement: None,
            additional_windows: Vec::new(),
            tab_groups: Default::default(),
            split: None,
            workspace: Default::default(),
            profile: Default::default(),
        }
    }
}

impl std::ops::Deref for BrowserData {
    type Target = ProfileData;

    fn deref(&self) -> &Self::Target {
        &self.profile
    }
}

impl std::ops::DerefMut for BrowserData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::rc::Rc::make_mut(&mut self.profile)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileData {
    #[serde(skip_serializing_if = "crate::workspaces::Catalog::is_empty")]
    pub workspaces: crate::workspaces::Catalog,
    pub home: Option<crate::home::Dashboard>,
    pub sidebar: crate::layout::Sidebar,
    pub shortcuts: crate::shortcuts::Shortcuts,
    pub custom_icons: std::collections::BTreeMap<String, String>,
    pub app_icon_rgba: String,
    #[serde(default)]
    pub password_autosave: bool,
    pub saved_palettes: std::collections::BTreeMap<String, Palette>,
    pub customization: Customization,
    pub browser_settings: BrowserSettings,
    pub background: String,
    pub background_style: BackgroundStyle,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(skip_serializing_if = "crate::notes::Notebook::is_empty")]
    pub notebook: crate::notes::Notebook,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden_tools: Vec<String>,
    pub account_prompt_seen: bool,
    pub account_prompt_always: bool,
    pub search_engine: crate::navigation::SearchEngine,
    pub appearance: Appearance,
    pub dock_position: crate::layout::DockPosition,
    pub bookmarks: Vec<Bookmark>,
    pub bookmark_folders: Vec<String>,
    pub history: Vec<HistoryEntry>,
    pub downloads: Vec<DownloadEntry>,
    pub cosmetic_filter: bool,
    pub network_blocking: bool,
    /// Exact hostnames for which experimental network blocking is turned off.
    /// No passwords, URLs, cookies or path data are stored in this list.
    pub adblock_exceptions: Vec<String>,
    #[serde(default = "default_live_tab_budget")]
    pub live_tab_budget: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_mode: Option<crate::tab_resources::Mode>,
    pub background_policy: crate::tab_resources::BackgroundPolicy,
    /// Retained for importing and opening profiles from earlier versions.
    pub auto_discard: bool,
}

impl Default for ProfileData {
    fn default() -> Self {
        Self {
            workspaces: Default::default(),
            home: None,
            sidebar: crate::layout::Sidebar::default(),
            shortcuts: crate::shortcuts::Shortcuts::default(),
            custom_icons: std::collections::BTreeMap::new(),
            app_icon_rgba: String::new(),
            password_autosave: false,
            saved_palettes: Default::default(),
            customization: Customization::default(),
            browser_settings: BrowserSettings::default(),
            background: String::new(),
            background_style: BackgroundStyle::default(),
            notes: String::new(),
            notebook: Default::default(),
            hidden_tools: Vec::new(),
            account_prompt_seen: false,
            account_prompt_always: false,
            search_engine: crate::navigation::SearchEngine::default(),
            appearance: Appearance::default(),
            dock_position: crate::layout::DockPosition::default(),
            bookmarks: vec![],
            bookmark_folders: vec![],
            history: vec![],
            downloads: vec![],
            cosmetic_filter: false,
            network_blocking: false,
            adblock_exceptions: vec![],
            live_tab_budget: DEFAULT_LIVE_TAB_BUDGET,
            resource_mode: None,
            background_policy: Default::default(),
            auto_discard: false,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SnapshotSource {
    Current,
    Legacy,
    Backup,
    Missing,
}

impl BrowserData {
    pub fn active_workspace(&self) -> Option<&crate::workspaces::Workspace> {
        self.workspace
            .current
            .and_then(|id| self.workspaces.get(id))
    }

    pub fn active_home(&self) -> Option<&crate::home::Dashboard> {
        self.active_workspace()
            .and_then(|workspace| workspace.home.as_deref())
            .or(self.home.as_ref())
    }

    pub fn display_customization(&self) -> Customization {
        let mut customization = self.customization;
        if let Some(space) = self.active_workspace() {
            customization.accent = space.display.accent.or(customization.accent);
            customization.compact_tabs = space
                .display
                .compact_tabs
                .unwrap_or(customization.compact_tabs);
        }
        customization
    }

    pub fn tab_session(&self) -> crate::workspaces::TabSession {
        crate::workspaces::TabSession {
            tabs: self.tabs.clone(),
            tab_groups: self.tab_groups.clone(),
            split: self.split,
            active: self.active,
        }
    }

    pub fn exchange_tab_session(
        &mut self,
        mut session: crate::workspaces::TabSession,
    ) -> crate::workspaces::TabSession {
        std::mem::swap(&mut self.tabs, &mut session.tabs);
        std::mem::swap(&mut self.active, &mut session.active);
        std::mem::swap(&mut self.tab_groups, &mut session.tab_groups);
        std::mem::swap(&mut self.split, &mut session.split);
        session
    }

    pub fn apply_startup(&mut self, resume: bool) {
        if resume || self.browser_settings.startup == Startup::Restore {
            return;
        }
        // Startup preferences reset visible windows, while named workspaces
        // remain available through the workspace switcher.
        let mut saved = Vec::new();
        if let Some(id) = self.workspace.current {
            saved.push((id, self.tab_session()));
        }
        for window in std::mem::take(&mut self.additional_windows) {
            if let Some(id) = window.workspace.current {
                saved.push((
                    id,
                    crate::workspaces::TabSession {
                        tabs: window.tabs,
                        tab_groups: window.tab_groups,
                        split: window.split,
                        active: window.active,
                    },
                ));
            }
        }
        for (id, session) in saved {
            if let Some(space) = self.workspaces.get_mut(id) {
                space.session = Some(std::rc::Rc::new(session));
            }
        }
        self.workspace = Default::default();
        let url = if self.browser_settings.startup == Startup::Home {
            self.browser_settings.home()
        } else {
            NEW_TAB
        };
        self.tabs = vec![if valid_web_url(url) {
            Tab::web(url)
        } else {
            Tab::new()
        }];
        self.active = 0;
        self.tab_groups = Default::default();
        self.split = None;
    }

    fn read_data(path: &Path) -> io::Result<(Self, bool)> {
        let bytes = crate::profile::read_limited(path, MAX_BROWSER_DATA_BYTES)?;
        let current = crate::protected_data::is_current_format(&bytes);
        serde_json::from_slice::<Self>(&crate::protected_data::open(&bytes)?)
            .map(|data| (data, current))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    /// Fail closed if existing files cannot be recovered.
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::load_snapshot(path)?.0)
    }

    /// Call with the profile lock held. Preserve a damaged primary before restoring it.
    pub fn recover(path: &Path) -> io::Result<Self> {
        let (data, source) = Self::load_snapshot(path)?;
        if source == SnapshotSource::Backup {
            data.restore_backup(path)?;
        }
        Ok(data)
    }

    /// Keep an existing sealed session intact until an actual change is saved.
    /// Legacy profiles and explicit startup resets are committed before browsing.
    pub fn load_for_startup(path: &Path, resume: bool) -> io::Result<Self> {
        let (mut data, source) = Self::load_snapshot(path)?;
        let resets_session = !resume && data.browser_settings.startup != Startup::Restore;
        data.apply_startup(resume);
        if source == SnapshotSource::Backup {
            data.restore_backup(path)?;
        } else if source != SnapshotSource::Current || resets_session {
            data.save(path)?;
        }
        Ok(data)
    }

    fn restore_backup(&self, path: &Path) -> io::Result<()> {
        let bytes = self.protected_snapshot()?;
        crate::profile::preserve_damaged(path)?;
        crate::profile::atomic_write(path, &bytes)
    }

    fn load_snapshot(path: &Path) -> io::Result<(Self, SnapshotSource)> {
        let backup = path.with_extension("json.bak");
        match Self::read_data(path) {
            Ok((data, current)) => Ok((
                data.clean(),
                if current {
                    SnapshotSource::Current
                } else {
                    SnapshotSource::Legacy
                },
            )),
            Err(primary_error) => match Self::read_data(&backup) {
                Ok((data, _)) => Ok((data.clean(), SnapshotSource::Backup)),
                Err(backup_error)
                    if primary_error.kind() == io::ErrorKind::NotFound
                        && backup_error.kind() == io::ErrorKind::NotFound =>
                {
                    Ok((Self::default().clean(), SnapshotSource::Missing))
                }
                Err(_) => Err(primary_error),
            },
        }
    }

    pub fn clean(mut self) -> Self {
        if self.home.as_ref().is_some_and(|home| !home.valid()) {
            self.home = None;
        }
        self.sidebar = std::mem::take(&mut self.sidebar).validated();
        self.custom_icons.retain(|key, value| {
            crate::icons::valid_target(key) && crate::icons::png_bytes(value).is_some()
        });
        while self.custom_icons.len() > 32 {
            self.custom_icons.pop_last();
        }
        if self.app_icon_rgba.len() > 22_000 {
            self.app_icon_rgba.clear();
        }

        self.background_style = self.background_style.validated();
        let mut legacy_notes = std::mem::take(&mut self.notes);
        self.notebook.clean(&mut legacy_notes);
        self.notes = legacy_notes;
        let hidden_tools = std::mem::take(&mut self.hidden_tools);
        self.hidden_tools = crate::page_tools::hidden_tools(hidden_tools);
        self.customization = self.customization.validated();
        self.browser_settings = std::mem::take(&mut self.browser_settings).validated();
        self.shortcuts.0.remove("fullscreen"); // Retire legacy fullscreen without resetting other assignments.
        self.shortcuts.add_window_shortcut();
        if !self.shortcuts.validate() {
            self.shortcuts = crate::shortcuts::Shortcuts::default();
        }
        self.appearance = self.appearance.validated();
        if !matches!(self.live_tab_budget, 1 | 3 | 5) {
            self.live_tab_budget = DEFAULT_LIVE_TAB_BUDGET;
        }
        crate::workspaces::validate_tabs(
            &mut self.tabs,
            &mut self.active,
            &mut self.tab_groups,
            &mut self.split,
        );
        self.workspaces.clean();
        let catalog = &self.profile.workspaces;
        let mut claimed = std::collections::HashSet::new();
        self.workspace.clean(catalog, &mut claimed);
        self.placement = self.placement.filter(|placement| placement.valid());
        self.additional_windows
            .truncate(crate::window_session::MAX_WINDOWS - 1);
        for window in &mut self.additional_windows {
            window.clean();
            window.workspace.clean(catalog, &mut claimed);
        }
        let mut seen_hosts = std::collections::HashSet::new();
        self.adblock_exceptions = std::mem::take(&mut self.adblock_exceptions)
            .into_iter()
            .filter_map(|value| normalise_host(&value))
            .filter(|host| seen_hosts.insert(host.clone()))
            .take(MAX_ADBLOCK_EXCEPTIONS)
            .collect();
        let mut folder_names = std::collections::HashSet::new();
        self.bookmark_folders = std::mem::take(&mut self.bookmark_folders)
            .into_iter()
            .filter_map(|name| bookmark_folder_name(&name))
            .filter(|name| folder_names.insert(name.to_lowercase()))
            .take(MAX_BOOKMARK_FOLDERS)
            .collect();
        let folders = self.bookmark_folders.clone();
        let mut seen_bookmarks = std::collections::HashSet::new();
        self.bookmarks.retain_mut(|b| {
            if !valid_web_url(&b.url) {
                return false;
            }
            b.url = Url::parse(&b.url)
                .map(|url| url.to_string())
                .unwrap_or_default();
            seen_bookmarks.insert(b.url.clone())
        });
        self.bookmarks.truncate(MAX_BOOKMARKS);
        for b in &mut self.bookmarks {
            b.title = compact_title(&b.title);
            b.folder = b
                .folder
                .take()
                .and_then(|folder| folders.iter().find(|name| name.as_str() == folder).cloned());
        }
        self.history.retain(|h| valid_web_url(&h.url));
        if self.history.len() > MAX_HISTORY {
            let excess = self.history.len() - MAX_HISTORY;
            self.history.drain(..excess);
        }
        for h in &mut self.history {
            h.title = compact_title(&h.title);
        }
        if self.downloads.len() > MAX_DOWNLOADS {
            let excess = self.downloads.len() - MAX_DOWNLOADS;
            self.downloads.drain(..excess);
        }
        // A previous process can terminate mid-download. Never leave a stale
        // indefinite "Downloading" label after restarting the browser.
        for (index, entry) in self.downloads.iter_mut().enumerate() {
            // Downloads from older versions used IDs scoped to a process.
            entry.id = index as u64 + 1;
            if matches!(
                entry.state(),
                DownloadStatus::Running | DownloadStatus::Paused
            ) {
                entry.status = "Прервано".into();
            }
        }
        self
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = self.protected_snapshot()?;
        let backup = path.with_extension("json.bak");
        let should_refresh_backup = fs::metadata(&backup)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_none_or(|elapsed| elapsed >= std::time::Duration::from_secs(120));
        if should_refresh_backup {
            if let Ok((previous, _)) = Self::read_data(path) {
                crate::profile::atomic_write(&backup, &previous.protected_snapshot()?)?;
            }
        }
        crate::profile::atomic_write(path, &encoded)?;
        Ok(())
    }

    fn protected_snapshot(&self) -> io::Result<Vec<u8>> {
        let encoded = serde_json::to_vec(self).map_err(io::Error::other)?;
        if encoded.len() as u64 > MAX_BROWSER_DATA_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "browser state is too large",
            ));
        }
        crate::protected_data::seal(&encoded)
    }

    /// Create a new account-bound backup of LiteRust metadata.
    pub fn backup_metadata(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        let bytes = self.protected_snapshot()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(())
    }

    pub fn bookmarks_as_html(&self) -> String {
        // Standard Netscape format understood by Chrome and many other browsers.
        // Escape URLs and titles: even manually edited profile JSON is untrusted.
        let mut html = String::from("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>LiteRust bookmarks</TITLE>\n<H1>LiteRust bookmarks</H1>\n<DL><p>\n");
        let write_items = |html: &mut String, folder: Option<&str>| {
            for item in &self.bookmarks {
                if item.folder.as_deref() != folder || !valid_web_url(&item.url) {
                    continue;
                }
                html.push_str(&format!(
                    "<DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\n",
                    escape_html(&item.url),
                    item.added_at,
                    escape_html(&item.title)
                ));
            }
        };
        write_items(&mut html, None);
        for folder in &self.bookmark_folders {
            html.push_str(&format!("<DT><H3>{}</H3>\n<DL><p>\n", escape_html(folder)));
            write_items(&mut html, Some(folder));
            html.push_str("</DL><p>\n");
        }
        html.push_str("</DL><p>\n");
        html
    }

    pub fn cycle_live_tab_budget(&mut self) -> u8 {
        self.live_tab_budget = match self.live_tab_budget {
            1 => 3,
            3 => 5,
            _ => 1,
        };
        self.live_tab_budget
    }

    pub fn adblock_exception_for(&self, url: &str) -> bool {
        site_host(url).is_some_and(|host| self.adblock_exceptions.contains(&host))
    }

    /// Returns the new exception state, or None for an unsupported/internal URL.
    pub fn toggle_adblock_exception(&mut self, url: &str) -> Option<bool> {
        let host = site_host(url)?;
        if let Some(index) = self
            .adblock_exceptions
            .iter()
            .position(|stored| stored == &host)
        {
            self.adblock_exceptions.remove(index);
            Some(false)
        } else if self.adblock_exceptions.len() < MAX_ADBLOCK_EXCEPTIONS {
            self.adblock_exceptions.push(host);
            Some(true)
        } else {
            None
        }
    }

    pub fn bookmarked(&self, url: &str) -> bool {
        let normalized = Url::parse(url).map(|u| u.to_string()).unwrap_or_default();
        self.bookmarks.iter().any(|b| b.url == normalized)
    }

    pub fn toggle_bookmark(&mut self, url: &str, title: &str) {
        if !valid_web_url(url) {
            return;
        }
        let normalized = Url::parse(url).map(|u| u.to_string()).unwrap_or_default();
        let url = normalized.as_str();
        if let Some(pos) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(pos);
        } else if valid_web_url(url) {
            if self.bookmarks.len() >= MAX_BOOKMARKS {
                return;
            }
            self.bookmarks.push(Bookmark {
                url: url.into(),
                title: compact_title(title),
                added_at: now(),
                folder: None,
            });
        }
    }

    pub fn update_bookmark(&mut self, url: &str, title: &str, folder: Option<String>) -> bool {
        if folder
            .as_ref()
            .is_some_and(|name| !self.bookmark_folders.contains(name))
        {
            return false;
        }
        let Some(item) = self.bookmarks.iter_mut().find(|item| item.url == url) else {
            return false;
        };
        item.title = compact_title(title.trim());
        item.folder = folder;
        true
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|bookmark| bookmark.url != url);
    }

    pub fn update_bookmark_folder(&mut self, previous: Option<&str>, name: Option<&str>) -> bool {
        let name = match name {
            Some(name) => match bookmark_folder_name(name) {
                Some(name) => Some(name),
                None => return false,
            },
            None => None,
        };
        let folders = &self.bookmark_folders;
        if let Some(name) = &name {
            if folders.iter().any(|current| {
                Some(current.as_str()) != previous && current.to_lowercase() == name.to_lowercase()
            }) {
                return false;
            }
        }
        if let Some(previous) = previous {
            let Some(index) = folders.iter().position(|folder| folder == previous) else {
                return false;
            };
            if let Some(name) = &name {
                self.bookmark_folders[index].clone_from(name);
            } else {
                self.bookmark_folders.remove(index);
            }
            for bookmark in &mut self.bookmarks {
                if bookmark.folder.as_deref() == Some(previous) {
                    bookmark.folder.clone_from(&name);
                }
            }
        } else if let Some(name) = name.filter(|_| folders.len() < MAX_BOOKMARK_FOLDERS) {
            self.bookmark_folders.push(name);
        } else {
            return false;
        }
        true
    }

    pub fn update_history_title(&mut self, url: &str, title: &str) {
        if self.browser_settings.history_enabled {
            if let Some(entry) = self.history.iter_mut().rev().find(|h| h.url == url) {
                entry.title = compact_title(title);
            }
        }
    }
    pub fn record_visit(&mut self, url: &str, title: &str) {
        if !self.browser_settings.history_enabled {
            return;
        }
        if !valid_web_url(url) {
            return;
        }
        let current = now();
        if let Some(last) = self.history.last_mut() {
            if last.url == url && current.saturating_sub(last.visited_at) < 5 {
                last.visited_at = current;
                last.title = compact_title(title);
                return;
            }
        }
        self.history.push(HistoryEntry {
            url: url.into(),
            title: compact_title(title),
            visited_at: current,
        });
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    pub fn record_download_start(&mut self, id: u64, url: String, file: String) {
        self.downloads.push(DownloadEntry {
            id,
            url,
            file,
            status: DownloadStatus::Running.label().into(),
            started_at: now(),
            received: 0,
            total: None,
        });
    }

    pub fn record_download_state(
        &mut self,
        id: u64,
        file: &Path,
        status: DownloadStatus,
        progress: DownloadProgress,
    ) {
        // The URL is not an operation identifier. Callbacks can arrive in any order.
        if let Some(entry) = self.downloads.iter_mut().rev().find(|d| d.id == id) {
            entry.status = status.label().into();
            if !file.as_os_str().is_empty() {
                entry.file = file.to_string_lossy().into_owned();
            }
            entry.received = progress.received;
            entry.total = progress.total;
        }
    }
}

fn bookmark_folder_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= 60 && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
}

pub fn compact_title(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect()
}

pub fn escape_html(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrecoverable_profile_is_not_silently_reset() {
        let root = std::env::temp_dir().join(format!("literust-corrupt-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("browser-data.json");
        fs::write(&file, b"{bad").unwrap();
        fs::write(file.with_extension("json.bak"), b"bad too").unwrap();
        assert!(BrowserData::load(&file).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"{bad");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn download_callbacks_are_correlated_and_do_not_undo_clear() {
        let mut data = BrowserData::default();
        data.record_download_start(1, "https://example.org/file".into(), "one".into());
        data.record_download_start(2, "https://example.org/file".into(), "two".into());
        data.record_download_state(
            1,
            Path::new("chosen-one"),
            DownloadStatus::Completed,
            DownloadProgress::default(),
        );
        assert_eq!(data.downloads[0].status, "Готово");
        assert_eq!(data.downloads[1].status, "Загрузка");
        data.downloads.clear();
        data.record_download_state(
            2,
            Path::new("two"),
            DownloadStatus::Completed,
            DownloadProgress::default(),
        );
        assert!(data.downloads.is_empty());
    }
    #[test]
    fn canonical_bookmarks_and_capacity_never_drop_existing() {
        let mut data = BrowserData::default();
        data.toggle_bookmark("https://EXAMPLE.org", "one");
        assert!(data.bookmarked("https://example.org/"));
        data.toggle_bookmark("https://example.org/", "one");
        assert!(data.bookmarks.is_empty());
        for n in 0..MAX_BOOKMARKS {
            data.toggle_bookmark(&format!("https://example.org/{n}"), "x");
        }
        data.toggle_bookmark("https://new.example/", "new");
        assert_eq!(data.bookmarks.len(), MAX_BOOKMARKS);
        assert!(data.bookmarked("https://example.org/0"));
        assert!(!data.bookmarked("https://new.example/"));
    }
    #[test]
    fn hostile_ids_and_old_settings_are_safe() {
        let mut data: BrowserData = serde_json::from_str(r#"{"tabs":[{"id":18446744073709551615,"url":"literust://newtab","title":"x"},{"id":18446744073709551615,"url":"literust://newtab","title":"x"}]}"#).unwrap();
        data = data.clean();
        assert_eq!(data.tabs[0].id, 1);
        assert_eq!(data.tabs[1].id, 2);
        assert!(!data.auto_discard);
    }
    #[test]
    fn restricts_restored_tabs() {
        let d = BrowserData {
            tabs: vec![Tab {
                group: None,
                id: 0,
                url: "javascript:alert(1)".into(),
                title: "BAD".into(),
                back: vec![],
                forward: vec![],
                scroll_position: None,
            }],
            ..BrowserData::default()
        };
        assert_eq!(d.clean().tabs[0].url, NEW_TAB);
    }
    #[test]
    fn old_bookmarks_gain_folders_without_losing_titles_or_addresses() {
        let mut data: BrowserData = serde_json::from_str(
            r#"{"bookmarks":[{"url":"https://example.com/","title":"Original","added_at":1}]}"#,
        )
        .unwrap();
        assert!(data.bookmark_folders.is_empty());
        assert_eq!(data.bookmarks[0].folder, None);
        assert!(data.update_bookmark_folder(None, Some("Work & Study")));
        assert!(!data.update_bookmark_folder(None, Some("Work & study")));
        let invalid_name = format!("line{}break", char::from(10));
        assert!(!data.update_bookmark_folder(None, Some(&invalid_name)));
        assert!(data.update_bookmark(
            "https://example.com/",
            "Edited",
            Some("Work & Study".into())
        ));
        let html = data.bookmarks_as_html();
        assert!(html.contains("<H3>Work &amp; Study</H3>"));
        let mut data = serde_json::from_value::<BrowserData>(serde_json::to_value(data).unwrap())
            .unwrap()
            .clean();
        assert!(data.update_bookmark_folder(Some("Work & Study"), Some("Personal")));
        assert_eq!(data.bookmarks[0].folder.as_deref(), Some("Personal"));
        assert!(data.update_bookmark_folder(Some("Personal"), None));
        assert_eq!(data.bookmarks[0].folder, None);
        assert_eq!(data.bookmarks[0].title, "Edited");
    }

    #[test]
    fn old_download_history_is_actionable_without_duplicate_ids() {
        let data: BrowserData = serde_json::from_str(r#"{"downloads":[
            {"id":1,"url":"https://example.com/a","file":"a","status":"Готово","started_at":1},
            {"id":1,"url":"https://example.com/b","file":"b","status":"Приостановлено","started_at":2}
        ]}"#).unwrap();
        let data = data.clean();
        assert_eq!(data.downloads[0].id, 1);
        assert_eq!(data.downloads[0].state(), DownloadStatus::Completed);
        assert_eq!(data.downloads[1].id, 2);
        assert_eq!(data.downloads[1].state(), DownloadStatus::Interrupted);
    }

    #[test]
    fn bookmark_export_escapes_html_and_omits_unsafe_urls() {
        let mut data = BrowserData::default();
        data.bookmarks.push(Bookmark {
            url: "https://example.org/?q=\"<x>".into(),
            title: "A <tag> & B".into(),
            added_at: 123,
            folder: None,
        });
        data.bookmarks.push(Bookmark {
            url: "javascript:alert(1)".into(),
            title: "Bad".into(),
            added_at: 0,
            folder: None,
        });
        let html = data.bookmarks_as_html();
        assert!(html.contains("&lt;tag&gt; &amp; B"));
        assert!(html.contains("?q=&quot;&lt;x&gt;"));
        assert!(html.contains("ADD_DATE=\"123\""));
        assert!(!html.contains("javascript:"));
    }
    #[cfg(windows)]
    #[test]
    fn startup_seals_legacy_state_once_and_keeps_a_current_snapshot() {
        let root =
            std::env::temp_dir().join(format!("literust-startup-{}-{}", std::process::id(), now()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("browser-data.json");
        fs::write(
            &path,
            br#"{"tabs":[{"id":7,"url":"https://example.org/","title":"Fixture"}]}"#,
        )
        .unwrap();
        let migrated = BrowserData::load_for_startup(&path, false).unwrap();
        assert_eq!(migrated.tabs[0].id, 7);
        let sealed = fs::read(&path).unwrap();
        assert!(crate::protected_data::is_current_format(&sealed));
        let current = BrowserData::load_for_startup(&path, false).unwrap();
        assert_eq!(current.tabs[0].url, "https://example.org/");
        assert_eq!(fs::read(&path).unwrap(), sealed);
        fs::remove_file(path.with_extension("json.bak")).unwrap();
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn browser_state_recovers_from_corrupted_json() {
        let tmpdir = std::env::temp_dir().join(format!(
            "literust-v08-test-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&tmpdir).unwrap();
        let primary = tmpdir.join("browser-data.json");
        let mut data = BrowserData::default();
        data.toggle_bookmark("https://example.org/", "ok");
        data.save(&primary).unwrap();
        data.save(&primary).unwrap(); // creates last-good backup
        fs::write(&primary, b"{broken").unwrap();
        let recovered = BrowserData::load(&primary).unwrap();
        assert_eq!(recovered.bookmarks.len(), 1);
        let _ = fs::remove_dir_all(&tmpdir);
    }
    #[test]
    fn interrupted_download_is_marked_on_recovery() {
        let mut data = BrowserData::default();
        data.record_download_start(1, "https://example.org/test.zip".into(), "test.zip".into());
        assert_eq!(data.clean().downloads[0].status, "Прервано");
    }
    #[test]
    fn escape_protects_local_pages() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
    #[test]
    fn bookmark_toggle_does_not_store_untrusted_scheme() {
        let mut d = BrowserData::default();
        d.toggle_bookmark("file:///etc/passwd", "x");
        assert_eq!(d.bookmarks.len(), 0);
        d.toggle_bookmark("https://example.com/", "Example");
        assert!(d.bookmarked("https://example.com/"));
        d.toggle_bookmark("https://example.com/", "Example");
        assert!(!d.bookmarked("https://example.com/"));
    }
    #[test]
    fn tab_navigation_isolated_and_persistent() {
        let mut a = Tab::new();
        let mut b = Tab::web("https://other.example/");
        a.navigate("https://example.org/one");
        a.navigate("https://example.org/two");
        assert!(a.go_back());
        assert_eq!(a.url, "https://example.org/one");
        assert!(a.go_forward());
        assert_eq!(a.url, "https://example.org/two");
        assert!(!b.go_back());
        let serialized = serde_json::to_string(&a).unwrap();
        let restored: Tab = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.back, a.back);
    }
    #[test]
    fn upgrades_old_tabs_with_distinct_ids() {
        let mut data = BrowserData {
            tabs: vec![Tab::new(), Tab::web("https://example.org/")],
            ..BrowserData::default()
        };
        data.tabs[1].id = data.tabs[0].id;
        let restored = data.clean();
        assert_ne!(restored.tabs[0].id, restored.tabs[1].id);
        assert_ne!(restored.tabs[0].id, 0);
    }
    #[test]
    fn old_tab_json_is_compatible() {
        let old = r#"{"url":"https://example.org/","title":"Old"}"#;
        let tab: Tab = serde_json::from_str(old).unwrap();
        assert!(tab.back.is_empty() && tab.forward.is_empty());
    }
    #[test]
    fn adblock_exceptions_match_exact_hosts_and_ignore_paths() {
        let mut d = BrowserData::default();
        assert_eq!(
            d.toggle_adblock_exception("https://example.org/some/path?x=1"),
            Some(true)
        );
        assert!(d.adblock_exception_for("https://example.org/another"));
        assert!(!d.adblock_exception_for("https://sub.example.org/"));
        assert!(!d.adblock_exception_for("https://notexample.org/"));
        assert_eq!(
            d.toggle_adblock_exception("https://example.org/"),
            Some(false)
        );
        assert!(!d.adblock_exception_for("https://example.org/"));
        assert_eq!(d.toggle_adblock_exception("literust://newtab"), None);
        assert_eq!(
            d.toggle_adblock_exception("https://google.com@evil.example/"),
            None
        );
    }
    #[test]
    fn old_settings_and_host_validation() {
        let mut d: BrowserData = serde_json::from_str(r#"{"network_blocking":true}"#).unwrap();
        assert!(d.adblock_exceptions.is_empty());
        d.adblock_exceptions = vec![
            "Example.org".into(),
            "example.org".into(),
            "google.com@evil.example".into(),
            "example.org/path".into(),
            "*.other.org".into(),
        ];
        let clean = d.clean();
        assert_eq!(clean.adblock_exceptions, vec!["example.org"]);
    }
    #[test]
    fn live_tab_budget_defaults_and_cycles() {
        let mut d: BrowserData = serde_json::from_str(r#"{"network_blocking":false}"#).unwrap();
        assert_eq!(d.live_tab_budget, 3);
        assert_eq!(d.cycle_live_tab_budget(), 5);
        assert_eq!(d.cycle_live_tab_budget(), 1);
        assert_eq!(d.cycle_live_tab_budget(), 3);
        d.live_tab_budget = 255;
        assert_eq!(d.clean().live_tab_budget, 3);
    }
    #[test]
    fn persisted_data_roundtrip() {
        let tmp =
            std::env::temp_dir().join(format!("literust-{}-{}.json", std::process::id(), now()));
        let mut d = BrowserData::default();
        d.toggle_bookmark("https://example.com/", "Example");
        d.save(&tmp).unwrap();
        let read = BrowserData::load(&tmp).unwrap();
        assert!(read.bookmarked("https://example.com/"));
        let _ = fs::remove_file(tmp);
    }
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    fn migrates_and_validates_appearance() {
        let old: BrowserData = serde_json::from_str("{}").unwrap();
        assert_eq!(old.appearance, Appearance::default());
        let invalid: BrowserData =
            serde_json::from_str(r#"{"appearance":{"design":30,"theme":330}}"#).unwrap();
        assert_eq!(invalid.clean().appearance, Appearance::default());
        for design in 0..30 {
            for theme in 0..330 {
                let data = BrowserData {
                    profile: std::rc::Rc::new(ProfileData {
                        appearance: Appearance { design, theme },
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let recovered: BrowserData =
                    serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
                assert_eq!(recovered.clean().appearance, data.appearance);
            }
        }
    }
}

#[cfg(test)]
mod expanded_settings_tests {
    use super::*;
    #[test]
    fn startup_preserves_restart_and_accepts_only_safe_home() {
        let mut data = BrowserData {
            tabs: vec![Tab::web("https://old.example")],
            ..BrowserData::default()
        };
        data.browser_settings.home_url = "https://home.example".into();
        data.browser_settings.startup = Startup::Home;
        data.apply_startup(true);
        assert_eq!(data.tabs[0].url, "https://old.example");
        data.apply_startup(false);
        assert_eq!(data.tabs[0].url, "https://home.example");
        for url in [
            "javascript:alert(1)",
            "file:///C:/secret",
            "https://user:password@example.org",
        ] {
            data.browser_settings.home_url = url.into();
            assert!(!data.browser_settings.valid_home());
            assert_eq!(data.browser_settings.clone().validated().home(), NEW_TAB);
        }
    }
    #[test]
    fn disabling_history_keeps_existing_records_without_recording_new_ones() {
        let mut data = BrowserData::default();
        data.record_visit("https://old.example", "old");
        data.browser_settings.history_enabled = false;
        data.record_visit("https://new.example", "new");
        data.update_history_title("https://old.example", "changed while disabled");
        assert_eq!(data.history.len(), 1);
        assert_eq!(data.history[0].title, "old");
    }
    #[test]
    fn settings_roundtrip_and_untrusted_values_are_bounded() {
        let mut data = BrowserData::default();
        data.browser_settings.zoom_percent = 125;
        data.browser_settings.background_links = true;
        data.customization.accent = Some(0x22aaff);
        let copy: BrowserData =
            serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(copy.customization.accent, Some(0x22aaff));
        assert_eq!(copy.browser_settings.zoom_percent, 125);
        assert!(copy.browser_settings.background_links);
        let root = std::env::temp_dir().join(format!("literust-settings-{}", std::process::id()));
        let file = root.join("browser-data.json");
        data.browser_settings.home_url = "https://home.example/".into();
        data.browser_settings.startup = Startup::Home;
        data.save(&file).unwrap();
        let loaded = BrowserData::load(&file).unwrap();
        assert_eq!(loaded.customization.accent, Some(0x22aaff));
        assert_eq!(loaded.browser_settings.zoom_percent, 125);
        assert_eq!(loaded.browser_settings.startup, Startup::Home);
        assert_eq!(loaded.browser_settings.home_url, "https://home.example/");
        assert!(loaded.browser_settings.background_links);
        fs::remove_dir_all(root).unwrap();
        data.customization.accent = Some(u32::MAX);
        data.customization.wallpaper_blur = u8::MAX;
        data.browser_settings.zoom_percent = u16::MAX;
        let clean = data.clean();
        assert_eq!(clean.customization.accent, None);
        assert_eq!(clean.customization.wallpaper_blur, 20);
        assert_eq!(clean.browser_settings.zoom_percent, 200);
    }
}

#[cfg(test)]
mod new_preferences_tests {
    use super::*;
    #[test]
    fn new_preferences_roundtrip_and_bounds() {
        let mut data = BrowserData {
            profile: std::rc::Rc::new(ProfileData {
                account_prompt_seen: true,
                account_prompt_always: true,
                notes: "я".repeat(20_001),
                background_style: BackgroundStyle {
                    mode: 255,
                    color_a: u32::MAX,
                    color_b: 0,
                    angle: u16::MAX,
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        data = data.clean();
        assert_eq!(data.notebook.entries()[0].body.chars().count(), 20_000);
        assert_eq!(
            data.background_style.css(),
            "background:radial-gradient(ellipse at top left,#ffffff,#000000)!important"
        );
        assert_eq!(data.background_style.angle, 360);
        let copy: BrowserData =
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        assert!(copy.account_prompt_seen && copy.account_prompt_always);
        assert_eq!(copy.notebook, data.notebook);
    }
}
