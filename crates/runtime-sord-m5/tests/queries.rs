//! Folded CTC + VDP query paths for the Sord M5 (#456 step 2).
//!
//! The bespoke `query_ctc` / `query_vdp` MCP tools became `ctc` / `vdp`
//! (grouped) plus leaves on the generic `query` surface. A small synthetic
//! ROM makes the machine debuggable without an external ROM.

use emu198x_shell::SessionQueryProvider;
use runtime_sord_m5::{M5Runtime, M5SessionQueryProvider, Model};

fn loaded() -> M5Runtime {
    M5Runtime::new(Model::M5Ntsc, vec![0u8; 1024])
}

fn resolve(rt: &M5Runtime, path: &str) -> serde_json::Value {
    M5SessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn ctc_resolves_grouped_with_channels_and_scalar_leaves() {
    let rt = loaded();
    let ctc = resolve(&rt, "ctc");
    assert!(ctc.get("vector_base").is_some());
    assert!(ctc.get("interrupt").is_some());
    let channels = ctc["channels"].as_array().expect("ctc has 4 channels");
    assert_eq!(channels.len(), 4);
    assert_eq!(resolve(&rt, "ctc.vector_base"), ctc["vector_base"]);
    assert_eq!(resolve(&rt, "ctc.interrupt"), ctc["interrupt"]);
}

#[test]
fn vdp_resolves_grouped_with_registers_and_leaves() {
    let rt = loaded();
    let vdp = resolve(&rt, "vdp");
    assert!(vdp["registers"].is_array(), "vdp.registers is the file");
    assert!(vdp.get("scanline").is_some());
    assert_eq!(resolve(&rt, "vdp.scanline"), vdp["scanline"]);
    assert_eq!(resolve(&rt, "vdp.registers"), vdp["registers"]);
}

#[test]
fn query_paths_lists_the_folded_paths() {
    let paths = M5SessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "ctc",
        "ctc.vector_base",
        "ctc.interrupt",
        "vdp",
        "vdp.scanline",
        "vdp.registers",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
