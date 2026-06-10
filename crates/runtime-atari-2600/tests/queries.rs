//! Folded TIA query paths for the Atari 2600 (#456 step 2).
//!
//! The bespoke `query_tia` MCP tool became `tia` (grouped) + leaves on the
//! generic `query` surface. A small synthetic cartridge makes the machine
//! debuggable without an external ROM.

use emu198x_shell::SessionQueryProvider;
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider, Model};

fn loaded() -> Atari2600Runtime {
    Atari2600Runtime::new(Model::Vcs2600Ntsc, vec![0u8; 4096]).expect("synthetic cart builds")
}

fn resolve(rt: &Atari2600Runtime, path: &str) -> serde_json::Value {
    Atari2600SessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn tia_resolves_grouped_object_and_leaves() {
    let rt = loaded();
    let tia = resolve(&rt, "tia");
    assert!(tia.get("hpos").is_some());
    assert!(tia.get("vpos").is_some());
    assert!(tia.get("framebuffer_width").is_some());
    assert_eq!(resolve(&rt, "tia.hpos"), tia["hpos"]);
    assert_eq!(
        resolve(&rt, "tia.framebuffer_width"),
        tia["framebuffer_width"]
    );
}

#[test]
fn query_paths_lists_the_folded_tia_paths() {
    let paths = Atari2600SessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "tia",
        "tia.hpos",
        "tia.vpos",
        "tia.framebuffer_width",
        "tia.framebuffer_height",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
