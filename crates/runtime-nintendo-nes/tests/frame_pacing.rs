//! One scripted frame is one recorded frame.
//!
//! An NTSC field is 341 x 262 = 89,342 PPU dots, except that the
//! pre-render line of an odd frame is one dot short. The shell asks for
//! a nominal 89,342 per frame, so the machine lands a dot below the
//! target every other field — and a runtime that runs whole fields
//! until it reaches the target covers that single dot with an entire
//! second field, emitting a second frame with it.
//!
//! Nothing about that reads as broken from the outside: the run
//! succeeds, the frames are real, and the video is twice as long as the
//! script asked for at half the intended speed (#1179). These tests
//! count what the machine actually emitted.

use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{Model, NesRuntime};

const NES_FRAME_TICKS: u64 = 341 * 262;

/// 16 KiB of PRG that holds rendering on and spins.
///
/// Rendering has to be on: the dot an odd frame skips is skipped by the
/// *renderer*, so a cartridge that leaves PPUMASK alone runs every
/// field at the full 89,342 dots, never lands short of the nominal
/// target, and passes these tests with the bug still in place.
///
/// The write is inside the loop rather than done once at reset. The PPU
/// ignores writes during its warm-up, and a single `STA $2001` seven
/// cycles after reset lands inside that window and is lost -- measured,
/// not assumed: the one-shot version of this cartridge runs every field
/// at 89,342 dots.
///
/// ```text
/// reset:  LDA #$1E     ; background and sprites, no clipping
///         STA $2001    ; PPUMASK
///         JMP reset
/// ```
fn rendering_ines() -> Vec<u8> {
    let mut prg = vec![0xeau8; 16 * 1024];
    prg[0x0000..0x0008].copy_from_slice(&[
        0xa9, 0x1e, // LDA #$1E
        0x8d, 0x01, 0x20, // STA $2001
        0x4c, 0x00, 0x80, // JMP $8000
    ]);
    prg[0x3ffc] = 0x00;
    prg[0x3ffd] = 0x80;
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1; // 1 x 16 KiB PRG
    data[5] = 1; // 1 x 8 KiB CHR
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

fn booted_session() -> HeadlessSession<NesRuntime> {
    let cartridge = rendering_ines();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &cartridge,
    ));

    let mut session = HeadlessSession::new(NesRuntime::blank(Model::NesNtsc), NES_FRAME_TICKS);
    session
        .prepare(&media, &[])
        .expect("the minimal cartridge prepares");
    session
}

#[test]
fn a_hundred_scripted_frames_emit_a_hundred_frames() {
    let mut session = booted_session();
    session.run_frames(100).expect("a hundred frames run");

    assert_eq!(
        session.frames_emitted(),
        100,
        "run_frames(100) must emit a hundred frames; \
         the reported failure emitted 191"
    );
}

#[test]
fn the_short_field_does_not_buy_a_second_frame() {
    // Two frames is where the alternation first bites: whichever field
    // is short lands a dot below the nominal target.
    let mut session = booted_session();
    for expected in 1..=8 {
        session.run_frames(1).expect("one frame runs");
        assert_eq!(
            session.frames_emitted(),
            expected,
            "frame {expected} emitted more than one frame"
        );
    }
}

#[test]
fn a_run_of_frames_keeps_the_clock_on_the_nominal_grid() {
    // Undershooting by a dot per short field is the point of the fix,
    // so the deficit must stay a rounding error rather than accumulate:
    // each request computes its target from the actual time.
    let mut session = booted_session();
    session.run_frames(100).expect("a hundred frames run");

    let nominal = NES_FRAME_TICKS * 100;
    let actual = session.time().get();
    let drift = nominal.abs_diff(actual);
    assert!(
        drift < NES_FRAME_TICKS,
        "a hundred frames landed {drift} dots from the nominal {nominal}, \
         which is more than a whole field"
    );
}
