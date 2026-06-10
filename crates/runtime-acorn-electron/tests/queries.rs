//! Folded ULA query paths for the Acorn Electron (#456 step 2).
//!
//! The bespoke `query_ula` MCP tool became `ula` (grouped) + leaves on the
//! generic `query` surface. Synthetic 16 KiB OS + BASIC ROMs make the
//! machine debuggable without external firmware.

use emu198x_shell::SessionQueryProvider;
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};

fn loaded() -> ElectronRuntime {
    ElectronRuntime::new(Model::Electron, vec![0u8; 16384], vec![0u8; 16384])
        .expect("synthetic OS + BASIC ROMs build the machine")
}

fn resolve(rt: &ElectronRuntime, path: &str) -> serde_json::Value {
    ElectronSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn ula_resolves_grouped_object_and_leaves() {
    let rt = loaded();
    let ula = resolve(&rt, "ula");
    assert!(ula.get("display_mode").is_some());
    assert!(ula.get("irq").is_some());
    assert!(ula.get("framebuffer_width").is_some());
    assert_eq!(resolve(&rt, "ula.display_mode"), ula["display_mode"]);
    assert_eq!(
        resolve(&rt, "ula.framebuffer_width"),
        ula["framebuffer_width"]
    );
}

#[test]
fn query_paths_lists_the_folded_ula_paths() {
    let paths = ElectronSessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "ula",
        "ula.display_mode",
        "ula.irq",
        "ula.framebuffer_width",
        "ula.framebuffer_height",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
