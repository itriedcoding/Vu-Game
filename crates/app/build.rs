//! Embeds the Volt icon, version information and application manifest into the
//! Windows executable.
//!
//! Resource compilation needs the MSVC resource compiler, so it only runs for
//! `*-pc-windows-msvc`. On that target the resources are not optional: the
//! manifest is what makes Windows grant the elevated token Volt needs for
//! machine-scope settings, so a missing or uncompilable resource fails the build
//! instead of silently producing an unelevated executable.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/volt.ico");
    println!("cargo:rerun-if-changed=assets/volt.manifest");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon = manifest_dir.join("../../assets/volt.ico");
    let manifest = manifest_dir.join("assets/volt.manifest");

    for (path, purpose) in [
        (&icon, "application icon"),
        (&manifest, "application manifest"),
    ] {
        assert!(
            path.is_file(),
            "The {purpose} is missing: {}",
            path.display()
        );
    }

    let icon_path = icon
        .to_str()
        .expect("assets/volt.ico path is not valid UTF-8");
    let manifest_path = manifest
        .to_str()
        .expect("crates/app/assets/volt.manifest path is not valid UTF-8");

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_path);
    resource.set_manifest_file(manifest_path);
    resource.set("FileDescription", "Volt - Roblox performance optimizer");
    resource.set("ProductName", "Volt");
    resource.set("OriginalFilename", "volt.exe");
    resource.set("LegalCopyright", "MIT licensed");

    if let Err(err) = resource.compile() {
        panic!("Windows resources could not be compiled: {err}");
    }
}
