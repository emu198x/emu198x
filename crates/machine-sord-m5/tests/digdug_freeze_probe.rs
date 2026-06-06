//! Sord M5 Dig Dug round-boot regression tests (was: round-freeze diagnostic).
//!
//! **Resolved 2026-06-06.** After the "1" key started the game (title →
//! ROUND 01) the round used to freeze — player/enemies never spawned, score
//! stayed 00, every sprite parked at (Y=194, X=0). The cause was the VDP→CTC
//! interrupt wiring: the M5 routes the TMS9918 /INT into CTC channel 3, and the
//! BIOS arms that channel for the *falling* edge. We fed the raw /INT level,
//! so its falling edge fell at status-read time — inside the very vblank
//! handler the interrupt is meant to trigger — and the channel deadlocked
//! (ch3 fired twice ever, while a ÷n timer on ch1 flooded ~16 IRQs/frame and
//! masked the gap). MAME inverts the line into TRG3
//! (`vdp.int_callback().set(m_ctc, trg3).invert()`); inverting it moves the
//! falling edge to VBlank, so ch3 fires once per frame, the vblank-synced round
//! logic advances, and the game plays. See `machine-sord-m5/src/lib.rs`.
//!
//! `cart_round_spawns_and_runs` asserts the game leaves its pre-play state;
//! `vdp_int_drives_ctc_channel3` asserts the frame interrupt reaches the CPU
//! via ch3 at the frame rate.
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
fn cart_round_spawns_and_runs() {
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

    // With the VDP→CTC interrupt wired correctly the round reaches play: game
    // RAM mutates *and* the cart writes real sprite positions (player + enemies
    // leave the (194,0) park slot). A regression to the old freeze parks every
    // sprite forever, failing this.
    assert!(
        ram_fingerprint(&sys) != fp0,
        "round logic stalled — work RAM never changed"
    );
    assert!(
        sprites_ever_placed,
        "round never spawned — all sprites stayed parked at (194,0) (VDP/CTC frame interrupt dead?)"
    );
}

/// The game's frame sync flows VDP vblank → CTC channel 3 → IM2. This asserts
/// ch3 actually fires at ~1/frame: the regression (raw, non-inverted /INT)
/// left ch3 silent while a timer on ch1 flooded ~16 IRQs/frame.
#[test]
#[ignore = "needs Sord M5 BIOS + Dig Dug cart — run with --ignored"]
fn vdp_int_drives_ctc_channel3() {
    let bios = env::var("EMU198X_SORD_M5_BIOS")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .expect("set EMU198X_SORD_M5_BIOS");
    let cart = env::var("EMU198X_SORD_M5_CART")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .expect("set EMU198X_SORD_M5_CART to the Dig Dug cart");
    let mut sys = SordM5::new(bios, cart, M5Region::Ntsc);

    for _ in 0..400 {
        sys.run_frame();
    }
    sys.press_key(1, 0); // "1" starts a 1-player game
    for _ in 0..6 {
        sys.run_frame();
    }
    sys.release_key(1, 0);
    for _ in 0..200 {
        sys.run_frame();
    }

    // Total per-channel acks since boot (did ch3 ever fire at all?).
    println!(
        "total acks by channel since boot: {:?}",
        sys.irq_acks_by_channel()
    );

    // Per-channel CTC configuration in the stuck round.
    let ctc = sys.ctc();
    for ch in 0..4u8 {
        println!(
            "ch{ch}: running={} int_en={} counter_mode={} rising_edge={} counter={}",
            ctc.running(ch),
            ctc.int_enabled(ch),
            ctc.counter_mode(ch),
            ctc.rising_edge(ch),
            ctc.counter(ch),
        );
    }

    // Per-channel interrupt rate over 60 frames.
    let before = sys.irq_acks_by_channel();
    for _ in 0..60 {
        sys.run_frame();
    }
    let after = sys.irq_acks_by_channel();
    let mut ch3_per_frame = 0.0;
    for ch in 0..4 {
        let per_frame = (after[ch] - before[ch]) as f64 / 60.0;
        if ch == 3 {
            ch3_per_frame = per_frame;
        }
        println!("ch{ch} interrupts/frame = {per_frame:.2}");
    }
    println!("VDP interrupt line now = {}", sys.vdp().interrupt);
    println!("$754A = 0x{:02X}", sys.peek(0x754A));

    // The VDP-fed channel must deliver the frame interrupt at ~1/frame. A dead
    // ch3 (the old non-inverted-trigger bug) is the freeze.
    assert!(
        ch3_per_frame > 0.9,
        "CTC ch3 (VDP frame interrupt) fired {ch3_per_frame:.2}/frame; expected ~1.0"
    );
}
