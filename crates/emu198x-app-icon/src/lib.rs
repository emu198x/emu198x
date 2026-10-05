//! The Emu198x application icon, for build scripts.
//!
//! Every emulator binary's `build.rs` calls [`embed`], which compiles
//! `emu198x.ico` into the executable as a Windows resource so Explorer, the
//! Start menu and shortcuts show the plate. On any other target it does
//! nothing, so macOS and Linux builds need no resource compiler.
//!
//! The artwork and how to regenerate it are in `art/`; the window icons the running
//! emulator sets are in `emu198x-ui`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

/// The multi-size icon (16 and 24px small drawing, 32-256px full plate).
const ICO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/emu198x.ico");

/// Embed the application icon in this package's binaries when building for
/// Windows. Call from `build.rs`.
///
/// A missing resource compiler (a cross build without `llvm-rc`) is a
/// warning: the binary builds without a file icon. A resource compiler that
/// fails stops the build, because that is a broken resource, not a missing
/// tool.
pub fn embed() {
    println!("cargo:rerun-if-changed=build.rs");
    if !targets_windows(env::var("CARGO_CFG_TARGET_OS").ok().as_deref()) {
        return;
    }
    println!("cargo:rerun-if-changed={ICO}");
    if let Err(err) = compile_resource(Path::new(ICO)) {
        eprintln!("emu198x-app-icon: {err}");
        process::exit(1);
    }
}

fn targets_windows(target_os: Option<&str>) -> bool {
    target_os == Some("windows")
}

fn compile_resource(ico: &Path) -> Result<(), String> {
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or("OUT_DIR is not set; call embed() from a build script")?;
    let rc = out_dir.join("emu198x-app-icon.rc");
    fs::write(&rc, rc_source(ico)).map_err(|err| format!("writing {}: {err}", rc.display()))?;
    match embed_resource::compile(&rc, embed_resource::NONE) {
        embed_resource::CompilationResult::NotAttempted(why) => {
            println!("cargo:warning=Windows file icon not embedded: {why}");
            Ok(())
        }
        result => result.manifest_optional().map_err(|err| err.to_string()),
    }
}

/// A resource script naming the icon as resource 1, the one Explorer shows.
fn rc_source(ico: &Path) -> String {
    let mut quoted = String::new();
    for ch in ico.display().to_string().chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\"\""),
            _ => quoted.push(ch),
        }
    }
    let mut source = String::new();
    let _ = writeln!(source, "1 ICON \"{quoted}\"");
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_targets_embed() {
        assert!(targets_windows(Some("windows")));
        for other in [Some("macos"), Some("linux"), None] {
            assert!(!targets_windows(other));
        }
    }

    #[test]
    fn resource_script_escapes_windows_paths() {
        assert_eq!(
            rc_source(Path::new(r"C:\a\emu198x.ico")),
            "1 ICON \"C:\\\\a\\\\emu198x.ico\"\n"
        );
    }

    #[test]
    fn icon_carries_every_size() {
        let bytes = fs::read(ICO).expect("icon file present");
        // ICONDIR: reserved 0, type 1 (icon), entry count.
        assert_eq!(bytes[..4], [0, 0, 1, 0]);
        let count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        let sizes: Vec<u32> = (0..count)
            .map(|i| match bytes[6 + i * 16] {
                0 => 256,
                px => u32::from(px),
            })
            .collect();
        assert_eq!(sizes, [16, 24, 32, 48, 64, 128, 256]);
    }
}
