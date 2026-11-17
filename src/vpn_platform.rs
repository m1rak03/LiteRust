//! Windows user-bound encrypted vault and an owned, kill-on-close Xray process.
use crate::{
    profile,
    vpn::{self, Vault},
};
use std::{
    fs,
    io::{self, Write},
    net::{TcpListener, TcpStream},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
        System::JobObjects::*,
    },
};
fn fail(message: &str) -> io::Error {
    io::Error::other(message)
}
pub(crate) fn crypt(bytes: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| fail("VPN-профиль слишком большой"))?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|_| {
            fail("Не удалось открыть защищённый VPN-профиль этого пользователя Windows")
        })?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(bytes)
    }
}
pub fn load(root: &Path) -> io::Result<Vault> {
    let file = root.join("vpn.dpapi");
    match fs::metadata(&file) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vault::default()),
        Err(e) => return Err(e),
        Ok(m) if m.len() > 2 * 1024 * 1024 => return Err(fail("VPN-профиль превышает лимит")),
        _ => (),
    }
    let bytes = crypt(&fs::read(file)?, false)?;
    let vault: Vault = serde_json::from_slice(&bytes).map_err(|_| fail("Повреждён VPN-профиль"))?;
    if vault.nodes.len() > vpn::MAX_NODES || vault.enabled && vault.active().is_err() {
        return Err(fail("Некорректный VPN-профиль"));
    }
    Ok(vault)
}
pub fn save(root: &Path, vault: &Vault) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let data = serde_json::to_vec(vault).map_err(|_| fail("Не удалось сохранить VPN"))?;
    profile::atomic_write(&root.join("vpn.dpapi"), &crypt(&data, true)?)
}
struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub struct Tunnel {
    child: Child,
    _job: Job,
    pub port: u16,
    pub socks_port: u16,
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Tunnel {
    pub fn start(vault: &Vault) -> io::Result<Self> {
        let mut node = vault.active().map_err(fail)?.clone();
        if vault.system_scope {
            use std::net::ToSocketAddrs;
            let key = if node.outbound["settings"]["vnext"].is_array() {
                "vnext"
            } else {
                "servers"
            };
            let host = node.outbound["settings"][key][0]["address"]
                .as_str()
                .ok_or_else(|| fail("Нет адреса VPN-сервера"))?
                .to_string();
            if host.parse::<std::net::IpAddr>().is_err() {
                let port = node.outbound["settings"][key][0]["port"]
                    .as_u64()
                    .unwrap_or(443) as u16;
                let ip = (host.as_str(), port)
                    .to_socket_addrs()?
                    .next()
                    .ok_or_else(|| fail("Не удалось разрешить адрес VPN"))?
                    .ip();
                node = vpn::pin_address(&node, ip);
            }
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let socks_listener = TcpListener::bind(("127.0.0.1", 0))?;
        let socks_port = socks_listener.local_addr()?.port();
        drop(listener);
        drop(socks_listener);
        let engine = std::env::current_exe()?
            .parent()
            .ok_or_else(|| fail("Не найдена папка приложения"))?
            .join("xray.exe");
        if !engine.is_file() {
            return Err(fail(
                "Рядом с браузером нет xray.exe. Распакуйте полный комплект LiteRust",
            ));
        }
        let job = Job(unsafe {
            CreateJobObjectW(None, PCWSTR::null())
                .map_err(|_| fail("Не удалось создать группу VPN-процессов"))?
        });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|_| fail("Не удалось настроить остановку VPN"))?;
        }
        let mut child = Command::new(engine)
            .args(["run", "-config", "stdin:", "-format", "json"])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| fail("Не удалось запустить движок VPN"))?;
        if unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) }.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(fail("Не удалось привязать VPN к процессу браузера"));
        }
        let mut tunnel = Self {
            child,
            _job: job,
            port,
            socks_port,
        };
        let mut config = vpn::config(&node, port);
        config["inbounds"].as_array_mut().unwrap().push(serde_json::json!({"tag":"system","listen":"127.0.0.1","port":socks_port,"protocol":"socks","settings":{"auth":"noauth","udp":true}}));
        let data = serde_json::to_vec(&config).map_err(|_| fail("Ошибка VPN-конфигурации"))?;
        tunnel
            .child
            .stdin
            .take()
            .ok_or_else(|| fail("Не удалось передать настройки VPN"))?
            .write_all(&data)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if tunnel.child.try_wait()?.is_some() {
                return Err(fail("Движок VPN отклонил ключ или не смог запуститься"));
            }
            if TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}").parse().unwrap(),
                Duration::from_millis(100),
            )
            .is_ok()
            {
                return Ok(tunnel);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(fail("Движок VPN не открыл локальный порт за 5 секунд"))
    }
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}
