//! Single-file distribution launcher. Build after the browser, with the two
//! absolute payload paths supplied as environment variables to rustc.
#![windows_subsystem = "windows"]
use std::{fs, io, path::Path, process::Command};
const BROWSER: &[u8] = include_bytes!(env!("LITERUST_BROWSER_PAYLOAD"));
const LOADER: &[u8] = include_bytes!(env!("LITERUST_LOADER_PAYLOAD"));
#[cfg(windows)]
const XRAY: &[u8] = include_bytes!(env!("LITERUST_XRAY_PAYLOAD"));
#[cfg(windows)]
const XRAY_LICENSE: &[u8] = include_bytes!("../vendor/xray/LICENSE");
#[cfg(windows)]
const XRAY_SOURCE: &[u8] = b"Unmodified Xray-core v26.11.9 (MPL-2.0). Source: https://github.com/XTLS/Xray-core/tree/v26.11.9\n";

fn matches(path: &Path, bytes: &[u8]) -> bool {
    fs::metadata(path).is_ok_and(|m| m.len() == bytes.len() as u64)
        && fs::read(path).is_ok_and(|existing| existing == bytes)
}

fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if matches(path, bytes) {
        return Ok(());
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        use io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path).or_else(|err| {
            if matches(path, bytes) {
                Ok(())
            } else {
                Err(err)
            }
        })
    })();
    let _ = fs::remove_file(temp);
    result
}

fn run() -> io::Result<()> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("Не найдена папка LOCALAPPDATA"))?;
    let base = std::path::PathBuf::from(base);
    if !base.is_absolute() {
        return Err(io::Error::other(
            "LOCALAPPDATA должен быть абсолютным путём",
        ));
    }
    let folder = base.join("LiteRust").join("app").join("0.11.0-alpha.1-x64");
    fs::create_dir_all(&folder)?;
    install(&folder.join("WebView2Loader.dll"), LOADER)?;
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
    if let Err(err) = run() {
        let message = format!("Не удалось запустить LiteRust: {err}\n\nЗакройте другие окна LiteRust и повторите запуск. Можно также воспользоваться portable-архивом.");
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "LiteRust".encode_utf16().chain(Some(0)).collect();
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                title: *const u16,
                kind: u32,
            ) -> i32;
        }
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_reuses_and_repairs_payload() {
        let root =
            std::env::temp_dir().join(format!("literust-launcher-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("payload.bin");
        install(&file, b"expected payload").unwrap();
        assert!(matches(&file, b"expected payload"));
        install(&file, b"expected payload").unwrap();
        fs::write(&file, b"damaged").unwrap();
        install(&file, b"expected payload").unwrap();
        assert!(matches(&file, b"expected payload"));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
