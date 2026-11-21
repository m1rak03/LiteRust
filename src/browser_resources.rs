use super::{AppEvent, Browser, LivePage, Tab};
use literust_browser::{
    tab_groups::TabGroups,
    tab_resources::{self, BackgroundPolicy, Mode, TabCounts},
};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
#[cfg(windows)]
use wry::WebViewExtWindows;

#[derive(Default)]
pub(super) struct Controller {
    checked: HashSet<u64>,
    pending: Option<Probe>,
    #[cfg(windows)]
    background_due: Option<Instant>,
    next_probe: u64,
    manual_released: Option<usize>,
    #[cfg(windows)]
    sampling: bool,
}

#[derive(Clone, Copy)]
enum ProbeKind {
    Release,
    #[cfg(windows)]
    MemoryHint,
}

struct Probe {
    kind: ProbeKind,
    request: u64,
    tab: u64,
    generation: u64,
    navigation: u64,
    deadline: Instant,
}

impl Browser {
    pub(super) fn resource_mode(&self) -> Mode {
        Mode::for_profile(self.data.resource_mode, self.data.auto_discard)
    }

    pub(super) fn set_resource_mode(&mut self, mode: Mode) {
        self.data.resource_mode = Some(mode);
        self.data.auto_discard = matches!(mode, Mode::Automatic | Mode::Saver);
        self.refresh_release_candidates();
        self.enforce_page_budget();
        self.update_background_hints(Instant::now());
        self.persist();
        self.sync_toolbar();
    }

    pub(super) fn refresh_release_candidates(&mut self) {
        self.resources.checked.clear();
    }

    fn target(&self) -> Option<usize> {
        self.resource_mode().target(
            self.data.live_tab_budget,
            if self.data.split.is_some() { 2 } else { 1 },
        )
    }

    fn needs_release(&self) -> bool {
        self.resources.manual_released.is_some()
            || self
                .target()
                .is_some_and(|target| self.pages.len() > target)
    }

    fn tabs_for_page(&self, page: &LivePage) -> Option<(&[Tab], &TabGroups)> {
        if page.workspace == self.workspace_id() {
            return Some((&self.data.tabs, &self.data.tab_groups));
        }
        let session = if page.workspace == 0 {
            self.data.workspace.ordinary.as_ref()
        } else {
            self.data.workspaces.get(page.workspace)?.session.as_deref()
        }?;
        Some((&session.tabs, &session.tab_groups))
    }

    fn pinned_and_external(&self, id: u64, page: &LivePage) -> Option<(bool, bool)> {
        let (tabs, groups) = self.tabs_for_page(page)?;
        let tab = tabs.iter().find(|tab| tab.id == id)?;
        Some((
            groups
                .entries()
                .iter()
                .any(|group| group.pinned && tab.group == Some(group.id)),
            super::valid_web_url(&tab.url),
        ))
    }

    fn eligible(&self, id: u64, page: &LivePage) -> Option<bool> {
        if self.is_visible_tab(id) || page.foreground || page.busy() || page.loading {
            return None;
        }
        let (pinned, external) = self.pinned_and_external(id, page)?;
        if pinned {
            return None;
        }
        #[cfg(windows)]
        if external && super::webview_resources::playing_audio(&page.view) != Some(false) {
            return None;
        }
        Some(external)
    }

    fn finish_manual_release(&mut self) {
        if let Some(count) = self.resources.manual_released.take() {
            self.notice(&format!(
                "{} {count}",
                self.ui_language.pick("Выгружено вкладок", "Tabs released")
            ));
        }
    }

    fn release_page(&mut self, id: u64, position: Option<[i32; 2]>) {
        if let Some(tab) = self.tab_for_page_mut(id) {
            tab.scroll_position = position;
        }
        self.pages.remove(&id);
        super::note_diagnostic(literust_browser::diagnostics::Event::TabReleased);
        if let Some(count) = &mut self.resources.manual_released {
            *count += 1;
        }
    }

    pub(super) fn start_manual_release(&mut self) {
        self.resources.manual_released.get_or_insert(0);
        self.refresh_release_candidates();
        self.enforce_page_budget();
        self.sync_toolbar();
    }

    /// Native internal pages can be released immediately. Site documents are
    /// assessed one at a time on their renderer and can change while we wait.
    pub(super) fn enforce_page_budget(&mut self) -> bool {
        let before = self.pages.len();
        while self.needs_release() && self.resources.pending.is_none() {
            let candidate = self
                .pages
                .iter()
                .filter(|(id, _)| !self.resources.checked.contains(id))
                .filter_map(|(&id, page)| {
                    self.eligible(id, page).map(|external| (id, page, external))
                })
                .min_by_key(|(id, page, _)| (page.last_used, *id));
            let Some((id, _page, external)) = candidate else {
                self.finish_manual_release();
                break;
            };
            if !external {
                self.release_page(id, None);
                continue;
            }
            self.resources.checked.insert(id);
            self.assess_document(id, ProbeKind::Release);
        }
        self.pages.len() != before
    }

    fn assess_document(&mut self, id: u64, kind: ProbeKind) {
        let Some(page) = self.pages.get(&id) else {
            return;
        };
        self.resources.next_probe = self.resources.next_probe.wrapping_add(1);
        let request = self.resources.next_probe;
        let pending = Probe {
            kind,
            request,
            tab: id,
            generation: page.generation,
            navigation: page.navigation_serial,
            deadline: Instant::now() + Duration::from_secs(2),
        };
        let proxy = self.proxy.clone();
        if page.view.evaluate_script_with_callback(
        "typeof window.__liteRustReleasePosition === 'function' ? window.__liteRustReleasePosition() : null",
        move |json| {
            let _ = proxy.send_event(AppEvent::ReleaseAssessed {
                request, position: tab_resources::release_position(&json),
            });
        },
    ).is_ok() {
        self.resources.pending = Some(pending);
    }
    }

    pub(super) fn apply_release_result(&mut self, request: u64, position: Option<[i32; 2]>) {
        let Some(pending) = self
            .resources
            .pending
            .take_if(|probe| probe.request == request)
        else {
            return;
        };
        let safe = position.is_some()
            && self.pages.get(&pending.tab).is_some_and(|page| {
                page.generation == pending.generation
                    && page.navigation_serial == pending.navigation
                    && self.eligible(pending.tab, page) == Some(true)
            });
        let mut released = false;
        match pending.kind {
            ProbeKind::Release if safe && self.needs_release() => {
                self.release_page(pending.tab, position);
                self.persist();
                released = true;
            }
            ProbeKind::Release => {}
            #[cfg(windows)]
            ProbeKind::MemoryHint if safe => {
                if let Some(page) = self.pages.get_mut(&pending.tab) {
                    page.idle_hint_approved = Some(true);
                }
            }
            #[cfg(windows)]
            ProbeKind::MemoryHint => {}
        }
        if self.enforce_page_budget() || released {
            self.sync_toolbar();
        }
        self.update_background_hints(Instant::now());
    }

    pub(super) fn set_background_policy(&mut self, policy: BackgroundPolicy) {
        self.data.background_policy = policy;
        self.background_policy_changed();
        self.persist();
        self.sync_toolbar();
    }

    pub(super) fn background_policy_changed(&mut self) {
        #[cfg(windows)]
        for page in self.pages.values_mut() {
            page.idle_hint_approved = None;
        }
        self.update_background_hints(Instant::now());
    }

    // WebView2's memory hint keeps scripts and network connections running. Control
    // expensive assessment here instead of driving it from a repeating UI timer.
    pub(super) fn update_background_hints(&mut self, now: Instant) {
        #[cfg(windows)]
        {
            let policy = self.data.background_policy.for_mode(self.resource_mode());
            let quiet_time = Duration::from_secs(60);
            let mut deadline = None;
            let mut candidate = None;
            let mut changes = Vec::new();
            for (&id, page) in &self.pages {
                let mut use_low = false;
                if policy != BackgroundPolicy::Off
                    && !page.foreground
                    && !page.loading
                    && !page.busy()
                {
                    match self.pinned_and_external(id, page) {
                        Some((false, false)) => use_low = true,
                        Some((false, true))
                            if policy == BackgroundPolicy::IdleSites
                                && self.eligible(id, page) == Some(true) =>
                        {
                            if let Some(approved) = page.idle_hint_approved {
                                use_low = approved;
                            } else if self.resources.pending.is_none() {
                                let due = page.last_visible + quiet_time;
                                if due <= now {
                                    let rank = (page.last_used, id);
                                    if candidate.is_none_or(|(current, _)| rank < current) {
                                        candidate = Some((rank, id));
                                    }
                                } else {
                                    deadline = Some(
                                        deadline.map_or(due, |earliest: Instant| earliest.min(due)),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if use_low != page.memory_hint_low {
                    changes.push((id, use_low));
                }
            }
            for (id, low) in changes {
                if let Some(page) = self.pages.get_mut(&id) {
                    page.memory_hint_low = low;
                    let hint = if low {
                        wry::MemoryUsageLevel::Low
                    } else {
                        wry::MemoryUsageLevel::Normal
                    };
                    let _ = page.view.set_memory_usage_level(hint);
                }
            }
            if let Some((_, id)) = candidate {
                // Failed or inconclusive replies leave the standard policy in place.
                self.pages
                    .get_mut(&id)
                    .expect("selected live tab")
                    .idle_hint_approved = Some(false);
                self.assess_document(id, ProbeKind::MemoryHint);
                if self.resources.pending.is_none() {
                    deadline = Some(now + Duration::from_millis(50));
                }
            }
            self.resources.background_due = deadline;
        }
        #[cfg(not(windows))]
        let _ = now;
    }

    pub(super) fn release_deadline(&self) -> Option<Instant> {
        let pending = self.resources.pending.as_ref().map(|probe| probe.deadline);
        #[cfg(windows)]
        let pending = pending.or(self.resources.background_due);
        pending
    }

    pub(super) fn poll_release(&mut self, now: Instant) {
        let expired = self
            .resources
            .pending
            .as_ref()
            .is_some_and(|probe| probe.deadline <= now);
        if expired {
            self.resources.pending = None;
            if self.enforce_page_budget() {
                self.sync_toolbar();
            }
        }
        #[cfg(windows)]
        if expired
            || (self.resources.pending.is_none()
                && self
                    .resources
                    .background_due
                    .is_some_and(|deadline| deadline <= now))
        {
            self.update_background_hints(now);
        }
    }

    pub(super) fn tab_counts(&self) -> TabCounts {
        let visible = if self.data.split.is_some() { 2 } else { 1 };
        let total = self.data.tabs.len();
        let mut counts = TabCounts {
            total,
            visible,
            background: total.saturating_sub(visible),
            ..TabCounts::default()
        };
        for tab in &self.data.tabs {
            let page = self.pages.get(&tab.id);
            let pinned = self
                .data
                .tab_groups
                .entries()
                .iter()
                .any(|group| group.pinned && tab.group == Some(group.id));
            counts.pinned += usize::from(pinned);
            if let Some(page) = page {
                counts.live += 1;
                counts.downloading += usize::from(page.downloads.get() != 0);
                #[cfg(windows)]
                {
                    counts.playing_audio += usize::from(
                        super::webview_resources::playing_audio(&page.view) == Some(true),
                    );
                }
                counts.idle += usize::from(
                    !page.foreground && page.last_visible.elapsed() >= Duration::from_secs(90),
                );
            } else {
                counts.idle += usize::from(!self.is_visible_tab(tab.id));
            }
        }
        counts
    }

    pub(super) fn sample_resources(&mut self) {
        #[cfg(windows)]
        {
            if self.resources.sampling {
                return;
            }
            let inventory = super::webview_resources::inventory(
                [&self.toolbar, &self.dock]
                    .into_iter()
                    .chain(self.pages.values().map(|page| &page.view)),
            );
            let proxy = self.proxy.clone();
            self.resources.sampling = std::thread::Builder::new()
                .spawn(move || {
                    let sample = super::webview_resources::sample(
                        inventory,
                        Some(Duration::from_millis(450)),
                    );
                    let _ = proxy.send_event(AppEvent::ResourcesMeasured(sample));
                })
                .is_ok();
            if self.resources.sampling {
                return;
            }
        }
        self.display_resources(&());
    }

    pub(super) fn display_resources(&mut self, sample: &impl serde::Serialize) {
        #[cfg(windows)]
        {
            self.resources.sampling = false;
        }
        let payload = literust_browser::home::script_json(&(self.tab_counts(), sample));
        let _ = self
            .toolbar
            .evaluate_script(&format!("window.resourcesUpdate?.({payload});"));
    }
}
