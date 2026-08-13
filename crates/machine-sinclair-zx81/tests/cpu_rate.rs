//! The Z80 runs at the rate this machine's own constants budget for.
//!
//! `Z80::tick` advances **one half-cycle** — `T1Rise`, then `T1Fall`. A tick
//! function that calls it once while counting a whole T-state therefore runs
//! the CPU at half speed: every instruction costs twice the T-states it
//! should, and the machine executes half the work per frame its own budget
//! allows. Nine of the workspace's Z80 machines did exactly that, this one
//! among them; the sweep is in
//! `knowledge/decisions/z80-validation-surface.md`.
//!
//! Nothing else here catches it. A *uniform* halving is invisible to boot
//! tests, which reach their screens either way, and to golden framebuffers,
//! which were captured under the same halving. Only comparing an
//! instruction's cost against a known figure catches it, and that is what
//! this test does.
//!
//! `NOP` is the right probe: the least disputed number in the instruction
//! set, with no memory operand, no index prefix and no conditional path. If
//! `NOP` is right the clock division is right, and if it is wrong nothing
//! else can be.

use machine_sinclair_zx81::Zx81;
use zilog_z80::Z80Stepper;

/// Zilog `NOP`: one `M1` fetch, four T-states.
const NOP_TSTATES: u64 = 4;

#[test]
fn a_nop_costs_four_tstates() {
    // 8 KB of `NOP` in place of the ZX81 ROM, 16 KB RAM. The NMI generator
    // is disabled out of reset, so nothing interrupts the `NOP` stream.
    let mut zx81 = Zx81::new(vec![0x00u8; 0x2000], 16 * 1024).expect("build machine");

    // Clear the reset sequence so the measurement covers steady-state `NOP`s
    // rather than whatever the first instruction happens to be.
    for _ in 0..64 {
        zx81.step_tick();
    }

    let tstates_before = zx81.master_clock();
    let retired_before = zx81.z80_instructions_retired();
    for _ in 0..4_000 {
        zx81.step_tick();
    }
    let tstates = zx81.master_clock() - tstates_before;
    let retired = zx81.z80_instructions_retired() - retired_before;

    assert!(
        retired > 100,
        "expected the CPU to retire a useful number of instructions, got {retired}"
    );
    assert_eq!(
        tstates,
        retired * NOP_TSTATES,
        "{retired} `NOP`s cost {tstates} T-states, not {}. A ratio of \
         exactly 2 means `Z80::tick` — which advances one half-cycle — is \
         being called once per T-state instead of twice.",
        retired * NOP_TSTATES,
    );
}
