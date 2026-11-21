use super::{Browser, DIAGNOSTICS};
use literust_browser::diagnostics;
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    browser_version: &'static str,
    rust_build: &'static str,
    runtime: Option<String>,
    private_window: bool,
    workspace_tabs: usize,
    live_views: usize,
    active_profiles: usize,
    vpn: &'static str,
    journal: &'static str,
    events: Vec<diagnostics::Entry>,
}

impl Browser {
    pub(super) fn show_diagnostics(&mut self, preview: bool) {
        if self.panel.as_deref() != Some("settings") {
            return;
        }
        let mut profiles = Vec::new();
        #[cfg(windows)]
        let root = super::app_root();
        #[cfg(windows)]
        let catalog = literust_browser::local_profiles::Catalog::load(&root).ok();
        #[cfg(windows)]
        for profile in &catalog.as_ref().unwrap_or(&self.local_profiles).profiles {
            if literust_browser::local_profiles::root(&root, &profile.id)
                .is_ok_and(|path| super::windows_platform::profile_is_open(&path))
            {
                profiles.push(profile.name.as_str());
            }
        }
        #[cfg(not(windows))]
        if !self.private_mode {
            profiles.push(super::current_profile_id());
        }
        let runtime = wry::webview_version().ok().and_then(|value| {
            let version = value.split_ascii_whitespace().next()?;
            (version.len() <= 64
                && version
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.'))
            .then(|| version.to_owned())
        });
        let report = Report {
            browser_version: env!("CARGO_PKG_VERSION"),
            rust_build: env!("LITERUST_BUILD_RUST"),
            runtime,
            private_window: self.private_mode,
            workspace_tabs: self.data.tabs.len(),
            live_views: self.pages.len(),
            active_profiles: profiles.len(),
            vpn: if self.system_vpn_running() {
                "system"
            } else if self.vpn_running {
                "browser"
            } else if self.vpn_vault.enabled {
                "unavailable"
            } else {
                "off"
            },
            journal: if DIAGNOSTICS
                .get()
                .is_some_and(diagnostics::Journal::has_local_writer)
            {
                "local"
            } else {
                "session"
            },
            events: DIAGNOSTICS
                .get()
                .map_or_else(Vec::new, diagnostics::Journal::recent),
        };
        if let Ok(report) = serde_json::to_string_pretty(&report) {
            let payload = literust_browser::home::script_json(&(&report, profiles, preview));
            let _ = self
                .toolbar
                .evaluate_script(&format!("window.diagnosticsUpdate?.({payload});"));
            self.diagnostic_preview = preview.then_some(report);
        }
    }

    pub(super) fn export_diagnostics(&self) {
        if self.diagnostic_preview.is_none() || self.panel.as_deref() != Some("settings") {
            return;
        }
        #[cfg(windows)]
        match super::windows_platform::choose_diagnostics(&self.toolbar) {
            Ok(Some(path)) => {
                if self.write_diagnostics(&path).is_ok() {
                    self.notice_localized("Диагностический отчёт сохранён.");
                } else {
                    self.notice_localized("Не удалось записать диагностический отчёт.");
                }
            }
            Ok(None) => {}
            Err(_) => self.notice_localized("Не удалось открыть диалог сохранения отчёта."),
        }
    }

    #[cfg(windows)]
    pub(super) fn write_diagnostics(&self, path: &std::path::Path) -> std::io::Result<()> {
        let report = self
            .diagnostic_preview
            .as_ref()
            .filter(|_| self.panel.as_deref() == Some("settings"))
            .ok_or_else(|| std::io::Error::other("Diagnostic preview required"))?;
        literust_browser::profile::atomic_write(path, report.as_bytes())
    }
}
