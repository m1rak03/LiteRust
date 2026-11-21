//! Platform-independent browser metadata and policies.
pub mod google_passwords;
pub mod import;
pub mod layout;
pub mod navigation;
pub mod network_block;
pub mod newtab;
pub mod profile;
pub mod security;
pub mod storage;
pub mod vpn;
#[cfg(windows)]
pub mod vpn_platform;

pub mod icons;

#[cfg(windows)]
pub mod system_vpn;

pub mod protected_data;

pub mod accounts;

pub mod shortcuts;

pub mod local_profiles;

pub mod home;
pub mod i18n;

pub mod split_view;
pub mod tab_groups;
pub mod window_session;
