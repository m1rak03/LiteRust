use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DockPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
pub fn arrange(width: f64, height: f64, toolbar: f64, position: DockPosition) -> (Bounds, Bounds) {
    let width = width.max(1.0);
    let space = (height - toolbar).max(1.0);
    let thickness = 54.0_f64.min(space / 2.0).min(width / 2.0);
    let mut page = Bounds {
        x: 0.0,
        y: toolbar,
        width,
        height: space,
    };
    let dock = match position {
        DockPosition::Left => {
            page.x = thickness;
            page.width -= thickness;
            Bounds {
                x: 0.0,
                y: toolbar,
                width: thickness,
                height: space,
            }
        }
        DockPosition::Right => {
            page.width -= thickness;
            Bounds {
                x: page.width,
                y: toolbar,
                width: thickness,
                height: space,
            }
        }
        DockPosition::Top => {
            page.y += thickness;
            page.height -= thickness;
            Bounds {
                x: 0.0,
                y: toolbar,
                width,
                height: thickness,
            }
        }
        DockPosition::Bottom => {
            page.height -= thickness;
            Bounds {
                x: 0.0,
                y: toolbar + page.height,
                width,
                height: thickness,
            }
        }
    };
    (dock, page)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn docks_never_overlap_content_or_leave_window() {
        for pos in [
            DockPosition::Left,
            DockPosition::Right,
            DockPosition::Top,
            DockPosition::Bottom,
        ] {
            for (w, h, t) in [(1280.0, 800.0, 108.0), (690.0, 400.0, 320.0)] {
                let (d, p) = arrange(w, h, t, pos);
                assert!(p.width > 0.0 && p.height > 0.0);
                assert!(d.x + d.width <= w && p.x + p.width <= w);
                assert!(d.y + d.height <= h && p.y + p.height <= h);
                assert!(
                    d.x + d.width <= p.x
                        || p.x + p.width <= d.x
                        || d.y + d.height <= p.y
                        || p.y + p.height <= d.y
                );
            }
        }
    }
}

pub const SIDEBAR_ACTIONS: &[&str] = &[
    "new_tab",
    "show_bookmarks",
    "show_history",
    "show_downloads",
    "show_accounts",
    "show_security",
    "show_vpn",
    "show_tools",
    "duplicate_tab",
    "print_page",
    "new_private_window",
    "show_appearance",
    "show_settings",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Sidebar {
    pub mode: String,
    pub width: u16,
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}
impl Default for Sidebar {
    fn default() -> Self {
        Self {
            mode: "click".into(),
            width: 264,
            order: SIDEBAR_ACTIONS.iter().map(|s| s.to_string()).collect(),
            hidden: vec![],
        }
    }
}
impl Sidebar {
    pub fn validated(mut self) -> Self {
        if !["click", "hover", "pinned"].contains(&self.mode.as_str()) {
            self.mode = "click".into();
        }
        self.width = self.width.clamp(220, 320);
        let mut seen = std::collections::BTreeSet::new();
        self.order
            .retain(|s| SIDEBAR_ACTIONS.contains(&s.as_str()) && seen.insert(s.clone()));
        for action in SIDEBAR_ACTIONS {
            if !seen.contains(*action) {
                self.order.push(action.to_string());
            }
        }
        self.hidden
            .retain(|s| SIDEBAR_ACTIONS.contains(&s.as_str()));
        self.hidden.sort();
        self.hidden.dedup();
        self
    }
}
pub fn arrange_sidebar(
    width: f64,
    height: f64,
    toolbar: f64,
    position: DockPosition,
    sidebar: &Sidebar,
    expanded: bool,
) -> (Bounds, Bounds) {
    let (mut dock, mut page) = arrange(width, height, toolbar, position);
    if matches!(position, DockPosition::Left | DockPosition::Right)
        && (expanded || sidebar.mode == "pinned")
    {
        let extent = f64::from(sidebar.width.clamp(220, 320)).min((width - 120.0).max(54.0));
        dock.width = extent;
        if matches!(position, DockPosition::Right) {
            dock.x = (width - extent).max(0.0);
        }
        if sidebar.mode == "pinned" {
            page.width = (width - extent).max(1.0);
            if matches!(position, DockPosition::Left) {
                page.x = extent;
            }
        }
    }
    (dock, page)
}
/// A hover preview overlays the page without changing its viewport.
pub fn peek_sidebar(mut dock: Bounds, width: f64) -> Bounds {
    dock.width = dock.width.max(176.0_f64.min(width - 120.0));
    dock
}

#[cfg(test)]
mod sidebar_tests {
    use super::*;
    #[test]
    fn overlay_does_not_resize_page() {
        let s = Sidebar::default();
        let (_, p) = arrange_sidebar(1280., 800., 108., DockPosition::Left, &s, false);
        let (d, q) = arrange_sidebar(1280., 800., 108., DockPosition::Left, &s, true);
        assert_eq!(p.width, q.width);
        assert_eq!(p.x, q.x);
        assert_eq!(d.width, 264.);
        let pinned = Sidebar {
            mode: "pinned".into(),
            ..s
        };
        let (_, r) = arrange_sidebar(1280., 800., 108., DockPosition::Left, &pinned, true);
        assert_eq!(r.x, 264.);
        assert_eq!(r.width, 1016.);
    }
    #[test]
    fn sanitize_sidebar() {
        let s = Sidebar {
            mode: "bad".into(),
            width: 65535,
            order: vec!["show_vpn".into(), "evil".into(), "show_vpn".into()],
            hidden: vec!["evil".into()],
        }
        .validated();
        assert_eq!(s.mode, "click");
        assert_eq!(s.width, 320);
        assert_eq!(s.order.len(), SIDEBAR_ACTIONS.len());
        assert_eq!(s.order[0], "show_vpn");
        assert!(s.hidden.is_empty());
    }
}
