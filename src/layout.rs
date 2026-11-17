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
