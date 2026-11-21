//! Local profile catalogue. Legacy data stays in place; new profiles get isolated roots.
use crate::{
    profile::{self, unique_id},
    protected_data,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

const CATALOG_LIMIT: u64 = 2_000_000;
const RECORD_LIMIT: u64 = 110_000;
const PROFILE_RECORD: &str = "profile-entry.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub avatar: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub selected: String,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProfileDetails {
    pub bytes: BTreeMap<String, Option<u64>>,
    pub archived: Vec<ArchivedProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchivedProfile {
    pub id: String,
    pub name: String,
    pub bytes: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Primary,
    Backup,
    New,
}

fn lock_catalog(base: &Path) -> io::Result<fs::File> {
    fs::create_dir_all(base)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(base.join("profiles.lock"))?;
    lock.try_lock()
        .map_err(|_| io::Error::other("Каталог профилей занят. Повторите действие."))?;
    Ok(lock)
}

fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}

fn real_directory(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !linked(&meta) => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid profile directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    if !real_directory(path)? {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    protected_data::seal(&bytes)
}

fn read_record(path: &Path, id: &str) -> Option<Profile> {
    let bytes = profile::read_limited(&path.join(PROFILE_RECORD), RECORD_LIMIT).ok()?;
    let record: Profile = serde_json::from_slice(&protected_data::open(&bytes).ok()?).ok()?;
    (record.id == id && valid_profile(&record)).then_some(record)
}

fn valid_profile(profile: &Profile) -> bool {
    valid_id(&profile.id)
        && !profile.name.trim().is_empty()
        && profile.name.chars().count() <= 60
        && valid_avatar(&profile.avatar)
}

fn directory_bytes(path: &Path, exclude_other_profiles: bool) -> io::Result<u64> {
    if !real_directory(path)? {
        return Ok(0);
    }
    let mut pending = vec![fs::read_dir(path)?];
    let mut bytes = 0u64;
    while let Some(entries) = pending.last_mut() {
        let Some(entry) = entries.next() else {
            pending.pop();
            continue;
        };
        let entry = entry?;
        if exclude_other_profiles
            && pending.len() == 1
            && ["Profiles", "ProfileTrash"]
                .iter()
                .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if linked(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            pending.push(fs::read_dir(entry.path())?);
        } else if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(bytes)
}
impl Default for Catalog {
    fn default() -> Self {
        Self {
            selected: "main".into(),
            profiles: vec![Profile {
                id: "main".into(),
                name: "Основной профиль".into(),
                avatar: String::new(),
            }],
        }
    }
}
pub fn valid_id(id: &str) -> bool {
    id == "main"
        || (id.len() == 32
            && id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
}
pub fn root(base: &Path, id: &str) -> io::Result<PathBuf> {
    if !valid_id(id) {
        return Err(io::Error::other("Некорректный профиль"));
    }
    Ok(if id == "main" {
        base.to_path_buf()
    } else {
        base.join("Profiles").join(id)
    })
}
fn valid_avatar(value: &str) -> bool {
    value.is_empty() || (value.len() <= 100_000 && crate::icons::png_bytes(value).is_some())
}
impl Catalog {
    pub fn for_ui(&self, current: &str) -> Self {
        Self {
            selected: self.selected.clone(),
            profiles: self
                .profiles
                .iter()
                .map(|p| Profile {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    avatar: if p.id == current {
                        p.avatar.clone()
                    } else {
                        String::new()
                    },
                })
                .collect(),
        }
    }

    fn read_file(path: &Path) -> io::Result<Self> {
        let bytes = profile::read_limited(path, CATALOG_LIMIT)?;
        let value: Self = serde_json::from_slice(&protected_data::open(&bytes)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> io::Result<()> {
        let mut ids = BTreeSet::new();
        if self.profiles.len() > 16
            || !self
                .profiles
                .iter()
                .all(|p| valid_profile(p) && ids.insert(&p.id))
            || !self.profiles.iter().any(|p| p.id == self.selected)
            || !self.profiles.iter().any(|p| p.id == "main")
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Каталог профилей повреждён; исходный файл сохранён",
            ));
        }
        Ok(())
    }

    fn read_snapshot(base: &Path) -> io::Result<(Self, Source)> {
        let (mut catalog, source) = match Self::read_file(&base.join("profiles.json")) {
            Ok(data) => (data, Source::Primary),
            Err(primary) => match Self::read_file(&base.join("profiles.json.bak")) {
                Ok(data) => (data, Source::Backup),
                Err(backup)
                    if primary.kind() == io::ErrorKind::NotFound
                        && backup.kind() == io::ErrorKind::NotFound =>
                {
                    (Self::default(), Source::New)
                }
                Err(_) => return Err(primary),
            },
        };
        let trash = base.join("ProfileTrash");
        if real_directory(&trash)? {
            // An archive can be moved just before a crash or before the backup is written.
            catalog
                .profiles
                .retain(|p| p.id == "main" || !real_directory(&trash.join(&p.id)).unwrap_or(false));
        }
        if !catalog.profiles.iter().any(|p| p.id == catalog.selected) {
            catalog.selected = "main".into();
        }
        // Profile records survive a crash between moving a restored profile and saving the catalogue.
        let active = base.join("Profiles");
        if real_directory(&active)? {
            for entry in fs::read_dir(active)?.filter_map(Result::ok) {
                if catalog.profiles.len() == 16 {
                    break;
                }
                let Ok(id) = entry.file_name().into_string() else {
                    continue;
                };
                if id != "main"
                    && valid_id(&id)
                    && !catalog.profiles.iter().any(|p| p.id == id)
                    && real_directory(&entry.path()).unwrap_or(false)
                    && !real_directory(&trash.join(&id)).unwrap_or(false)
                {
                    if let Some(record) = read_record(&entry.path(), &id) {
                        catalog.profiles.push(record);
                    }
                }
            }
        }
        Ok((catalog, source))
    }

    /// Read-only lookup, also used by private windows.
    pub fn load(base: &Path) -> io::Result<Self> {
        Ok(Self::read_snapshot(base)?.0)
    }

    pub fn recover(base: &Path) -> io::Result<Self> {
        let (catalog, source) = Self::read_snapshot(base)?;
        if source != Source::Backup {
            return Ok(catalog);
        }
        let _lock = lock_catalog(base)?;
        let (catalog, source) = Self::read_snapshot(base)?;
        if source == Source::Backup {
            Self::repair(base, &catalog)?;
        }
        Ok(catalog)
    }

    fn repair(base: &Path, catalog: &Self) -> io::Result<()> {
        let primary = base.join("profiles.json");
        let encoded = encode(catalog)?;
        profile::preserve_damaged(&primary)?;
        profile::atomic_write(&primary, &encoded)
    }

    fn save(base: &Path, previous: &Self, source: Source, next: &Self) -> io::Result<()> {
        next.validate()?;
        if source == Source::Backup {
            Self::repair(base, previous)?;
        }
        if previous == next && source != Source::New {
            return Ok(());
        }
        let encoded = encode(next)?;
        profile::atomic_write(&base.join("profiles.json.bak"), &encode(previous)?)?;
        profile::atomic_write(&base.join("profiles.json"), &encoded)
    }

    pub fn edit(base: &Path, f: impl FnOnce(&mut Self) -> io::Result<()>) -> io::Result<Self> {
        let _lock = lock_catalog(base)?;
        let (previous, source) = Self::read_snapshot(base)?;
        let mut next = previous.clone();
        f(&mut next)?;
        Self::save(base, &previous, source, &next)?;
        Ok(next)
    }
    pub fn create(&mut self, name: &str) -> io::Result<String> {
        if self.profiles.len() >= 16 {
            return Err(io::Error::other("Максимум 16 профилей"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(io::Error::other("Имя: от 1 до 60 символов"));
        }
        let id = unique_id()?;
        self.profiles.push(Profile {
            id: id.clone(),
            name: name.into(),
            avatar: String::new(),
        });
        Ok(id)
    }

    pub fn create_profile(base: &Path, name: &str) -> io::Result<Self> {
        let _lock = lock_catalog(base)?;
        let (previous, source) = Self::read_snapshot(base)?;
        let mut next = previous.clone();
        let id = next.create(name)?;
        ensure_directory(&base.join("Profiles"))?;
        let path = root(base, &id)?;
        let record = next.profiles.last().expect("create appends one profile");
        let encoded = encode(record)?;
        fs::create_dir(&path)?;
        let marker = path.join(PROFILE_RECORD);
        let result = profile::atomic_write(&marker, &encoded)
            .and_then(|()| Self::save(base, &previous, source, &next));
        if let Err(error) = result {
            let _ = fs::remove_file(marker);
            let _ = fs::remove_dir(&path);
            return Err(error);
        }
        Ok(next)
    }

    /// Validate membership and take the per-profile lock while the registry is locked.
    pub fn activate(base: &Path, id: &str) -> io::Result<(Self, profile::ProfileLock)> {
        let _guard = lock_catalog(base)?;
        let (previous, source) = Self::read_snapshot(base)?;
        if !previous.profiles.iter().any(|p| p.id == id) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Профиль не найден"));
        }
        let path = root(base, id)?;
        if id != "main" {
            ensure_directory(&base.join("Profiles"))?;
        }
        ensure_directory(&path)?;
        let lock = profile::ProfileLock::acquire(&path)?;
        let mut next = previous.clone();
        next.selected = id.into();
        Self::save(base, &previous, source, &next)?;
        Ok((next, lock))
    }
    pub fn update(&mut self, id: &str, name: &str, avatar: &str) -> io::Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 || !valid_avatar(avatar) {
            return Err(io::Error::other(
                "Проверьте имя и изображение PNG до 100 КБ",
            ));
        }
        let p = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| io::Error::other("Профиль не найден"))?;
        p.name = name.into();
        p.avatar = avatar.into();
        Ok(())
    }
    pub fn delete(base: &Path, id: &str, current: &str) -> io::Result<Self> {
        if id == "main" || id == current {
            return Err(io::Error::other("Нельзя удалить открытый или основной профиль. Закройте нужный профиль и выберите другой."));
        }
        let path = root(base, id)?;
        let _catalog_lock = lock_catalog(base)?;
        let (previous, source) = Self::read_snapshot(base)?;
        let record = previous
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| io::Error::other("Профиль не найден"))?;
        ensure_directory(&base.join("Profiles"))?;
        ensure_directory(&path)?;
        let profile_lock = profile::ProfileLock::lock_existing(&path)?;
        let trash = base.join("ProfileTrash");
        ensure_directory(&trash)?;
        let target = trash.join(id);
        if real_directory(&target)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Archived profile already exists",
            ));
        }
        profile::atomic_write(&path.join(PROFILE_RECORD), &encode(record)?)?;
        let mut next = previous.clone();
        next.profiles.retain(|p| p.id != id);
        if next.selected == id {
            next.selected = "main".into();
        }
        // The catalogue lock still prevents a new owner. Windows may reject
        // moving a directory while a handle to a file inside it is open.
        drop(profile_lock);
        move_profile(&path, &target, || {
            Self::save(base, &previous, source, &next)
        })?;
        Ok(next)
    }

    pub fn restore(base: &Path, id: &str) -> io::Result<Self> {
        let _lock = lock_catalog(base)?;
        let archived = archived_root(base, id)?;
        let (previous, source) = Self::read_snapshot(base)?;
        if previous.profiles.len() >= 16 {
            return Err(io::Error::other("Максимум 16 профилей"));
        }
        let path = root(base, id)?;
        ensure_directory(&base.join("Profiles"))?;
        if real_directory(&path)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Active profile directory already exists",
            ));
        }
        let record = read_record(&archived, id).unwrap_or_else(|| Profile {
            id: id.into(),
            name: format!("Profile {}", &id[..8]),
            avatar: String::new(),
        });
        profile::atomic_write(&archived.join(PROFILE_RECORD), &encode(&record)?)?;
        let mut next = previous.clone();
        next.profiles.push(record);
        move_profile(&archived, &path, || {
            Self::save(base, &previous, source, &next)
        })?;
        Ok(next)
    }

    pub fn purge(base: &Path, id: &str) -> io::Result<()> {
        let _lock = lock_catalog(base)?;
        let archived = archived_root(base, id)?;
        let (previous, source) = Self::read_snapshot(base)?;
        let primary_has_id = Self::read_file(&base.join("profiles.json"))
            .is_ok_and(|catalog| catalog.profiles.iter().any(|p| p.id == id));
        let backup_has_id = Self::read_file(&base.join("profiles.json.bak"))
            .is_ok_and(|catalog| catalog.profiles.iter().any(|p| p.id == id));
        if primary_has_id || backup_has_id {
            // Clear older references first, or recovery could resurrect an empty profile.
            let bytes = encode(&previous)?;
            profile::atomic_write(&base.join("profiles.json.bak"), &bytes)?;
            if source == Source::Backup {
                Self::repair(base, &previous)?;
            } else {
                profile::atomic_write(&base.join("profiles.json"), &bytes)?;
            }
        }
        fs::remove_dir_all(archived)
    }

    pub fn details(&self, base: &Path) -> io::Result<ProfileDetails> {
        let bytes = self
            .profiles
            .iter()
            .map(|p| {
                let size = root(base, &p.id)
                    .and_then(|path| directory_bytes(&path, p.id == "main"))
                    .ok();
                (p.id.clone(), size)
            })
            .collect();
        let mut archived = Vec::new();
        let trash = base.join("ProfileTrash");
        if real_directory(&trash)? {
            for entry in fs::read_dir(&trash)? {
                let entry = entry?;
                let Ok(id) = entry.file_name().into_string() else {
                    continue;
                };
                if id == "main" || !valid_id(&id) || !real_directory(&entry.path()).unwrap_or(false)
                {
                    continue;
                }
                archived.push(ArchivedProfile {
                    name: read_record(&entry.path(), &id).map_or_else(String::new, |p| p.name),
                    bytes: directory_bytes(&entry.path(), false).ok(),
                    id,
                });
            }
        }
        archived.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(ProfileDetails { bytes, archived })
    }
}

fn archived_root(base: &Path, id: &str) -> io::Result<PathBuf> {
    if id == "main" || !valid_id(id) {
        return Err(io::Error::other("Некорректный профиль"));
    }
    let trash = base.join("ProfileTrash");
    let path = trash.join(id);
    if !real_directory(&trash)? || !real_directory(&path)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Архив профиля не найден",
        ));
    }
    Ok(path)
}

fn move_profile(from: &Path, to: &Path, save: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    fs::rename(from, to)?;
    if let Err(error) = save() {
        fs::rename(to, from)?;
        return Err(error);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let base = std::env::temp_dir().join(format!("literust-profiles-{}", unique_id().unwrap()));
        fs::create_dir(&base).unwrap();
        base
    }
    #[test]
    fn paths_never_escape() {
        let base = Path::new("/browser");
        assert_eq!(root(base, "main").unwrap(), base);
        for id in ["../secret", "", "MAIN", "C:\\data", "../../"] {
            assert!(root(base, id).is_err());
        }
        assert_ne!(
            root(base, "00000000000000000000000000000001").unwrap(),
            root(base, "00000000000000000000000000000002").unwrap()
        );
    }
    #[test]
    fn create_rename_and_validate() {
        let mut c = Catalog::default();
        let id = c.create("Работа").unwrap();
        assert!(valid_id(&id));
        c.update(&id, "Дом", "").unwrap();
        assert_eq!(c.profiles[1].name, "Дом");
        assert!(c.update(&id, "", "data:image/svg+xml;base64,evil").is_err());
        assert!(c.create("").is_err());
    }
    #[test]
    fn catalog_roundtrip_and_legacy_untouched() {
        let base = std::env::temp_dir().join(format!("literust-profiles-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("legacy-marker"), b"keep").unwrap();
        let c = Catalog::edit(&base, |c| {
            c.create("Second")?;
            Ok(())
        })
        .unwrap();
        let loaded = Catalog::load(&base).unwrap();
        assert_eq!(loaded.profiles.len(), c.profiles.len());
        assert_eq!(fs::read(base.join("legacy-marker")).unwrap(), b"keep");
        assert!(Catalog::delete(&base, "main", "main").is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn catalogue_recovers_only_valid_backup_and_preserves_damaged_bytes() {
        let base = fixture();
        let catalog = Catalog::create_profile(&base, "Recovery fixture").unwrap();
        let ready = base.join("profiles.json");
        let backup = base.join("profiles.json.bak");
        let previous = fs::read(&backup).unwrap();
        fs::write(&ready, b"damaged").unwrap();
        let repaired = Catalog::recover(&base).unwrap();
        assert_eq!(repaired, catalog);
        assert_eq!(Catalog::read_file(&ready).unwrap(), repaired);
        assert_eq!(fs::read(&backup).unwrap(), previous);
        assert!(fs::read_dir(&base)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("profiles.corrupt-")
                    && fs::read(entry.path()).unwrap() == b"damaged"
            }));

        let invalid = Catalog {
            selected: "missing".into(),
            ..Catalog::default()
        };
        fs::write(&backup, encode(&invalid).unwrap()).unwrap();
        fs::write(&ready, b"damaged again").unwrap();
        assert!(Catalog::recover(&base).is_err());
        assert_eq!(fs::read(ready).unwrap(), b"damaged again");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn archive_restore_and_purge_keep_profile_data_and_respect_the_lock() {
        let base = fixture();
        let catalog = Catalog::create_profile(&base, "Работа").unwrap();
        let id = &catalog.profiles[1].id;
        let path = root(&base, id).unwrap();
        fs::write(path.join("test-cookie-fixture"), b"synthetic").unwrap();
        let (active, lock) = Catalog::activate(&base, id).unwrap();
        assert_eq!(active.selected, *id);
        assert!(Catalog::activate(&base, id).is_err());
        assert!(Catalog::delete(&base, id, "main").is_err());
        drop(lock);

        let archived = Catalog::delete(&base, id, "main").unwrap();
        assert_eq!(archived.selected, "main");
        assert!(!path.exists());
        let details = archived.details(&base).unwrap();
        assert_eq!(details.archived[0].name, "Работа");
        assert!(details.archived[0].bytes.unwrap() >= 9);

        // A recovery must respect the on-disk archive even if the backup listed the profile.
        fs::write(base.join("profiles.json"), b"invalid").unwrap();
        assert_eq!(Catalog::recover(&base).unwrap().profiles.len(), 1);
        let restored = Catalog::restore(&base, id).unwrap();
        assert_eq!(restored.profiles[1].id, *id);
        assert_eq!(
            fs::read(path.join("test-cookie-fixture")).unwrap(),
            b"synthetic"
        );
        assert!(Catalog::purge(&base, id).is_err());

        Catalog::delete(&base, id, "main").unwrap();
        Catalog::purge(&base, id).unwrap();
        assert_eq!(Catalog::load(&base).unwrap().profiles.len(), 1);
        assert!(Catalog::restore(&base, id).is_err());
        assert!(Catalog::purge(&base, "../main").is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
