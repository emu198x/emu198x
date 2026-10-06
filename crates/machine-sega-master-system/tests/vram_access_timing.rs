//! A Z80 program that writes VRAM faster than the Master System's VDP can
//! take it.
//!
//! Sega's Software Reference Manual for the Mark III (1986, Hardware Reference
//! Manual p. 12, at `reference/by-system/sega-master-system/`) says the VDP
//! "cannot process data any faster than [...] 29 Z80A T-States during active
//! video", and only 16 during vertical blanking. Measured on SMS2s, the real
//! limit during the active display is 26 T-states: Maxim found 26 clean
//! (SMS Power! topic 10523), and sverx's tests found anything faster corrupts
//! (topic 16298). The VDP has no wait output, so the Z80 is never held back.
//!
//! The cartridge below is the whole machine's software: it selects Mode 4,
//! waits for line 10 of the active display, and writes 64 bytes from `$0000`
//! with a fixed number of T-states between `OUT`s.

use machine_sega_master_system::{Sms, SmsVariant};

const WRITES: usize = 64;

/// The byte the `i`th write stores; consecutive values differ, so a lost write
/// leaves another write's byte in its place.
fn value(i: usize) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let byte = i as u8;
    byte + 1
}

/// The cartridge: R0 = Mode 4, R1 as given, then the burst with `pad_nops`
/// NOPs (4 T-states each) after every `LD A,n` (7) + `OUT ($BE),A` (11).
fn cartridge(r1: u8, pad_nops: usize) -> Vec<u8> {
    let mut code = vec![
        0xF3, // DI
        0x3E, 0x04, 0xD3, 0xBF, 0x3E, 0x80, 0xD3, 0xBF, // R0 = $04: Mode 4
        0x3E, r1, 0xD3, 0xBF, 0x3E, 0x81, 0xD3, 0xBF, // R1
        0xDB, 0xBF, // IN A,($BF): clear a stale frame flag
        // wait: IN A,($BF) / AND $80 / JR Z,wait — until the frame flag sets
        // at the end of the active display.
        0xDB, 0xBF, 0xE6, 0x80, 0x28, 0xFA,
        // line: IN A,($7E) / CP 10 / JR NZ,line — until the V counter
        // reaches line 10 of the next frame.
        0xDB, 0x7E, 0xFE, 0x0A, 0x20, 0xFA,
    ];
    // Write address $0000.
    code.extend_from_slice(&[0xAF, 0xD3, 0xBF, 0x3E, 0x40, 0xD3, 0xBF]);
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
    let mut machine = Sms::new(cartridge(r1, pad_nops), SmsVariant::SmsNtsc);
    for _ in 0..3 {
        machine.run_frame();
    }
    let vram = machine.vdp().vram();
    (0..WRITES).take_while(|&i| vram[i] == value(i)).count()
}

/// R1: display enabled.
const DISPLAY_ON: u8 = 0xC0;
/// R1: display blanked.
const BLANKED: u8 = 0x80;

#[test]
fn an_18_t_state_copy_loop_loses_bytes_during_active_display() {
    let intact = intact_writes(DISPLAY_ON, 0);
    assert!(
        intact < WRITES,
        "all {WRITES} writes 18 T-states apart landed during the Mode 4 active display"
    );
}

#[test]
fn the_same_loop_loses_nothing_with_the_display_blanked() {
    assert_eq!(intact_writes(BLANKED, 0), WRITES);
}

#[test]
fn a_loop_paced_to_26_t_states_loses_nothing() {
    // 18 + 2 x 4 = 26 T-states, the spacing measured clean on an SMS2.
    assert_eq!(intact_writes(DISPLAY_ON, 2), WRITES);
}
