//! Folded chip-query paths for the MSX runtime (#456 step 2).
//!
//! The bespoke `query_vdp` / `query_psg` / `query_ppi` MCP tools were folded
//! into the generic `query` surface as paths. Each chip exposes a grouped
//! object path plus fine-grained leaves, and the AY-3-8910 (MSX "PSG") uses
//! the canonical `ay.*` namespace shared with the Spectrum. A 32 KiB
//! synthetic BIOS makes the machine debuggable without an external ROM.

use emu198x_shell::SessionQueryProvider;
use runtime_msx::{Model, MsxRuntime, MsxSessionQueryProvider};

fn loaded() -> MsxRuntime {
    MsxRuntime::new(Model::Msx1Ntsc, vec![0u8; 32 * 1024]).expect("32 KiB BIOS builds the machine")
}

fn resolve(rt: &MsxRuntime, path: &str) -> serde_json::Value {
    MsxSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn vdp_resolves_grouped_object_and_matching_leaf() {
    let rt = loaded();
    let vdp = resolve(&rt, "vdp");
    assert!(vdp.get("scanline").is_some(), "grouped vdp has scanline");
    assert!(vdp.get("framebuffer_width").is_some());
    assert!(vdp.get("framebuffer_height").is_some());
    // The leaf returns the same value the grouped object carries.
    assert_eq!(resolve(&rt, "vdp.scanline"), vdp["scanline"]);
}

#[test]
fn psg_is_exposed_under_the_canonical_ay_namespace() {
    let rt = loaded();
    let ay = resolve(&rt, "ay");
    assert!(ay.get("selected_register").is_some());
    assert!(ay["registers"].is_array(), "ay.registers is the hex file");
    assert_eq!(resolve(&rt, "ay.registers"), ay["registers"]);

    // The old `psg` / `query_psg` names are gone — only `ay.*` resolves.
    assert!(
        MsxSessionQueryProvider
            .query(&rt, "psg")
            .expect("query does not error")
            .is_none(),
        "the PSG is reached via ay.*, not psg"
    );
}

#[test]
fn ppi_resolves_grouped_object_and_leaves() {
    let rt = loaded();
    let ppi = resolve(&rt, "ppi");
    assert!(ppi.get("port_a").is_some());
    assert!(ppi.get("keyboard_row").is_some());
    assert_eq!(resolve(&rt, "ppi.keyboard_row"), ppi["keyboard_row"]);
}

#[test]
fn query_paths_lists_the_folded_chip_paths() {
    let paths = MsxSessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "vdp",
        "vdp.scanline",
        "vdp.framebuffer_width",
        "ay",
        "ay.registers",
        "ay.selected_register",
        "ppi",
        "ppi.port_a",
        "ppi.keyboard_row",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
