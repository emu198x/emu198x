//! Query-provider coverage for the Game Boy runtime.

mod common;

use emu198x_shell::{
    HeadlessSession, MachineCore, MediaImage, MediaKind, MediaSet, SessionQueryProvider,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, GameBoySessionQueryProvider, Model};
use serde_json::json;

use common::loop_rom;

#[test]
fn query_provider_lists_gameboy_paths() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    let provider = GameBoySessionQueryProvider;
    assert_eq!(
        provider.query_paths(&runtime, Some("")),
        vec!["cartridge.loaded".to_string(), "cpu.pc".to_string()]
    );
}

#[test]
fn query_provider_reports_loaded_state_and_cpu_pc() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    let provider = GameBoySessionQueryProvider;
    assert_eq!(
        provider
            .query(&runtime, "cartridge.loaded")
            .expect("cartridge.loaded query should not fail")
            .expect("cartridge.loaded should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "cpu.pc")
            .expect("cpu.pc query should not fail")
            .expect("cpu.pc should resolve")
            .value,
        json!(0x0100u16)
    );
}

#[test]
fn headless_session_exposes_gameboy_queries() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    let session = HeadlessSession::new_with_query_provider(runtime, 1, GameBoySessionQueryProvider);
    let paths = session.query_paths(None);
    // The session surfaces the machine's own paths alongside the shared
    // session/capture/run set; assert the machine paths are present (no
    // machine prefix to isolate them by any more).
    assert!(
        paths.paths.contains(&"cartridge.loaded".to_string()),
        "session exposes the cartridge.loaded query"
    );
    assert!(
        paths.paths.contains(&"cpu.pc".to_string()),
        "session exposes the cpu.pc query"
    );
}
