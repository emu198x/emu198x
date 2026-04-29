//! Query-provider coverage for the Amiga runtime — boot status,
//! catalogued paths, and A1000-bootstrap-specific surfaces.

mod common;

use emu198x_shell::SessionQueryProvider;
use runtime_commodore_amiga::{AmigaRuntime, AmigaSessionQueryProvider, Model};
use serde_json::json;

use common::{dummy_a1000_bootstrap_rom, dummy_kickstart};

#[test]
fn query_provider_returns_declared_paths() {
    let runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let provider = AmigaSessionQueryProvider;
    let paths = provider.query_paths(&runtime, None);
    assert!(paths.contains(&"amiga.a1000.boot_rom_visible".to_owned()));
    assert!(paths.contains(&"amiga.a1000.wom_locked".to_owned()));
    assert!(paths.contains(&"amiga.cpu.pc".to_owned()));
    assert!(paths.contains(&"amiga.debug.dsk_write_count".to_owned()));
    assert!(paths.contains(&"amiga.disk.change_pending".to_owned()));
    assert!(paths.contains(&"amiga.disk.inserted".to_owned()));
    assert!(paths.contains(&"amiga.disk.step_events".to_owned()));
    assert!(paths.contains(&"amiga.keyboard.state".to_owned()));
}

#[test]
fn query_cpu_pc_returns_initial_reset_vector() {
    let runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let result = AmigaSessionQueryProvider
        .query(&runtime, "amiga.cpu.pc")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(result.path, "amiga.cpu.pc");
    assert_eq!(result.value, json!(0x00F8_0008u32));
}

#[test]
fn a1000_queries_report_bootstrap_state() {
    let runtime = AmigaRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
        .expect("runtime init");
    let boot_rom_visible = AmigaSessionQueryProvider
        .query(&runtime, "amiga.a1000.boot_rom_visible")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(boot_rom_visible.value, json!(true));

    let wom_locked = AmigaSessionQueryProvider
        .query(&runtime, "amiga.a1000.wom_locked")
        .expect("query succeeds")
        .expect("path present");
    assert_eq!(wom_locked.value, json!(false));
}
