//! Native, non-destructive shutdown helper used only by the Windows installer.
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod shutdown {
    use std::{
        collections::BTreeMap,
        ffi::c_void,
        fs,
        io::Write,
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
    };
    type Handle = *mut c_void;
    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(Handle, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(window: Handle, pid: *mut u32) -> u32;
        fn GetClassNameW(window: Handle, name: *mut u16, count: i32) -> i32;
        fn PostMessageW(window: Handle, message: u32, w: usize, l: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn CompareStringOrdinal(
            a: *const u16,
            alen: i32,
            b: *const u16,
            blen: i32,
            ignore_case: i32,
        ) -> i32;
    }
    struct Process(Handle);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    struct State {
        target: Vec<u16>,
        processes: BTreeMap<u32, (Process, Vec<Handle>)>,
    }
    fn normalized(path: &Path) -> std::io::Result<Vec<u16>> {
        Ok(fs::canonicalize(path)?.as_os_str().encode_wide().collect())
    }
    unsafe extern "system" fn visit(window: Handle, data: isize) -> i32 {
        let state = &mut *(data as *mut State);
        let mut class = [0u16; 256];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if length <= 0 || String::from_utf16_lossy(&class[..length as usize]) != "Window Class" {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if let Some((_, windows)) = state.processes.get_mut(&pid) {
            windows.push(window);
            return 1;
        }
        let raw = OpenProcess(0x0010_0000 | 0x1000, 0, pid); // SYNCHRONIZE | QUERY_LIMITED_INFORMATION
        if raw.is_null() {
            return 1;
        }
        let process = Process(raw);
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        if QueryFullProcessImageNameW(raw, 0, image.as_mut_ptr(), &mut length) == 0 {
            return 1;
        }
        use std::os::windows::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]));
        let Ok(image) = normalized(&path) else {
            return 1;
        };
        if CompareStringOrdinal(
            image.as_ptr(),
            image.len() as i32,
            state.target.as_ptr(),
            state.target.len() as i32,
            1,
        ) == 2
        {
            state.processes.insert(pid, (process, vec![window]));
        }
        1
    }
    pub fn run(install: &Path, log: &mut fs::File) -> Result<(), String> {
        let target = install.join("literust-browser.exe");
        if !target.exists() {
            writeln!(log, "No existing installation").ok();
            return Ok(());
        }
        let mut state = State {
            target: normalized(&target).map_err(|e| e.to_string())?,
            processes: BTreeMap::new(),
        };
        if unsafe { EnumWindows(visit, &mut state as *mut State as isize) } == 0 {
            return Err(format!(
                "Window enumeration failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        for (pid, (process, windows)) in &state.processes {
            writeln!(
                log,
                "Closing process {pid}; browser windows={}",
                windows.len()
            )
            .ok();
            for window in windows {
                let mut owner = 0;
                unsafe {
                    GetWindowThreadProcessId(*window, &mut owner);
                }
                if owner != *pid || unsafe { WaitForSingleObject(process.0, 0) } == 0 {
                    continue;
                }
                if unsafe { PostMessageW(*window, 0x0010, 0, 0) } == 0
                    && unsafe { WaitForSingleObject(process.0, 0) } != 0
                {
                    return Err(format!(
                        "Cannot request close: {}",
                        std::io::Error::last_os_error()
                    ));
                }
            }
        }
        for (pid, (process, _)) in &state.processes {
            match unsafe { WaitForSingleObject(process.0, 30000) } {
                0 => {
                    writeln!(log, "Process {pid} exited").ok();
                }
                258 => return Err("Graceful shutdown timed out; browser left intact".into()),
                _ => {
                    return Err(format!(
                        "Cannot wait for shutdown: {}",
                        std::io::Error::last_os_error()
                    ))
                }
            }
        }
        writeln!(log, "Graceful shutdown complete").ok();
        Ok(())
    }
    pub fn main() -> i32 {
        let args: Vec<_> = std::env::args_os().collect();
        if args.len() != 3 {
            return 2;
        }
        let log_path = Path::new(&args[2]);
        if let Some(parent) = log_path.parent() {
            if fs::create_dir_all(parent).is_err() {
                return 2;
            }
        }
        let Ok(mut log) = fs::File::create(log_path) else {
            return 2;
        };
        writeln!(log, "Starting native graceful upgrade shutdown").ok();
        match run(Path::new(&args[1]), &mut log) {
            Ok(()) => 0,
            Err(error) => {
                writeln!(log, "{error}").ok();
                1
            }
        }
    }
}
#[cfg(windows)]
fn main() {
    std::process::exit(shutdown::main());
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This installer helper requires Windows.");
    std::process::exit(2);
}
