//! The declared contention pattern must fall out of the mask the gate uses.
//!
//! Two constants describe the same behaviour, and only one of them runs:
//!
//! - `DELAY_TABLE_*` — a 16-entry half-cycle mask. Every ULA indexes this
//!   on every tick to decide whether to withhold the CPU clock. This is
//!   the behaviour.
//! - `CONTENTION_PATTERN_*` — an 8-entry per-T-state delay sequence on
//!   `FrameTiming`. `grep` finds no runtime consumer; it is read only by
//!   assertions. This is the documentation.
//!
//! Each was pinned against a hand-written duplicate of itself —
//! `boot_invariants.rs` for the masks, the ULA crates' unit tests for the
//! patterns — and nothing compared the two to each other. So both could
//! pass while the emulator implemented neither, which is what happened
//! (#856).
//!
//! The relationship is arithmetic, not a matter of opinion. A CPU
//! arriving at T-phase `p` is held for as long as the mask stays `true`
//! from that point, so the delay is the run length from `origin + 2p`,
//! halved. Deriving one constant from the other replaces two independent
//! copies with a single source of truth.

use common_sinclair_zx_spectrum::timing::{CONTENTION_PATTERN_48K, CONTENTION_PATTERN_PLUS2A};
use common_sinclair_zx_spectrum::ula_engine::{DELAY_TABLE_48K, DELAY_TABLE_PLUS2A};

/// T-state delays implied by a half-cycle mask, for a CPU whose T-phase 0
/// arrives at half-cycle `origin`.
fn delays_from_mask(mask: &[bool; 16], origin: usize) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (p, slot) in out.iter_mut().enumerate() {
        let start = (origin + 2 * p) % 16;
        let mut run = 0usize;
        while run < 16 && mask[(start + run) % 16] {
            run += 1;
        }
        // Two half-cycles to a T-state; a stall can only be spent whole.
        *slot = (run / 2) as u8;
    }
    out
}

/// Every phase alignment that reproduces `pattern` from `mask`.
fn origins_reproducing(mask: &[bool; 16], pattern: [u8; 8]) -> Vec<usize> {
    (0..16)
        .filter(|&o| delays_from_mask(mask, o) == pattern)
        .collect()
}

/// The alignment is 0, and that is the result rather than a detail.
///
/// It was 3 while the mask was a hand-written literal: T-phase 0 arrived
/// three half-cycles into the 16-pixel cycle, so the CPU's T-state grid
/// and the ULA's pixel counter were offset by one and a half T-states
/// with nothing to say why. Now that the mask is `C3 + C2` read on the
/// origin the ULA's fetch group fixes, the two grids share an origin and
/// the free run occupies two whole T-states.
#[test]
fn the_48k_pattern_falls_out_of_its_mask() {
    let origins = origins_reproducing(&DELAY_TABLE_48K, CONTENTION_PATTERN_48K);
    assert_eq!(
        origins,
        vec![0],
        "DELAY_TABLE_48K should reproduce {CONTENTION_PATTERN_48K:?} at exactly \
         one phase alignment (half-cycle 0), and did so at {origins:?}. \
         A second alignment would mean the mask is ambiguous; none would mean \
         the mask and the pattern have drifted apart.",
    );
}

/// The +2A/+3 mask cannot express its declared pattern at any alignment.
///
/// `[1, 0, 7, 6, 5, 4, 3, 2]` needs a run of 7 T-states — 14 of the 16
/// mask entries. `DELAY_TABLE_PLUS2A` has three `true`s (0, 14, 15), a run
/// of 3 half-cycles, so the longest stall it can produce is one T-state.
/// At origin 14 it yields `[1, 0, 0, 0, 0, 0, 0, 0]`: the first two
/// entries of the declared pattern, and nothing after them.
///
/// So +2A/+3 contention is effectively absent, and this is the assertion
/// that says so rather than a comment claiming otherwise. Ignored, not
/// deleted or weakened, until #856 establishes which of the two constants
/// is right — measured against FUSE, per
/// `knowledge/decisions/fuse-governs-the-contended-window.md`, rather than
/// against our own documentation.
#[test]
#[ignore = "KNOWN DIVERGENCE (#856): the +2A mask yields at most 1 T-state, \
            the declared pattern needs 7 — no alignment reconciles them"]
fn the_plus2a_pattern_falls_out_of_its_mask() {
    let origins = origins_reproducing(&DELAY_TABLE_PLUS2A, CONTENTION_PATTERN_PLUS2A);
    assert!(
        !origins.is_empty(),
        "DELAY_TABLE_PLUS2A reproduces {CONTENTION_PATTERN_PLUS2A:?} at no phase \
         alignment. At origin 14 it yields {:?}. One of the two constants is \
         wrong — see #856.",
        delays_from_mask(&DELAY_TABLE_PLUS2A, 14),
    );
}

#[test]
fn the_derivation_itself_is_sound() {
    // A mask with no contention can only ever mean no delay.
    assert_eq!(delays_from_mask(&[false; 16], 0), [0; 8]);
    // A fully contended mask saturates at the whole 8 T-state cycle.
    assert_eq!(delays_from_mask(&[true; 16], 0), [8; 8]);
    // The run wraps the 16-entry boundary rather than stopping at it:
    // trues at 15 and 0 are one run of two, worth a whole T-state.
    let mut wrapping = [false; 16];
    wrapping[15] = true;
    wrapping[0] = true;
    assert_eq!(delays_from_mask(&wrapping, 15)[0], 1);
}
