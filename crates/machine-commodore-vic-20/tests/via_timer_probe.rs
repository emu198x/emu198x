//! VIA #2 timer 1 read a fixed number of cycles after a T1C-H write, through
//! the whole machine: the 6502's bus timing and the order in which the VIC-20
//! clocks its VIAs against the CPU's access both decide the value read.
//!
//! The probe is the one in emu198x#1642. VICE xvic 3.10 reads `$FE96`;
//! counting cycles from the MCS6522 data sheet gives the same. The `STA
//! $9125` write is cycle W. `LDX #20` (2 cycles), then 19 taken `DEX`/`BNE`
//! passes (5 each) and a last untaken one (4), take 101 cycles; `LDA $9124`
//! reads in its fourth cycle, so the read is cycle W + 105. The write cycle
//! loads `$FEFE`, and the counter then loses one a cycle, so the read sees
//! `$FEFE - 104 = $FE96`.
//!
//! The KERNAL here is built by the test, so it needs no fixture.

use machine_commodore_vic_20::{Vic20, Vic20Model, Vic20RamExpansion};

const PROBE: &[u8] = &[
    0x78, // SEI
    0xA9, 0xFE, // LDA #$FE
    0x8D, 0x24, 0x91, // STA $9124  (T1 low latch)
    0x8D, 0x25, 0x91, // STA $9125  (T1C-H: load and start)
    0xA2, 0x14, // LDX #20
    0xCA, // loop: DEX
    0xD0, 0xFD, //   BNE loop
    0xAD, 0x24, 0x91, // LDA $9124  (T1C-L)
    0xAC, 0x25, 0x91, // LDY $9125  (T1C-H)
    0x85, 0x00, // STA $00
    0x84, 0x01, // STY $01
    0x4C, 0x18, 0xE0, // JMP * ($E018)
];

fn probe_kernal() -> Vec<u8> {
    let mut kernal = vec![0xEA; 0x2000];
    kernal[..PROBE.len()].copy_from_slice(PROBE);
    // NMI, reset and IRQ vectors all point at the probe's start ($E000).
    for vector in [0x1FFA, 0x1FFC, 0x1FFE] {
        kernal[vector] = 0x00;
        kernal[vector + 1] = 0xE0;
    }
    kernal
}

#[test]
fn via2_timer1_read_105_cycles_after_t1ch_write_matches_vice() {
    for model in [Vic20Model::Pal, Vic20Model::Ntsc] {
        let mut machine = Vic20::new(
            probe_kernal(),
            vec![0; 0x2000],
            vec![0; 0x1000],
            model,
            Vic20RamExpansion::NONE,
        );
        machine.run_frame();

        let pc = machine.cpu().regs.pc;
        assert!(
            (0xE018..=0xE01B).contains(&pc),
            "{model:?}: the probe should be in its final loop, PC is ${pc:04X}"
        );
        let low = machine.peek(0x0000);
        let high = machine.peek(0x0001);
        assert_eq!(
            (high, low),
            (0xFE, 0x96),
            "{model:?}: T1 read ${high:02X}{low:02X}; VICE reads $FE96"
        );
    }
}
