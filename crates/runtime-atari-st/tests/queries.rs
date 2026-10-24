//! The Atari ST's query paths, grouped and leaf, on a synthetic TOS, so the
//! machine is debuggable without an external ROM.

mod common;

use emu198x_shell::{HeadlessSession, SessionQueryProvider};
use runtime_atari_st::{AtariStRuntime, AtariStSessionQueryProvider, Model, Monitor, blank};

fn loaded() -> AtariStRuntime {
    let mut runtime = AtariStRuntime::new(Model::St520Pal, common::tos()).expect("TOS");
    runtime.insert_disk(0, &[0u8; 737_280]).expect("720 KB");
    let mut session = HeadlessSession::new(runtime, Model::St520Pal.frame_ticks(Monitor::Colour));
    session.run_frames(1).expect("run");
    session.into_machine()
}

fn resolve(rt: &AtariStRuntime, path: &str) -> serde_json::Value {
    AtariStSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn video_and_floppy_resolve_grouped_and_leaves() {
    let rt = loaded();

    let video = resolve(&rt, "video");
    assert_eq!(video["frequency"], 50);
    assert_eq!(video["resolution"], "low");
    assert_eq!(resolve(&rt, "video.frequency"), video["frequency"]);
    assert_eq!(resolve(&rt, "video.base"), video["base"]);

    let floppy = resolve(&rt, "floppy");
    assert_eq!(floppy["a"], true);
    assert_eq!(floppy["b"], false);
    assert_eq!(resolve(&rt, "floppy.a"), true);

    assert_eq!(resolve(&rt, "machine.variant"), "520st-pal");
    assert_eq!(resolve(&rt, "machine.monitor"), "colour");
    assert_eq!(resolve(&rt, "tos.loaded"), true);
    // The synthetic image has a stack pointer where a TOS header has its
    // version word.
    assert_eq!(resolve(&rt, "tos.version"), "80.00");
    assert!(resolve(&rt, "machine.frame_count").as_u64().expect("count") >= 1);
    assert!(resolve(&rt, "cpu.pc").as_u64().expect("pc") >= 0xFC_0008);
}

#[test]
fn query_paths_lists_every_path_and_each_resolves() {
    let rt = loaded();
    let paths = AtariStSessionQueryProvider.query_paths(&rt, None);
    for path in &paths {
        resolve(&rt, path);
    }
    assert_eq!(
        AtariStSessionQueryProvider.query_paths(&rt, Some("floppy")),
        ["floppy", "floppy.a", "floppy.b"]
    );
}

#[test]
fn machine_paths_need_tos() {
    let rt = blank(Model::St520Pal);
    assert_eq!(resolve(&rt, "tos.loaded"), false);
    assert!(AtariStSessionQueryProvider.query(&rt, "cpu.pc").is_err());
    assert!(
        AtariStSessionQueryProvider
            .query(&rt, "tos.version")
            .is_err()
    );
}
