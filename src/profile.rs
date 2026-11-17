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
    pub fn acquire(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("profile.lock"))?;
        file.try_lock().map_err(|_| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "Профиль уже используется другим экземпляром LiteRust или недоступен",
            )
        })?;
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
                        atomic_write(&archive, &crate::protected_data::migrate(&bytes)?)?;
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
                if fs::metadata(&path)?.len() > MAX_BROWSER_DATA_BYTES {
                    return Err(io::Error::other("Profile backup exceeds size limit"));
                }
                let bytes = fs::read(&path)?;
                let protected = crate::protected_data::migrate(&bytes)?;
                if protected != bytes {
                    atomic_write(&path, &protected)?;
                }
            }
        }
        Ok(Self { _file: file })
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
