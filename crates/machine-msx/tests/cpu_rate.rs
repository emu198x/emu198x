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

use emu198x_zilog_z80::Z80Stepper;
use machine_msx::{Msx, MsxRegion};

/// Zilog `NOP`: four CPU T-states plus the standard MSX M1 wait state.
const MSX_NOP_TSTATES: u64 = 5;

#[test]
fn a_nop_costs_five_tstates_on_msx() {
    // 32 KB of `NOP` in place of the BIOS; the Z80 starts at `$0000`.
    let mut msx = Msx::new(vec![0x00u8; 32 * 1024], MsxRegion::Pal);

    // Clear the reset sequence so the measurement covers steady-state `NOP`s
    // rather than whatever the first instruction happens to be.
    for _ in 0..64 {
        msx.step_tick();
    }

    let tstates_before = msx.cpu_tstates();
    let retired_before = msx.z80_instructions_retired();
    for _ in 0..4_000 {
        msx.step_tick();
    }
    let tstates = msx.cpu_tstates() - tstates_before;
    let retired = msx.z80_instructions_retired() - retired_before;

    assert!(
        retired > 100,
        "expected the CPU to retire a useful number of instructions, got {retired}"
    );
    assert_eq!(
        tstates,
        retired * MSX_NOP_TSTATES,
        "{retired} `NOP`s cost {tstates} T-states, not {} including the \
         standard one-T-state MSX wait on every M1 fetch.",
        retired * MSX_NOP_TSTATES,
    );
}
