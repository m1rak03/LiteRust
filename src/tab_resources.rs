use serde::{Deserialize, Serialize};

/// Manual is also the fallback for profiles that have never opted into auto release.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Automatic,
    Saver,
    Performance,
    #[default]
    #[serde(other)]
    Manual,
}

impl Mode {
    pub fn for_profile(mode: Option<Self>, auto_discard: bool) -> Self {
        mode.unwrap_or(if auto_discard {
            Self::Automatic
        } else {
            Self::Manual
        })
    }

    /// These are targets. Active and protected tabs may keep more views alive.
    pub fn target(self, legacy_budget: u8, visible: usize) -> Option<usize> {
        match self {
            Self::Automatic => Some(usize::from(legacy_budget).max(visible)),
            Self::Saver => Some(visible.max(2)),
            Self::Manual | Self::Performance => None,
        }
    }
}

/// Script results contain only coordinates. Unknown, incomplete and modified
/// documents do not return a coordinate pair.
pub fn release_position(json: &str) -> Option<[i32; 2]> {
    if json.len() > 48 {
        return None;
    }
    serde_json::from_str(json).ok()
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TabCounts {
    pub total: usize,
    pub visible: usize,
    pub background: usize,
    pub live: usize,
    pub pinned: usize,
    pub idle: usize,
    pub playing_audio: usize,
    pub downloading: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_budget_and_explicit_modes_coexist() {
        assert_eq!(Mode::for_profile(None, false), Mode::Manual);
        assert_eq!(Mode::for_profile(None, true).target(5, 1), Some(5));
        assert_eq!(Mode::for_profile(None, true).target(1, 2), Some(2));
        assert_eq!(
            Mode::for_profile(Some(Mode::Performance), true).target(1, 1),
            None
        );
        assert_eq!(Mode::Saver.target(5, 1), Some(2));
        assert_eq!(Mode::Manual.target(1, 1), None);
    }

    #[test]
    fn only_bounded_numeric_positions_can_authorize_release() {
        assert_eq!(release_position("[0,240]"), Some([0, 240]));
        assert_eq!(release_position("null"), None);
        assert_eq!(release_position("[0,2147483648]"), None);
        assert_eq!(release_position(r#"{"x":0,"y":240}"#), None);
        assert_eq!(release_position(&" ".repeat(50)), None);
    }
}
