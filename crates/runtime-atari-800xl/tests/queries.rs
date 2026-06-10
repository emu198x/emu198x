//! Folded chip-register query paths for the Atari 800XL (#456 step 2).
//!
//! The bespoke `query_antic` / `query_gtia` / `query_pokey` / `query_pia`
//! MCP tools became grouped objects (`antic`, …) plus per-register leaves on
//! the generic `query` surface. A synthetic 16 KiB OS ROM makes the machine
//! debuggable without external firmware.

use emu198x_shell::SessionQueryProvider;
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};

fn loaded() -> Atari800xlRuntime {
    Atari800xlRuntime::new(Model::A800xlNtsc, Some(vec![0u8; 16384]), None, None, false)
        .expect("synthetic OS ROM builds the machine")
}

fn resolve(rt: &Atari800xlRuntime, path: &str) -> serde_json::Value {
    Atari800xlSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn chip_registers_resolve_grouped_and_as_leaves() {
    let rt = loaded();

    let antic = resolve(&rt, "antic");
    assert!(antic.get("dmactl").is_some());
    assert!(antic.get("scan_line").is_some());
    assert_eq!(resolve(&rt, "antic.dmactl"), antic["dmactl"]);

    // GTIA carries array fields (colpf / colpm); the leaf returns the array.
    let gtia = resolve(&rt, "gtia");
    assert!(gtia["colpf"].is_array());
    assert_eq!(resolve(&rt, "gtia.colpf"), gtia["colpf"]);

    assert!(resolve(&rt, "pokey").get("irqen").is_some());
    assert_eq!(
        resolve(&rt, "pokey.audctl"),
        resolve(&rt, "pokey")["audctl"]
    );

    assert!(resolve(&rt, "pia").get("portb").is_some());
}

#[test]
fn unknown_chip_leaf_returns_none_not_null() {
    let rt = loaded();
    assert!(
        Atari800xlSessionQueryProvider
            .query(&rt, "antic.bogus")
            .expect("query does not error")
            .is_none(),
        "an unknown chip sub-field is an unknown path, not a null value"
    );
}

#[test]
fn query_paths_lists_the_folded_chip_paths() {
    let paths = Atari800xlSessionQueryProvider.query_paths(&loaded(), None);
    for expected in [
        "antic",
        "antic.dmactl",
        "antic.scan_line",
        "gtia",
        "gtia.colpf",
        "pokey",
        "pokey.audctl",
        "pia",
        "pia.portb",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
}
