//! Embeds the Volt icon and version information into the Windows executable.
//!
//! Resource compilation needs the MSVC resource compiler, so it only runs for
//! `*-pc-windows-msvc`. If it is unavailable the build still succeeds — the app
//! always sets its window icon from the same vector geometry at runtime.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/volt.ico");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon = manifest_dir.join("../../assets/volt.ico");
    if !icon.exists() {
        println!("cargo:warning=assets/volt.ico is missing; skipping the Windows icon resource.");
        return;
    }

    let Some(icon_path) = icon.to_str() else {
        println!(
            "cargo:warning=assets/volt.ico path is not valid UTF-8; skipping the icon resource."
        );
        return;
    };

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_path);
    resource.set("FileDescription", "Volt - Roblox performance optimizer");
    resource.set("ProductName", "Volt");
    resource.set("OriginalFilename", "volt.exe");
    resource.set("LegalCopyright", "MIT licensed");

    if let Err(err) = resource.compile() {
        println!("cargo:warning=Windows resources were not compiled: {err}");
    }
}
