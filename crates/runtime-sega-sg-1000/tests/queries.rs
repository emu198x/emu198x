//! Folded TMS9918 VDP query paths for the SG-1000 (#456 step 2).
//!
//! The bespoke `query_vdp` MCP tool became `vdp` (grouped) + leaves on the
//! generic `query` surface. A small synthetic cartridge makes the machine
//! debuggable without an external ROM.

use emu198x_shell::SessionQueryProvider;
use runtime_sega_sg_1000::{Model, Sg1000Runtime, Sg1000SessionQueryProvider};

fn loaded() -> Sg1000Runtime {
    Sg1000Runtime::new(Model::Sg1000Ntsc, vec![0u8; 1024])
}

fn resolve(rt: &Sg1000Runtime, path: &str) -> serde_json::Value {
    Sg1000SessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn vdp_resolves_grouped_object_and_leaves() {
    let rt = loaded();
    let vdp = resolve(&rt, "vdp");
    assert!(vdp.get("scanline").is_some());
    assert!(vdp.get("framebuffer_width").is_some());
    assert!(vdp.get("framebuffer_height").is_some());
    assert_eq!(resolve(&rt, "vdp.scanline"), vdp["scanline"]);
    assert_eq!(
        resolve(&rt, "vdp.framebuffer_height"),
        vdp["framebuffer_height"]
    );
}

#[test]
fn query_paths_lists_the_folded_vdp_paths() {
    let paths = Sg1000SessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "vdp",
        "vdp.scanline",
        "vdp.framebuffer_width",
        "vdp.framebuffer_height",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
