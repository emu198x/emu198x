//! Sord M5 keyboard I/O-port trace — confirms which ports the Monitor ROM
//! actually reads/writes when scanning the keyboard, so the keyboard model is
//! grounded in the silicon's own behaviour rather than a (possibly wrong) donor
//! map. Per `project_donor_io_map_unverified.md`: trace the boot ROM's IN/OUT,
//! don't trust the map.
//!
//! MAME's `sord/m5.cpp` reads the keyboard as direct row ports `$30`-`$36`
//! (active-high); the donor model instead strobes a row via a `$30` write and
//! reads the column at `$40` (active-low). This test prints what the BIOS does
//! so we can pick the right one.
//!
//! Gated `#[ignore]` because the Monitor ROM is copyrighted. Run with:
//! ```text
//! EMU198X_SORD_M5_BIOS=/path/sord-m5.rom EMU198X_SORD_M5_CART=/path/cart.bin \
//!   cargo test --release -p machine-sord-m5 \
//!     --test keyboard_io_trace -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::env;
use std::fs;

use machine_sord_m5::{M5Region, SordM5};

#[test]
#[ignore = "FIXTURE: needs Sord M5 BIOS — run with --ignored"]
fn trace_keyboard_scan_ports() {
    let bios = env::var("EMU198X_SORD_M5_BIOS")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .expect("set EMU198X_SORD_M5_BIOS to the 8 KB Monitor ROM");
    let cart = env::var("EMU198X_SORD_M5_CART")
        .ok()
        .and_then(|p| fs::read(p).ok())
        .unwrap_or_default();

    let mut sys = SordM5::new(bios, cart, M5Region::Ntsc);
    // Boot well past VDP/CTC init so the monitor is in its keyboard-polling
    // loop / interrupt handler.
    for _ in 0..240 {
        sys.run_frame();
    }

    sys.start_io_trace();
    // A few frames of steady-state polling.
    for _ in 0..4 {
        sys.run_frame();
    }
    let events = sys.take_io_trace();

    // Tally reads and writes per port across the keyboard-relevant range.
    let mut reads: BTreeMap<u8, (u32, u8)> = BTreeMap::new();
    let mut writes: BTreeMap<u8, (u32, u8)> = BTreeMap::new();
    for ev in &events {
        let tally = if ev.write { &mut writes } else { &mut reads };
        let entry = tally.entry(ev.port).or_insert((0, ev.value));
        entry.0 += 1;
        entry.1 = ev.value; // last value seen
    }

    println!("--- Sord M5 keyboard-range I/O over 4 frames ---");
    println!("total I/O events: {}", events.len());
    println!("READS  (port: count, last_value):");
    for (port, (count, last)) in &reads {
        println!("  ${port:02X}: {count}x  last=0x{last:02X}");
    }
    println!("WRITES (port: count, last_value):");
    for (port, (count, last)) in &writes {
        println!("  ${port:02X}: {count}x  last=0x{last:02X}");
    }

    assert!(
        !reads.is_empty() || !writes.is_empty(),
        "expected the monitor to touch the keyboard I/O range"
    );
}
