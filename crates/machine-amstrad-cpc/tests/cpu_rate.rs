//! The Z80 runs at the rate this machine's own constants budget for.
//!
//! `Z80::tick` advances **one half-cycle** — `T1Rise`, then `T1Fall`. A tick
//! function that calls it once while counting a whole T-state runs the CPU at
//! half speed. Nine of the workspace's Z80 machines did exactly that until the
//! 2026-08-13 sweep measured them; see
//! `knowledge/decisions/z80-validation-surface.md`.
//!
//! This gate exists from the CPC's first commit rather than being added after
//! the defect is found, because a *uniform* halving is invisible to everything
//! else: boot tests reach their screens either way, and goldens captured under
//! it look right.
//!
//! # Why 4 and not ~3.3 MHz worth
//!
//! The real CPC's Gate Array stretches every Z80 M-cycle to a multiple of four
//! T-states, so instructions cost more than Zilog's figures and the effective
//! rate is about 3.3 MHz — the official CPC464 firmware guide says so outright.
//! That is **not modelled yet**: `/WAIT` has no oracle among the vendored
//! emulators, none of which drives it as a pin. So this asserts the unstretched
//! Zilog figure, which is what the machine currently implements. When `/WAIT`
//! lands, this test *should* fail, and the new expected figure belongs here with
//! its derivation.

use machine_amstrad_cpc::AmstradCpc;
use zilog_z80::Z80Stepper;

/// Zilog `NOP`: one `M1` fetch, four T-states.
const NOP_TSTATES: u64 = 4;

#[test]
fn a_nop_costs_four_tstates() {
    // 16 KB of `NOP` as the OS ROM, so the Z80 runs a steady `NOP` stream from
    // `$0000`; the BASIC half is never reached.
    let firmware = vec![0x00u8; 0x8000];
    let mut cpc = AmstradCpc::new(&firmware).expect("build machine");

    // Clear the reset sequence so the measurement covers steady-state `NOP`s.
    for _ in 0..64 {
        cpc.step_tick();
    }

    let tstates_before = cpc.cpu_tstates();
    let retired_before = cpc.z80_instructions_retired();
    for _ in 0..4_000 {
        cpc.step_tick();
    }
    let tstates = cpc.cpu_tstates() - tstates_before;
    let retired = cpc.z80_instructions_retired() - retired_before;

    assert!(
        retired > 100,
        "expected the CPU to retire a useful number of instructions, got {retired}"
    );
    assert_eq!(
        tstates,
        retired * NOP_TSTATES,
        "{retired} `NOP`s cost {tstates} T-states, not {}. A ratio of exactly \
         2 means `Z80::tick` — which advances one half-cycle — is being called \
         once per T-state instead of twice.",
        retired * NOP_TSTATES,
    );
}
