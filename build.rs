use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let version = env::var_os("RUSTC")
        .and_then(|rustc| Command::new(rustc).arg("--version").output().ok())
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    println!(
        "cargo:rustc-env=LITERUST_BUILD_RUST={}",
        version
            .as_deref()
            .and_then(|value| value.lines().next())
            .unwrap_or("unknown")
    );
}
