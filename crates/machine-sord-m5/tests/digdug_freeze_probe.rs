//! Diagnostic + reproducer for the Sord M5 Dig Dug round-freeze.
//!
//! After the "1" key starts the game (title → ROUND 01), the round never
//! progresses: the player and enemies never spawn, the score stays 00. This
//! test boots the cart, presses "1", and records what *is* and *isn't* moving,
//! to localise the freeze. Findings as of 2026-06-05:
//!
//! - **Game logic runs.** Work RAM ($7000-$7FFF) keeps mutating; the CPU
//!   executes both BIOS and cart code (not halted, not a tight spin).
//! - **Display + sprites are enabled** (VDP reg1 bit6/bit1 set) and the sprite
//!   attribute table base (reg5) matches what the VDP reads — yet every sprite
//!   stays parked at (Y=194, X=0, pattern=0) forever. The game never writes
//!   sprite positions.
//! - **The game reads no input at all** in the round — only VRAM ($10/$11) and
//!   a little PSG ($20). A live Dig Dug must poll the joystick, so it is stuck
//!   in a pre-play state, never reaching its input/spawn code.
//! - **Interrupt delivery is not the cause.** IM2 acks run ~16/frame
//!   identically at boot, on the (working) title screen, and in the stuck round.
//!
//! Not the keyboard (the "1" key demonstrably starts the round) nor the
//! joystick (the JOY byte is bit-exact, verified separately). The freeze is in
//! the cart's round-init state machine — next step is reverse-engineering what
//! it waits on (hot cart code reads `$754A`), ideally against a MAME execution
//! trace of the same ROM.
//!
//! Gated `#[ignore]`. Run with:
//! ```text
//! EMU198X_SORD_M5_BIOS=/path/sord-m5.rom EMU198X_SORD_M5_CART=/path/digdug.bin \
//!   cargo test --release -p machine-sord-m5 \
//!     --test digdug_freeze_probe -- --ignored --nocapture
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;

use machine_sord_m5::{M5Region, SordM5};

/// Sum of M5 work RAM ($7000-$7FFF) — a cheap fingerprint of game state.
fn ram_fingerprint(sys: &SordM5) -> u32 {
    (0x7000u32..=0x7FFF)
        .map(|a| u32::from(sys.peek(a as u16)))
        .sum()
}

/// First `n` sprites' (Y, X, pattern) from the VDP sprite-attribute table.
fn sprite_snapshot(sys: &SordM5, n: usize) -> Vec<(u8, u8, u8)> {
    let vdp = sys.vdp();
    let base = vdp.sprite_attr_table_addr();
    let vram = vdp.vram();
    (0..n)
        .map(|i| {
            let o = base + i * 4;
            (vram[o], vram[o + 1], vram[o + 2])
        })
        .collect()
}

#[test]
#[ignore = "needs Sord M5 BIOS + Dig Dug cart — run with --ignored"]
fn probe_digdug_round_freeze() {
    let bios = env::var("EMU198X_SORD_M5_BIOS")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .expect("set EMU198X_SORD_M5_BIOS");
    let cart = env::var("EMU198X_SORD_M5_CART")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .expect("set EMU198X_SORD_M5_CART to the Dig Dug cart");

    let mut sys = SordM5::new(bios, cart, M5Region::Ntsc);

    // Boot to the title screen, then start a 1-player game with the "1" key.
    for _ in 0..400 {
        sys.run_frame();
    }
    let boot_acks = sys.irq_acks();
    sys.press_key(1, 0); // "1" = Y1 bit 0
    for _ in 0..6 {
        sys.run_frame();
    }
    sys.release_key(1, 0);
    for _ in 0..200 {
        sys.run_frame();
    }

    // IM2 interrupt rate — same at boot and in the stuck round means delivery
    // is not the cause.
    let pre = sys.irq_acks();
    for _ in 0..60 {
        sys.run_frame();
    }
    println!(
        "IM2 acks: ~{}/frame at boot, ~{}/frame in the stuck round",
        boot_acks / 400,
        (sys.irq_acks() - pre) / 60
    );

    // VDP/sprite state: display + sprites enabled, SAT base agrees, sprites
    // parked.
    let regs = *sys.vdp().registers();
    println!(
        "VDP reg1=0x{:02X} (bit6 display, bit1 16x16), SAT base reg5*0x80=0x{:04X} == vdp 0x{:04X}",
        regs[1],
        usize::from(regs[5]) * 0x80,
        sys.vdp().sprite_attr_table_addr(),
    );
    println!("first sprites (Y,X,pat): {:?}", sprite_snapshot(&sys, 4));

    // Is the round logic running but the sprites frozen? Sample over 2000 frames.
    let fp0 = ram_fingerprint(&sys);
    let mut sprites_ever_placed = false;
    for _ in 0..2000 {
        sys.run_frame();
        if sprite_snapshot(&sys, 4)
            .iter()
            .any(|&(y, x, _)| y != 194 || x != 0)
        {
            sprites_ever_placed = true;
        }
    }
    println!(
        "over 2000 round frames: ram changed={}, any sprite ever placed={}",
        ram_fingerprint(&sys) != fp0,
        sprites_ever_placed
    );

    // What I/O does the round touch? (No reads = never reaches input/spawn code.)
    sys.start_io_trace();
    for _ in 0..4 {
        sys.run_frame();
    }
    let events = sys.take_io_trace();
    let mut reads: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
    let mut writes: BTreeMap<u8, u32> = BTreeMap::new();
    for ev in &events {
        if ev.write {
            *writes.entry(ev.port).or_insert(0) += 1;
        } else {
            reads.entry(ev.port).or_default().insert(ev.value);
        }
    }
    println!("round I/O over 4 frames: reads={reads:02X?} writes={writes:02X?}");

    // The game runs (RAM mutates) but never places a sprite — the signature of
    // a round-init that stalls before spawning.
    assert!(
        ram_fingerprint(&sys) != fp0 || !sprites_ever_placed,
        "probe sanity: expected either live RAM or parked sprites"
    );
}
