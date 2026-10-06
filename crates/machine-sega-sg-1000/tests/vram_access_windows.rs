//! A Z80 program that writes VRAM faster than the TMS9918 can take it.
//!
//! The VDP performs a CPU access only in a memory cycle its scan engine leaves
//! free, and in Graphics II during active display that is one cycle in 16
//! (data manual §2.1.5, Table 2-2, at `reference/by-topic/vdp-tms9918/`). The
//! chip has no wait output, so the Z80 is never held back: a copy loop that
//! comes round too soon overtakes its last write, and the byte is lost.
//!
//! The SG-1000 runs the cartridge straight from reset, so the program below is
//! the whole machine's software. It sets the mode, waits for the vertical
//! interrupt flag, idles into the active display, and then writes 64 bytes
//! from `$0000` with a fixed number of T-states between `OUT`s.

use machine_sega_sg_1000::{Sg1000, Sg1000Region};

const WRITES: usize = 64;

/// The byte the `i`th write stores; consecutive values differ, so a lost write
/// leaves the next one's byte in its place.
fn value(i: usize) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let byte = i as u8;
    byte + 1
}

/// The cartridge: R0 and R1 as given, then the burst with `pad_nops` NOPs
/// (4 T-states each) after every `LD A,n` (7) + `OUT ($BE),A` (11).
fn cartridge(r0: u8, r1: u8, pad_nops: usize) -> Vec<u8> {
    let mut code = vec![
        0xF3, // DI
        0x3E, r0, 0xD3, 0xBF, 0x3E, 0x80, 0xD3, 0xBF, // R0
        0x3E, r1, 0xD3, 0xBF, 0x3E, 0x81, 0xD3, 0xBF, // R1
        0xDB, 0xBF, // IN A,($BF): clear a stale frame flag
        // wait: IN A,($BF) / AND $80 / JR Z,wait — until the frame flag sets
        // at the end of active display.
        0xDB, 0xBF, 0xE6, 0x80, 0x28, 0xFA,
        // Idle through the vertical border into the active display: 693
        // passes of 26 T-states is 79 lines of 228, so the burst starts
        // around line 10.
        0x01, 0xB5, 0x02, // LD BC,693
        0x0B, 0x78, 0xB1, 0x20, 0xFB, // DEC BC / LD A,B / OR C / JR NZ
        // Write address $0000.
        0xAF, 0xD3, 0xBF, 0x3E, 0x40, 0xD3, 0xBF,
    ];
    for i in 0..WRITES {
        code.extend_from_slice(&[0x3E, value(i), 0xD3, 0xBE]);
        code.extend(std::iter::repeat_n(0x00, pad_nops));
    }
    code.extend_from_slice(&[0x18, 0xFE]); // JR $
    code.resize(32 * 1024, 0xFF);
    code
}

/// Run the cartridge and count the writes that reached VRAM before the first
/// one that did not.
fn intact_writes(r1: u8, pad_nops: usize) -> usize {
    // R0 = $02: Graphics II.
    let mut machine = Sg1000::new(cartridge(0x02, r1, pad_nops), Sg1000Region::Ntsc);
    for _ in 0..3 {
        machine.run_frame();
    }
    let vram = machine.vdp().vram();
    (0..WRITES).take_while(|&i| vram[i] == value(i)).count()
}

/// R1: 16K VRAM, display enabled.
const DISPLAY_ON: u8 = 0xC0;
/// R1: 16K VRAM, display blanked.
const BLANKED: u8 = 0x80;

#[test]
fn an_18_t_state_copy_loop_loses_bytes_during_active_display() {
    let intact = intact_writes(DISPLAY_ON, 0);
    assert!(
        intact < WRITES,
        "all {WRITES} writes 18 T-states apart landed during Graphics II active display"
    );
}

#[test]
fn the_same_loop_loses_nothing_with_the_display_blanked() {
    // Table 2-2: with R1's blank bit at 0 there is no window to wait for.
    assert_eq!(intact_writes(BLANKED, 0), WRITES);
}

#[test]
fn a_loop_paced_to_the_worst_case_window_loses_nothing() {
    // 18 + 2 x 4 = 26 T-states, 39 dots: longer than the 38-dot worst wait.
    assert_eq!(intact_writes(DISPLAY_ON, 2), WRITES);
}
