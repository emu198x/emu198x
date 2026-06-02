//! Regression: the OS cold-boots into a programmed display.
//!
//! Guards two bugs that together hung the 800XL boot at a JAM opcode
//! ($C007) around frame 197:
//!
//! 1. **PIA register addressing.** The Atari board cross-wires CPU
//!    A0↔A1 into the PIA's RS pins, so $D300/01/02/03 select
//!    PORTA/PORTB/CRA/CRB. With the bits un-swapped, the OS's
//!    `BIT $D302` read PORTB instead of CRA; PORTB bit 7 (self-test
//!    off) looked like a pending PIA "proceed" interrupt, and the OS
//!    spun in VPRCED dispatch until the stack walked it into the JAM.
//!
//! 2. **ANTIC NMIEN bit assignment.** VBI enable is bit 6 and DLI
//!    enable is bit 7; they were swapped, so the OS's NMIEN=$40 never
//!    armed the vertical-blank NMI. Without VBI, RTCLOK never advanced
//!    and the OS busy-waited forever before reaching display setup.
//!
//! Gated behind a local ROM bundle — needs the XL OS + BASIC ROMs.

use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

fn rom_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/atari-800xl"))
}

#[test]
#[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
fn basic_boot_programs_antic_and_gtia() {
    let dir = rom_dir().expect("HOME unset");
    let os = std::fs::read(dir.join("atarixl.rom")).expect("atarixl.rom");
    let basic = std::fs::read(dir.join("ataribas.rom")).expect("ataribas.rom");
    let mut sys =
        Atari800xl::new(Some(os), Some(basic), None, Atari800xlRegion::Ntsc, true).expect("boot");

    // ~5 seconds of NTSC frames — well past the OS cold-boot settle.
    let rtclok_start = sys.ram()[0x14];
    for _ in 0..300 {
        sys.run_frame();
        assert!(
            !sys.cpu().halted,
            "CPU executed an illegal opcode during boot (PC=${:04X}) — the \
             PIA addressing / NMIEN regression has returned",
            sys.cpu().regs.pc
        );
    }

    // The vertical-blank interrupt must be advancing the real-time clock.
    let rtclok_end = sys.ram()[0x14];
    assert_ne!(
        rtclok_start, rtclok_end,
        "RTCLOK ($14) never advanced — VBI NMI is not firing"
    );

    // The OS must have programmed the display: DMACTL with DL DMA enabled
    // (bit 5) and a non-zero display-list pointer.
    let dmactl = sys.antic().dmactl_value();
    assert_ne!(
        dmactl & 0x20,
        0,
        "ANTIC DMACTL did not enable display-list DMA (${dmactl:02X})"
    );
    assert_ne!(
        sys.antic().dlist_value(),
        0x0000,
        "ANTIC display-list pointer was never set"
    );

    // The GR.0 screen must actually render. A correct boot frame holds three
    // distinct colours: the COLBK border, the COLPF2 playfield background, and
    // the hi-res cursor block (COLPF2 hue + COLPF1 luminance). Before the
    // ANTIC LMS/DLI, hi-res-text colour, and CHACTL fixes this collapsed to a
    // single black frame.
    let fb = sys.framebuffer();
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_insert(0) += 1;
    }
    assert!(
        counts.len() >= 3,
        "framebuffer has only {} distinct colour(s); expected border + \
         playfield + cursor",
        counts.len()
    );

    // The inverse-video cursor renders as a small 8x8 block — the rarest
    // colour, distinctly smaller than the border or playfield fills. Its
    // presence proves hi-res character pixels render (the CHACTL fix) in a
    // third colour (the hi-res-text colour fix).
    let smallest = counts.values().copied().min().unwrap_or(0);
    assert!(
        (32..4096).contains(&smallest),
        "expected a small cursor block (~64 px); rarest colour covers {smallest} px"
    );
}

/// "READY" screen codes (internal display codes, not ATASCII).
/// R=$32 E=$25 A=$21 D=$24 Y=$39.
const READY_SCREEN_CODES: [u8; 5] = [0x32, 0x25, 0x21, 0x24, 0x39];

#[test]
#[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
fn boots_to_basic_ready() {
    let dir = rom_dir().expect("HOME unset");
    let os = std::fs::read(dir.join("atarixl.rom")).expect("atarixl.rom");
    let basic = std::fs::read(dir.join("ataribas.rom")).expect("ataribas.rom");
    let mut sys =
        Atari800xl::new(Some(os), Some(basic), None, Atari800xlRegion::Ntsc, true).expect("boot");

    // The built-in BASIC cartridge sets the "boot peripherals" flag, so the
    // OS attempts a disk boot over SIO before running BASIC. With no drive,
    // POKEY's serial transmit completes, the ACK times out, and the OS falls
    // through to the cartridge. BASIC cold-starts and prints READY. This needs
    // ~5 s of emulated time for the SIO retries to exhaust.
    for _ in 0..600 {
        sys.run_frame();
    }

    // BASIC's cold start initialises its zero-page pointers: LOMEM ($80/$81)
    // and the variable-name table pointer VNTP ($82/$83) become non-zero.
    let lomem = u16::from(sys.ram()[0x80]) | (u16::from(sys.ram()[0x81]) << 8);
    let vntp = u16::from(sys.ram()[0x82]) | (u16::from(sys.ram()[0x83]) << 8);
    assert_ne!(
        lomem, 0,
        "BASIC never cold-started (LOMEM still zero) — the OS did not fall \
         through the SIO disk boot to the cartridge"
    );
    assert_ne!(vntp, 0, "BASIC VNTP still zero — cold start incomplete");

    // "READY" must be present in the live screen RAM (located via the display
    // list's first LMS operand, since BASIC's screen sits below RAMTOP $A0).
    let ram = sys.ram();
    let dlist = u16::from(ram[0x0230]) | (u16::from(ram[0x0231]) << 8);
    let screen = first_lms_target(ram, dlist).expect("display list has an LMS");
    let found = (0..40 * 24 - READY_SCREEN_CODES.len())
        .any(|j| ram[screen + j..screen + j + READY_SCREEN_CODES.len()] == READY_SCREEN_CODES);
    assert!(
        found,
        "BASIC's READY prompt was not found in screen RAM at ${screen:04X}"
    );
}

/// Walk a display list and return the screen-memory address from its first
/// LMS (load-memory-scan) instruction — a mode-line byte ($02-$0F) with the
/// LMS bit (6) set, whose two operand bytes hold the address.
fn first_lms_target(ram: &[u8], dlist: u16) -> Option<usize> {
    let mut p = dlist as usize;
    for _ in 0..64 {
        let b = ram[p];
        let mode = b & 0x0F;
        let lms = b & 0x40 != 0;
        if lms && mode >= 0x02 {
            return Some(usize::from(ram[p + 1]) | (usize::from(ram[p + 2]) << 8));
        }
        match mode {
            0x01 => return None, // jump — give up
            _ if lms => p += 3,  // mode line + LMS operand
            _ => p += 1,         // blank / plain mode line
        }
    }
    None
}
