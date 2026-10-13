//! Debug stepping must preserve the same frame boundaries as normal execution.
//! A synthetic ROM makes this regression independent of firmware availability.

use emu198x_shell::{DebugTarget, HeadlessSession, MachineCore};
use runtime_sinclair_zx_spectrum::{
    Spectrum48kRuntime, SpectrumLiveAccess, SpectrumMachine, SpectrumRuntimeKind,
};

fn runtime() -> Spectrum48kRuntime {
    let mut rom = [0; 16 * 1024];
    // DI; LD A,4; OUT ($FE),A; JR $0005. Green is produced by execution.
    rom[..7].copy_from_slice(&[0xF3, 0x3E, 4, 0xD3, 0xFE, 0x18, 0xFE]);
    Spectrum48kRuntime::new_48k(rom)
}

#[test]
fn instruction_steps_wrap_frames_like_continuous_execution() {
    let mut stepped = runtime();
    let mut continuous = runtime();
    let mut elapsed = 0;
    let frame_ticks = stepped.machine().frame_halfcycles();
    while elapsed < frame_ticks * 3 + 101 {
        elapsed += SpectrumMachine::step_instructions(stepped.machine_mut(), 1);
    }
    continuous.machine_mut().advance_halfcycles(elapsed);
    assert_eq!(stepped.machine().hc(), continuous.machine().hc());
    assert_eq!(
        stepped.machine().framebuffer(),
        continuous.machine().framebuffer()
    );
    assert_eq!(
        stepped.machine().z80_registers().pc,
        continuous.machine().z80_registers().pc
    );
}

#[test]
fn live_steps_and_pc_waits_account_for_consumed_time() {
    let mut stepped = runtime();
    let ticks = stepped.step_instructions(3);
    assert!(ticks > 0);
    assert_eq!(stepped.time().get(), u64::from(ticks));
    let (reached, waited, _) = stepped.run_until_pc(5, 512);
    assert!(reached);
    assert!(waited > 0);
    assert_eq!(stepped.time().get(), u64::from(ticks + waited));
}

#[test]
fn frame_execution_after_debug_steps_settles_the_border_and_keeps_time() {
    let runtime = runtime();
    let frame_ticks = u64::from(runtime.machine().frame_halfcycles());
    let mut session = HeadlessSession::new(SpectrumRuntimeKind::Spectrum48K(runtime), frame_ticks);
    let mut elapsed = 0;
    while elapsed < frame_ticks * 3 + 101 {
        elapsed += session.machine_mut().step_instruction_no_resync();
    }
    session.machine_mut().resync();
    assert_eq!(session.time().get(), elapsed);
    session.run_frames(2).expect("resume frame execution");
    let SpectrumRuntimeKind::Spectrum48K(runtime) = session.machine() else {
        panic!("48K fixture");
    };
    // run_until rounds a target up to a complete frame. Its reported time
    // must count only the ticks actually consumed from a partial frame.
    assert_eq!(runtime.machine().hc(), 0);
    assert_eq!(session.time().get() % frame_ticks, 0);
    assert!(session.time().get() >= elapsed + frame_ticks * 2);

    let mut normal = HeadlessSession::new(
        SpectrumRuntimeKind::Spectrum48K(self::runtime()),
        frame_ticks,
    );
    normal.run_frames(3).expect("ordinary frames");
    let captured = session.latest_frame().expect("resumed capture");
    let expected = normal.latest_frame().expect("normal capture");
    assert_eq!(captured.pixels, expected.pixels);
    assert!(captured.pixels.contains(&4), "the ROM must paint green");
}
