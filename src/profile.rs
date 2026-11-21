//! Profile ownership and crash-resistant same-directory replacement.
use crate::storage::MAX_BROWSER_DATA_BYTES;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub struct ProfileLock {
    _file: File,
}
impl ProfileLock {
    /// Takes ownership of an existing profile directory for maintenance.
    pub(crate) fn lock_existing(root: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("profile.lock"))?;
        file.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "Профиль открыт в другом окне LiteRust",
            ),
            fs::TryLockError::Error(error) => error,
        })?;
        Ok(Self { _file: file })
    }

    pub fn acquire(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let lock = Self::lock_existing(root)?;
        // Preserve the exact pre-migration bytes, including records beyond limits.
        let original = root.join("browser-data.json");
        if original.exists() {
            if fs::metadata(&original)?.len() > MAX_BROWSER_DATA_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Профиль превышает 8 MiB; исходный файл сохранён без изменений",
                ));
            }
            let archive = root.join("browser-data.pre-v09.json");
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&archive)
            {
                Ok(output) => {
                    drop(output);
                    let result = File::open(original).and_then(|input| {
                        let mut bytes = Vec::new();
                        input
                            .take(MAX_BROWSER_DATA_BYTES + 1)
                            .read_to_end(&mut bytes)?;
                        let size = bytes.len() as u64;
                        if size > MAX_BROWSER_DATA_BYTES {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "profile grew beyond limit",
                            ));
                        }
                        atomic_write(&archive, &crate::protected_data::protect_backup(&bytes)?)?;
                        Ok(size)
                    });
                    if let Err(error) = result {
                        let _ = fs::remove_file(archive);
                        return Err(error);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
        // Encrypt historical backup copies as well; leave unrelated files alone.
        for name in ["browser-data.json.bak", "browser-data.pre-v09.json"] {
            let path = root.join(name);
            if path.exists() {
                // State loading independently verifies the primary and its backup.
                // Preserve an oversized backup for recovery without reading it at startup.
                let Ok(bytes) = read_limited(&path, MAX_BROWSER_DATA_BYTES) else {
                    continue;
                };
                let protected = crate::protected_data::protect_backup(&bytes)?;
                if protected != bytes {
                    atomic_write(&path, &protected)?;
                }
            }
        }
        Ok(lock)
    }
}

pub(crate) fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Profile file exceeds size limit",
        ));
    }
    Ok(bytes)
}

pub(crate) fn unique_id() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn preserve_damaged(path: &Path) -> io::Result<()> {
    let suffix = unique_id()?;
    match fs::rename(path, path.with_extension(format!("corrupt-{suffix}.json"))) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("pending");
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_profile_is_not_copied_or_overwritten() {
        let root = std::env::temp_dir().join(format!("literust-large-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let original = root.join("browser-data.json");
        File::create(&original)
            .unwrap()
            .set_len(MAX_BROWSER_DATA_BYTES + 1)
            .unwrap();
        assert!(ProfileLock::acquire(&root).is_err());
        assert_eq!(
            fs::metadata(&original).unwrap().len(),
            MAX_BROWSER_DATA_BYTES + 1
        );
        assert!(!root.join("browser-data.pre-v09.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lock_releases_and_preserves_migration_source() {
        let root = std::env::temp_dir().join(format!("literust-lock-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("browser-data.json"), b"{original}").unwrap();
        let lock = ProfileLock::acquire(&root).unwrap();
        assert!(ProfileLock::acquire(&root).is_err());
        assert_eq!(
            crate::protected_data::open(&fs::read(root.join("browser-data.pre-v09.json")).unwrap())
                .unwrap(),
            b"{original}"
        );
        drop(lock);
        let second = ProfileLock::acquire(&root).unwrap();
        drop(second);
        fs::remove_dir_all(root).unwrap();
    }
}
