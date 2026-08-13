//! The Z80 runs at the rate the machine's own constants budget for.
//!
//! `Z80::tick` advances **one half-cycle** — `T1Rise`, then `T1Fall`.
//! `tick_tstate` used to call it once while incrementing `cpu_tstates` by
//! one, so every instruction cost twice the T-states it should and the
//! machine executed half the work per frame that
//! `CPU_TSTATES_PER_SCANLINE` budgets for.
//!
//! Nothing caught it. The SMS suite passed throughout: boot tests reach
//! their screens either way, and a uniform halving of CPU speed is
//! invisible to anything that does not compare an instruction's cost
//! against a known figure. This test compares against a known figure.
//!
//! `NOP` is the right probe because its cost is the least disputed number
//! in the instruction set and it involves no memory operand, no index
//! prefix and no conditional path — if `NOP` is right the clock division
//! is right, and if it is wrong nothing else can be.

use machine_sega_master_system::{Sms, SmsVariant};
use zilog_z80::Z80Stepper;

/// Zilog `NOP`: one `M1` fetch, four T-states.
const NOP_TSTATES: u64 = 4;

#[test]
fn a_nop_costs_four_tstates() {
    // 32 KB of `NOP`; the Z80 starts executing at `$0000`.
    let rom = vec![0x00u8; 32 * 1024];
    let mut sms = Sms::new(rom, SmsVariant::SmsNtsc);

    // Clear the reset sequence so the measurement covers steady-state
    // `NOP`s rather than whatever the first instruction happens to be.
    for _ in 0..64 {
        sms.step_tick();
    }

    let tstates_before = sms.cpu_tstates();
    let retired_before = sms.z80_instructions_retired();
    for _ in 0..4_000 {
        sms.step_tick();
    }
    let tstates = sms.cpu_tstates() - tstates_before;
    let retired = sms.z80_instructions_retired() - retired_before;

    assert!(
        retired > 100,
        "expected the CPU to retire a useful number of instructions, got {retired}"
    );
    assert_eq!(
        tstates,
        retired * NOP_TSTATES,
        "{retired} `NOP`s cost {tstates} T-states, not {}. A ratio of \
         exactly 2 means `Z80::tick` — which advances one \
         half-cycle — is being called once per T-state instead of twice.",
        retired * NOP_TSTATES,
    );
}
