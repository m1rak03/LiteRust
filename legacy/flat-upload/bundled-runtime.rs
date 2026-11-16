//! Standalone distribution: browser + Microsoft Evergreen offline installer.
//! Uses the same browser payload as alpha.2; only packaging changes.
#![windows_subsystem = "windows"]
#[cfg(windows)]
use std::process::Command;
use std::{fs, io, path::Path};
#[cfg(windows)]
const BROWSER: &[u8] = include_bytes!(env!("LITERUST_BROWSER_PAYLOAD"));
#[cfg(windows)]
const LOADER: &[u8] = include_bytes!(env!("LITERUST_LOADER_PAYLOAD"));
#[cfg(windows)]
const XRAY: &[u8] = include_bytes!(env!("LITERUST_XRAY_PAYLOAD"));
#[cfg(windows)]
const XRAY_LICENSE: &[u8] = include_bytes!("../vendor/xray/LICENSE");
#[cfg(windows)]
const XRAY_SOURCE: &[u8] = b"Unmodified Xray-core v26.11.9 (MPL-2.0). Source: https://github.com/XTLS/Xray-core/tree/v26.11.9\n";
#[cfg(windows)]
const RUNTIME_INSTALLER: &[u8] = include_bytes!(env!("LITERUST_RUNTIME_INSTALLER"));

// Stream comparisons: the 205 MiB installer must never be copied into a Vec.
fn matches(path: &Path, bytes: &[u8]) -> bool {
    use io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    if !file.metadata().is_ok_and(|m| m.len() == bytes.len() as u64) {
        return false;
    }
    let mut chunk = [0u8; 65536];
    for expected in bytes.chunks(chunk.len()) {
        if file.read_exact(&mut chunk[..expected.len()]).is_err()
            || &chunk[..expected.len()] != expected
        {
            return false;
        }
    }
    true
}
fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if matches(path, bytes) {
        return Ok(());
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        use io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path).or_else(|error| {
            if matches(path, bytes) {
                Ok(())
            } else {
                Err(error)
            }
        })
    })();
    let _ = fs::remove_file(temp);
    result
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(memory: *mut c_void);
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut c_void, text: *const u16, title: *const u16, flags: u32)
            -> i32;
    }
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    pub fn message(text: &str, flags: u32) -> i32 {
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "LiteRust — установка и запуск"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) }
    }
    pub fn runtime_available(loader: &Path) -> io::Result<bool> {
        let path: Vec<u16> = loader.as_os_str().encode_wide().chain(Some(0)).collect();
        // Resolve dependencies only beside the verified DLL and in System32.
        let library =
            Library(unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), 0x100 | 0x800) });
        if library.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        let address = unsafe {
            GetProcAddress(
                library.0,
                c"GetAvailableCoreWebView2BrowserVersionString"
                    .as_ptr()
                    .cast(),
            )
        };
        if address.is_null() {
            return Err(io::Error::last_os_error());
        }
        type GetVersion = unsafe extern "system" fn(*const u16, *mut *mut u16) -> i32;
        let get_version: GetVersion = unsafe { std::mem::transmute(address) };
        let mut version = std::ptr::null_mut();
        let result = unsafe { get_version(std::ptr::null(), &mut version) };
        let available = result >= 0 && !version.is_null() && unsafe { *version != 0 };
        if !version.is_null() {
            unsafe {
                CoTaskMemFree(version.cast());
            }
        }
        Ok(available)
    }
}

// Kept separate so cancel, install failure and recheck behavior are testable.
fn ensure_runtime(
    mut available: impl FnMut() -> io::Result<bool>,
    confirm: impl FnOnce() -> bool,
    install_runtime: impl FnOnce() -> io::Result<()>,
) -> io::Result<bool> {
    if available()? {
        return Ok(true);
    }
    if !confirm() {
        return Ok(false);
    }
    install_runtime()?;
    if !available()? {
        return Err(io::Error::other("WebView2 пока недоступен. Если установщик запросил перезагрузку Windows, перезагрузите компьютер и запустите LiteRust снова."));
    }
    Ok(true)
}

#[cfg(windows)]
fn run() -> io::Result<()> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("Не найдена LOCALAPPDATA"))?;
    let base = std::path::PathBuf::from(base);
    if !base.is_absolute() {
        return Err(io::Error::other(
            "LOCALAPPDATA должен быть абсолютным путём",
        ));
    }
    let folder = base
        .join("LiteRust")
        .join("app")
        .join("0.11.0-alpha.2-bundled-x64");
    fs::create_dir_all(&folder)?;
    let loader = folder.join("WebView2Loader.dll");
    install(&loader, LOADER)?;
    let ready = ensure_runtime(
        || native::runtime_available(&loader),
        || {
            native::message("Для LiteRust нужен Microsoft WebView2 Runtime. Его полный установщик уже включён в этот EXE; отдельно скачивать ничего не нужно.\n\nУстановить WebView2 и затем запустить LiteRust? Установка может занять несколько минут.", 0x4 | 0x20) == 6
        },
        || {
            let installer = folder.join(format!("WebView2Setup-{}.exe", std::process::id()));
            install(&installer, RUNTIME_INSTALLER)?;
            // Show Microsoft's own installation UI; do not request elevation.
            let result = Command::new(&installer).arg("/install").status();
            let _ = fs::remove_file(&installer);
            let status = result?;
            if status.success() || status.code() == Some(3010) {
                Ok(())
            } else {
                Err(io::Error::other(format!("Установка WebView2 не завершена (код {:?}). LiteRust не запущен. Повторите запуск EXE, чтобы попробовать ещё раз.", status.code())))
            }
        },
    )?;
    if !ready {
        return Ok(());
    }
    let browser = folder.join("literust-browser.exe");
    install(&browser, BROWSER)?;
    #[cfg(windows)]
    {
        install(&folder.join("xray.exe"), XRAY)?;
        install(&folder.join("XRAY-LICENSE.txt"), XRAY_LICENSE)?;
        install(&folder.join("XRAY-SOURCE.txt"), XRAY_SOURCE)?;
    }
    Command::new(browser)
        .args(std::env::args_os().skip(1))
        .spawn()?;
    Ok(())
}
#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        native::message(&format!("Не удалось запустить LiteRust:\n{error}"), 0x10);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_runtime_skips_installer_and_prompt() {
        assert!(ensure_runtime(
            || Ok(true),
            || panic!("unexpected prompt"),
            || panic!("unexpected install")
        )
        .unwrap());
    }
    #[test]
    fn cancellation_stops_before_install() {
        assert!(!ensure_runtime(|| Ok(false), || false, || panic!("unexpected install")).unwrap());
    }
    #[test]
    fn successful_install_is_rechecked() {
        let mut checks = 0;
        assert!(ensure_runtime(
            || {
                checks += 1;
                Ok(checks == 2)
            },
            || true,
            || Ok(())
        )
        .unwrap());
        assert_eq!(checks, 2);
    }
    #[test]
    fn installer_failure_prevents_launch() {
        assert!(
            ensure_runtime(|| Ok(false), || true, || Err(io::Error::other("failure"))).is_err()
        );
    }
    #[test]
    fn missing_runtime_after_install_prevents_launch() {
        assert!(ensure_runtime(|| Ok(false), || true, || Ok(())).is_err());
    }
    #[test]
    fn streamed_payload_is_reused_and_repaired() {
        let root =
            std::env::temp_dir().join(format!("literust-bundle-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test.bin");
        let bytes = vec![42; 131077];
        install(&path, &bytes).unwrap();
        assert!(matches(&path, &bytes));
        fs::write(&path, vec![41; bytes.len()]).unwrap();
        assert!(!matches(&path, &bytes));
        install(&path, &bytes).unwrap();
        assert!(matches(&path, &bytes));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
