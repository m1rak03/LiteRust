//! Local profile catalogue. Legacy data stays in place; new profiles get isolated roots.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub avatar: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub selected: String,
    pub profiles: Vec<Profile>,
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

    pub fn load(base: &Path) -> io::Result<Self> {
        let file = base.join("profiles.json");
        if !file.exists() {
            return Ok(Self::default());
        }
        if fs::metadata(&file)?.len() > 2_000_000 {
            return Err(io::Error::other("Слишком большой каталог профилей"));
        }
        let value: Self = serde_json::from_slice(&crate::protected_data::open(&fs::read(file)?)?)
            .map_err(io::Error::other)?;
        let mut ids = std::collections::BTreeSet::new();
        if value.profiles.is_empty()
            || value.profiles.len() > 16
            || !value.profiles.iter().all(|p| {
                valid_id(&p.id)
                    && ids.insert(&p.id)
                    && !p.name.trim().is_empty()
                    && p.name.chars().count() <= 60
                    && valid_avatar(&p.avatar)
            })
            || !value.profiles.iter().any(|p| p.id == value.selected)
            || !value.profiles.iter().any(|p| p.id == "main")
        {
            return Err(io::Error::other(
                "Каталог профилей повреждён; исходный файл сохранён",
            ));
        }
        Ok(value)
    }
    pub fn edit(base: &Path, f: impl FnOnce(&mut Self) -> io::Result<()>) -> io::Result<Self> {
        fs::create_dir_all(base)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(base.join("profiles.lock"))?;
        lock.try_lock()
            .map_err(|_| io::Error::other("Каталог профилей занят. Повторите действие."))?;
        let mut c = Self::load(base)?;
        f(&mut c)?;
        let bytes = serde_json::to_vec(&c).map_err(io::Error::other)?;
        let file = base.join("profiles.json");
        if file.exists() {
            crate::profile::atomic_write(&base.join("profiles.json.bak"), &fs::read(&file)?)?;
        }
        crate::profile::atomic_write(&file, &crate::protected_data::seal(&bytes)?)?;
        Ok(c)
    }
    pub fn create(&mut self, name: &str) -> io::Result<String> {
        if self.profiles.len() >= 16 {
            return Err(io::Error::other("Максимум 16 профилей"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(io::Error::other("Имя: от 1 до 60 символов"));
        }
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        self.profiles.push(Profile {
            id: id.clone(),
            name: name.into(),
            avatar: String::new(),
        });
        Ok(id)
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
        let _lock = crate::profile::ProfileLock::acquire(&path)?;
        let result = Self::edit(base, |c| {
            if !c.profiles.iter().any(|p| p.id == id) {
                return Err(io::Error::other("Профиль не найден"));
            }
            let trash = base.join("ProfileTrash");
            fs::create_dir_all(&trash)?;
            let target = trash.join(id);
            fs::rename(&path, &target)?;
            c.profiles.retain(|p| p.id != id);
            if c.selected == id {
                c.selected = current.into();
            }
            Ok(())
        });
        if result.is_err() {
            let archived = base.join("ProfileTrash").join(id);
            if !path.exists() && archived.exists() {
                fs::rename(archived, &path)?;
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
