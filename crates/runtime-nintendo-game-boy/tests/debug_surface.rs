//! Game Boy SM83 debug-surface coverage — the shared `DebugTarget` verbs wired
//! via `impl_sm83_debug_primitives!` and exposed through `debug_target_hooks!`.

mod common;

use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

use common::loop_rom;

fn loaded_runtime() -> GameBoyRuntime {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge loads");
    runtime
}

#[test]
fn debug_target_present_once_a_cartridge_is_loaded() {
    let blank = GameBoyRuntime::blank(Model::Dmg);
    assert!(blank.debug_target().is_none(), "no target before a machine");
    let loaded = loaded_runtime();
    assert!(loaded.debug_target().is_some(), "target after load");
}

#[test]
fn peek_and_disassemble_read_the_cartridge() {
    let runtime = loaded_runtime();
    let dbg = runtime.debug_target().expect("target");
    // `loop_rom` places `JR -2` (0x18 0xFE) at $0100.
    assert_eq!(dbg.peek(0x0100), 0x18);
    assert_eq!(dbg.peek(0x0101), 0xFE);
    // It disassembles to a relative jump back to itself.
    assert_eq!(dbg.disassemble(0x0100), Some(("JR $0100".to_string(), 2)));
}

#[test]
fn poke_writes_through_to_wram() {
    let mut runtime = loaded_runtime();
    runtime
        .debug_target_mut()
        .expect("target")
        .poke(0xC000, 0xAB);
    assert_eq!(runtime.debug_target().expect("target").peek(0xC000), 0xAB);
}

#[test]
fn cpu_state_reports_the_sm83_register_set() {
    let runtime = loaded_runtime();
    let state = runtime.debug_target().expect("target").cpu_state();
    for key in ["af", "bc", "de", "hl", "sp", "pc", "flags", "ime", "halt"] {
        assert!(state.get(key).is_some(), "cpu_state should include {key}");
    }
    let flags = state.get("flags").expect("flags");
    for f in ["z", "n", "h", "c"] {
        assert!(flags.get(f).is_some(), "flags should include {f}");
    }
}

#[test]
fn step_advances_by_whole_m_cycles() {
    let mut runtime = loaded_runtime();
    let ticks = runtime
        .debug_target_mut()
        .expect("target")
        .step_instruction();
    assert!(ticks > 0, "stepping an instruction consumes cycles");
    assert_eq!(ticks % 4, 0, "T-cycles come in whole m-cycles");
}

#[test]
fn io_trace_unsupported_for_memory_mapped_io() {
    let runtime = loaded_runtime();
    assert!(!runtime.debug_target().expect("target").supports_io_trace());
}
