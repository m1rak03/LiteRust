//! Window-local selection and geometry for two existing tab WebViews.
use crate::{layout::Bounds, storage::Tab};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
    #[default]
    First,
    Second,
}

impl Pane {
    pub const BOTH: [Self; 2] = [Self::First, Self::Second];

    pub fn index(self) -> usize {
        usize::from(self == Self::Second)
    }

    pub fn other(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitView {
    pub panes: [u64; 2],
    #[serde(default)]
    pub axis: Axis,
    pub percent: u8,
    #[serde(default)]
    pub focus: Pane,
}

impl SplitView {
    pub fn new(first: u64, second: u64) -> Self {
        Self {
            panes: [first, second],
            axis: Axis::Horizontal,
            percent: 50,
            focus: Pane::First,
        }
    }

    pub fn validate(mut self, tabs: &[Tab], active: &mut usize) -> Option<Self> {
        if self.panes[0] == self.panes[1]
            || self
                .panes
                .iter()
                .any(|id| !tabs.iter().any(|tab| tab.id == *id))
        {
            return None;
        }
        self.resize(self.percent);
        *active = tabs.iter().position(|tab| tab.id == self.tab(self.focus))?;
        Some(self)
    }

    pub fn tab(self, pane: Pane) -> u64 {
        self.panes[pane.index()]
    }

    pub fn pane(self, tab: u64) -> Option<Pane> {
        Pane::BOTH.into_iter().find(|pane| self.tab(*pane) == tab)
    }

    pub fn select(&mut self, tab: u64) {
        if let Some(pane) = self.pane(tab) {
            self.focus = pane;
        } else {
            self.panes[self.focus.index()] = tab;
        }
    }

    pub fn swap(&mut self) {
        self.panes.swap(0, 1);
        self.focus = self.focus.other();
    }

    pub fn resize(&mut self, percent: u8) {
        self.percent = percent.clamp(25, 75);
    }

    pub fn bounds(self, bounds: Bounds, pane: Pane) -> Bounds {
        let length = match self.axis {
            Axis::Horizontal => bounds.width,
            Axis::Vertical => bounds.height,
        };
        let gap = (length / 12.0).min(5.0);
        let available = (length - gap).max(1.0);
        let first = available * f64::from(self.percent) / 100.0;
        let (start, size) = match pane {
            Pane::First => (0.0, first),
            Pane::Second => (first + gap, available - first),
        };
        match self.axis {
            Axis::Horizontal => Bounds {
                x: bounds.x + start,
                width: size,
                ..bounds
            },
            Axis::Vertical => Bounds {
                y: bounds.y + start,
                height: size,
                ..bounds
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Open { tab: u64 },
    Focus { pane: Pane },
    Choose { pane: Pane, tab: u64 },
    Navigate { pane: Pane, value: String },
    Axis { axis: Axis },
    Resize { percent: u8 },
    Swap,
    Close { pane: Pane },
    Exit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{layout::physical_axis, storage::BrowserData};

    #[test]
    fn split_keeps_tab_ids_on_restore_and_rejects_missing_panes() {
        let data: BrowserData = serde_json::from_str(
            r#"{
          "tabs":[{"id":89,"url":"https://one.example/","title":"One"},
                  {"id":64,"url":"https://two.example/","title":"Two"}],
          "split":{"panes":[89,64],"axis":"vertical","percent":90,"focus":"second"}
        }"#,
        )
        .unwrap();
        let mut data = data.clean();
        let split = data.split.unwrap();
        assert_eq!(split.percent, 75);
        assert_eq!(split.axis, Axis::Vertical);
        assert_eq!(data.tabs[data.active].id, 64);
        data.tabs.pop();
        assert!(data.clean().split.is_none());
    }

    #[test]
    fn physical_rectangles_share_the_available_width_or_height_at_fractional_dpi() {
        let area = Bounds {
            x: 46.0,
            y: 148.0,
            width: 849.5,
            height: 426.25,
        };
        let mut split = SplitView::new(1, 2);
        split.resize(33);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            split.axis = axis;
            let first = split.bounds(area, Pane::First);
            let second = split.bounds(area, Pane::Second);
            let (full, a, b) = match axis {
                Axis::Horizontal => (
                    (area.x, area.width),
                    (first.x, first.width),
                    (second.x, second.width),
                ),
                Axis::Vertical => (
                    (area.y, area.height),
                    (first.y, first.height),
                    (second.y, second.height),
                ),
            };
            for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
                let (full_start, full_size) = physical_axis(full.0, full.1, scale);
                let (first_start, first_size) = physical_axis(a.0, a.1, scale);
                let (second_start, second_size) = physical_axis(b.0, b.1, scale);
                assert_eq!(first_start, full_start);
                assert!(first_start + (first_size as i32) < second_start);
                assert_eq!(
                    second_start + second_size as i32,
                    full_start + full_size as i32
                );
            }
        }
        split.swap();
        assert_eq!(split.focus, Pane::Second);
        assert_eq!(split.tab(split.focus), 1);
    }
}
