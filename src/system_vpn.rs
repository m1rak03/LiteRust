//! Owned elevated TUN helper. Dropping the control connection stops the helper.
use std::{
    io::{self, Read},
    net::{Shutdown, TcpListener, TcpStream},
    os::windows::process::CommandExt,
    process::Command,
    time::{Duration, Instant},
};
pub struct SystemTunnel {
    stream: TcpStream,
}
impl SystemTunnel {
    pub fn start(port: u16) -> io::Result<Self> {
        Self::start_inner(port, false)
    }
    fn start_inner(port: u16, test_routes: bool) -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let helper = exe
            .parent()
            .ok_or_else(|| io::Error::other("Нет папки приложения"))?
            .join("system-vpn.ps1");
        if !helper.is_file() || !helper.with_file_name("sing-box.exe").is_file() {
            return Err(io::Error::other("Не установлен компонент системного VPN"));
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes)
            .map_err(|_| io::Error::other("Ошибка генератора случайных чисел"))?;
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        use base64::Engine;
        let helper_text = helper.to_string_lossy().replace('\'', "''");
        let wrapper = format!(
            r#"[IO.File]::WriteAllText('{helper_text}.wrapper.log','wrapper entered');try {{ & '{helper_text}' -ProxyPort {port} -ParentId {} -ControlPort {} -Token '{token}' {} }} catch {{ [IO.File]::WriteAllText('{helper_text}.fatal.log',($_ | Out-String));exit 1 }}"#,
            std::process::id(),
            listener.local_addr()?.port(),
            if test_routes { "-TestRoutes" } else { "" }
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            wrapper
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let script = r#"$ErrorActionPreference='Stop';$a='-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand '+$env:LR_WRAPPER;$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent());$admin=$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);[IO.File]::WriteAllText($env:LR_HELPER+'.launcher.log',('admin='+$admin));if($admin){$p=Start-Process -FilePath "$PSHOME\powershell.exe" -WindowStyle Hidden -ArgumentList $a -PassThru -RedirectStandardError ($env:LR_HELPER+'.stderr.log')}else{$p=Start-Process -FilePath "$PSHOME\powershell.exe" -Verb RunAs -WindowStyle Hidden -ArgumentList $a -PassThru};Start-Sleep -Milliseconds 750;$p.Refresh();[IO.File]::AppendAllText($env:LR_HELPER+'.launcher.log',('; exited='+$p.HasExited));if($p.HasExited){[IO.File]::AppendAllText($env:LR_HELPER+'.launcher.log',('; code='+$p.ExitCode));exit 1}"#;
        let result = Command::new(crate::security::windows_system_binary(
            "WindowsPowerShell/v1.0/powershell.exe",
        )?)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("LR_HELPER", helper)
        .env("LR_WRAPPER", encoded)
        .env("LR_PROXY", port.to_string())
        .env("LR_PARENT", std::process::id().to_string())
        .env("LR_CONTROL", listener.local_addr()?.port().to_string())
        .env("LR_TOKEN", &token)
        .env("LR_TEST", if test_routes { " -TestRoutes" } else { "" })
        .creation_flags(0x08000000)
        .status()?;
        if !result.success() {
            return Err(io::Error::other(
                "Запуск системного VPN не подтверждён в Windows",
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(35);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Winsock inherits FIONBIO from the listening socket. Wait for
                    // the helper's authenticated readiness line before switching back.
                    stream.set_nonblocking(false)?;
                    stream.set_read_timeout(Some(Duration::from_secs(35)))?;
                    let mut line = Vec::new();
                    let mut byte = [0];
                    while line.len() < 128 {
                        match stream.read(&mut byte) {
                            Ok(1) if byte[0] != b'\n' => line.push(byte[0]),
                            _ => break,
                        }
                    }
                    if String::from_utf8_lossy(&line).trim() == format!("{token} READY") {
                        stream.set_nonblocking(true)?;
                        return Ok(Self { stream });
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("Системный VPN не подтвердил запуск"))
    }
    pub fn alive(&self) -> bool {
        match self.stream.peek(&mut [0]) {
            Err(e) => e.kind() == io::ErrorKind::WouldBlock,
            Ok(n) => n > 0,
        }
    }
}
impl Drop for SystemTunnel {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Write);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.stream.read(&mut [0u8; 1]) {
                Ok(0) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => break,
            }
        }
    }
}

/// Windows CI exercise: routes only reserved benchmarking addresses, never the runner's internet traffic.
pub fn self_test() -> io::Result<()> {
    use std::io::Write;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let server = std::thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut head = [0u8; 2];
        stream.read_exact(&mut head)?;
        if head[0] != 5 {
            return Err(io::Error::other("SOCKS version"));
        }
        let mut methods = vec![0; head[1] as usize];
        stream.read_exact(&mut methods)?;
        stream.write_all(&[5, 0])?;
        let mut request = [0; 4];
        stream.read_exact(&mut request)?;
        if request[1] != 1 {
            return Err(io::Error::other("SOCKS command"));
        }
        let len = match request[3] {
            1 => 4,
            4 => 16,
            3 => {
                let mut n = [0];
                stream.read_exact(&mut n)?;
                n[0] as usize
            }
            _ => return Err(io::Error::other("SOCKS address")),
        };
        let mut address = vec![0; len + 2];
        stream.read_exact(&mut address)?;
        stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])?;
        let mut request = [0; 512];
        let _ = stream.read(&mut request)?;
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nLiteRustTUN",
        )?;
        Ok(())
    });
    let tun = SystemTunnel::start_inner(port, true)?;
    let mut stream =
        TcpStream::connect_timeout(&"198.18.0.1:8080".parse().unwrap(), Duration::from_secs(10))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if !response.ends_with("LiteRustTUN") {
        return Err(io::Error::other("TUN did not reach SOCKS fixture"));
    }
    server
        .join()
        .map_err(|_| io::Error::other("Fixture panicked"))??;
    drop(tun);
    Ok(())
}
