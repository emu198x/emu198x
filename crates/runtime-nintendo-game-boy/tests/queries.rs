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
        provider.query_paths(&runtime, Some("gameboy.")),
        vec![
            "gameboy.cartridge.loaded".to_string(),
            "gameboy.cpu.pc".to_string()
        ]
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
            .query(&runtime, "gameboy.cartridge.loaded")
            .expect("gameboy.cartridge.loaded query should not fail")
            .expect("gameboy.cartridge.loaded should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "gameboy.cpu.pc")
            .expect("gameboy.cpu.pc query should not fail")
            .expect("gameboy.cpu.pc should resolve")
            .value,
        json!(0x0100u16)
    );
}

#[test]
fn headless_session_exposes_gameboy_queries() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    let session =
        HeadlessSession::new_with_query_provider(runtime, 1, GameBoySessionQueryProvider);
    let paths = session.query_paths(Some("gameboy."));
    assert_eq!(
        paths.paths,
        vec![
            "gameboy.cartridge.loaded".to_string(),
            "gameboy.cpu.pc".to_string()
        ]
    );
}
