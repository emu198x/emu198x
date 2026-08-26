//! The Z80 runs at the rate this machine's own frame budget specifies.
//!
//! `Z80::tick` advances one half-cycle. A Jupiter Ace machine T-state must
//! therefore call it twice while advancing the display and master clock once.
//! A uniform clock error can survive boot and framebuffer tests, so compare a
//! steady stream of the simplest instruction with its architectural cost.

use emu198x_zilog_z80::Z80Stepper;
use machine_jupiter_ace::JupiterAce;

/// Zilog `NOP`: one M1 fetch, four T-states.
const NOP_TSTATES: u64 = 4;

#[test]
fn a_nop_costs_four_tstates() {
    let mut ace = JupiterAce::new(vec![0x00; 8 * 1024], 1024).expect("build machine");

    // Clear reset sequencing before measuring the steady-state NOP stream.
    for _ in 0..64 {
        ace.step_tick();
    }

    let tstates_before = ace.master_clock();
    let retired_before = ace.z80_instructions_retired();
    for _ in 0..4_000 {
        ace.step_tick();
    }
    let tstates = ace.master_clock() - tstates_before;
    let retired = ace.z80_instructions_retired() - retired_before;

    assert!(
        retired > 100,
        "expected the CPU to retire a useful number of instructions, got {retired}"
    );
    assert_eq!(
        tstates,
        retired * NOP_TSTATES,
        "{retired} NOPs cost {tstates} T-states, not {}",
        retired * NOP_TSTATES,
    );
}
