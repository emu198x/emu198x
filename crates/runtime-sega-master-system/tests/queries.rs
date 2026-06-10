//! Folded VDP + mapper query paths for the Master System (#456 step 2).
//!
//! The bespoke `query_vdp` / `query_mapper` MCP tools became `vdp` / `mapper`
//! (grouped) plus leaves on the generic `query` surface. A small synthetic
//! cartridge makes the machine debuggable without an external ROM.

use emu198x_shell::SessionQueryProvider;
use runtime_sega_master_system::{Model, SmsRuntime, SmsSessionQueryProvider};

fn loaded() -> SmsRuntime {
    SmsRuntime::new(Model::SmsNtsc, vec![0u8; 1024])
}

fn resolve(rt: &SmsRuntime, path: &str) -> serde_json::Value {
    SmsSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn vdp_and_mapper_resolve_grouped_and_leaves() {
    let rt = loaded();

    let vdp = resolve(&rt, "vdp");
    assert!(vdp.get("scanline").is_some());
    assert!(vdp.get("framebuffer_width").is_some());
    assert_eq!(resolve(&rt, "vdp.scanline"), vdp["scanline"]);

    let mapper = resolve(&rt, "mapper");
    assert!(mapper.get("control").is_some());
    assert!(mapper.get("page2").is_some());
    assert_eq!(resolve(&rt, "mapper.control"), mapper["control"]);
    assert_eq!(resolve(&rt, "mapper.page2"), mapper["page2"]);
}

#[test]
fn query_paths_lists_the_folded_paths() {
    let paths = SmsSessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "vdp",
        "vdp.framebuffer_width",
        "mapper",
        "mapper.control",
        "mapper.page0",
        "mapper.page2",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
