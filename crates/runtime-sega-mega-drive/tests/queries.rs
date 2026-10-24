//! The Mega Drive's query paths, grouped and leaf, on a synthetic
//! cartridge, so the machine is debuggable without an external ROM.

mod common;

use emu198x_shell::{HeadlessSession, SessionQueryProvider};
use runtime_sega_mega_drive::{
    MegaDriveRuntime, MegaDriveSessionQueryProvider, Model, blank, with_cartridge,
};

fn loaded() -> MegaDriveRuntime {
    let runtime = with_cartridge(Model::Genesis, common::cartridge());
    let mut session = HeadlessSession::new(runtime, Model::Genesis.frame_ticks());
    session.run_frames(1).expect("run");
    session.into_machine()
}

fn resolve(rt: &MegaDriveRuntime, path: &str) -> serde_json::Value {
    MegaDriveSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn vdp_and_z80_resolve_grouped_and_leaves() {
    let rt = loaded();

    let vdp = resolve(&rt, "vdp");
    assert!(vdp.get("vcounter").is_some());
    assert_eq!(resolve(&rt, "vdp.frame_width"), vdp["frame_width"]);
    assert_eq!(resolve(&rt, "vdp.vcounter"), vdp["vcounter"]);

    let z80 = resolve(&rt, "z80");
    assert_eq!(z80["in_reset"], true, "a cartridge must release the Z80");
    assert_eq!(resolve(&rt, "z80.in_reset"), z80["in_reset"]);
    assert_eq!(resolve(&rt, "z80.pc"), z80["pc"]);

    assert_eq!(resolve(&rt, "machine.variant"), "genesis");
    assert_eq!(resolve(&rt, "cartridge.loaded"), true);
    assert!(resolve(&rt, "machine.frame_count").as_u64().expect("count") >= 1);
    assert!(resolve(&rt, "cpu.pc").as_u64().expect("pc") >= 0x200);
}

#[test]
fn query_paths_lists_every_path_and_each_resolves() {
    let rt = loaded();
    let paths = MegaDriveSessionQueryProvider.query_paths(&rt, None);
    for expected in ["vdp", "vdp.hslot", "z80", "z80.bus_acknowledged", "cpu.pc"] {
        assert!(
            paths.contains(&expected.to_string()),
            "query_paths is missing `{expected}`"
        );
    }
    for path in &paths {
        resolve(&rt, path);
    }
    assert_eq!(
        MegaDriveSessionQueryProvider.query_paths(&rt, Some("z80")),
        ["z80", "z80.bus_acknowledged", "z80.in_reset", "z80.pc"]
    );
}

#[test]
fn machine_paths_need_a_cartridge() {
    let rt = blank(Model::Genesis);
    assert_eq!(resolve(&rt, "cartridge.loaded"), false);
    assert!(MegaDriveSessionQueryProvider.query(&rt, "cpu.pc").is_err());
}
