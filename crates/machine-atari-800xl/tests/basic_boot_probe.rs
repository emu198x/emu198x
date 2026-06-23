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
    // NOTE: this runs only 300 frames — before BASIC's SIO disk-boot timeout
    // exhausts and "READY" prints (~600 frames). So the cursor block is the only
    // foreground here; the glyph-rendering pixel guard lives in
    // `boots_to_basic_ready`, which runs long enough for the text to appear.
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

    // READY must also RENDER, not just sit in screen RAM. The cursor block alone
    // is ~64 px (one 8x8 cell); "READY" + the cursor must paint well over that.
    // Guards the GR.0 character-set fetch: ANTIC reads the font from $E000 in the
    // OS ROM, so feeding it bare RAM blanked every normal glyph and left only the
    // inverse-video cursor painting (~64 px) — invisible text with READY still in
    // RAM, which the RAM check above happily passes. (assert output, not state.)
    let fb = sys.framebuffer();
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_insert(0) += 1;
    }
    let mut by_count: Vec<usize> = counts.values().copied().collect();
    by_count.sort_unstable_by(|a, b| b.cmp(a));
    let foreground: usize = by_count.iter().skip(2).sum();
    assert!(
        foreground > 100,
        "READY is in screen RAM but only {foreground} foreground px rendered \
         (a lone ~64 px cursor block) — GR.0 glyphs are not painting; ANTIC is \
         not fetching the $E000 character set from the OS ROM"
    );
}

#[test]
#[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
fn keyboard_types_into_basic() {
    let dir = rom_dir().expect("HOME unset");
    let os = std::fs::read(dir.join("atarixl.rom")).expect("atarixl.rom");
    let basic = std::fs::read(dir.join("ataribas.rom")).expect("ataribas.rom");
    let mut sys =
        Atari800xl::new(Some(os), Some(basic), None, Atari800xlRegion::Ntsc, true).expect("boot");
    for _ in 0..600 {
        sys.run_frame();
    }

    // Type `PRINT 6*7` then RETURN. Scan codes are the XL keyboard codes; with
    // the power-on caps lock the bare letter codes type as uppercase, exactly
    // as on a real machine. Each key is pressed, held a few frames, released,
    // then a short settle so the OS keyboard scan sees the release before the
    // next press.
    const KEYS: &[u8] = &[
        0x0A, // P
        0x28, // R
        0x0D, // I
        0x23, // N
        0x2D, // T
        0x21, // space
        0x1B, // 6
        0x07, // *
        0x33, // 7
        0x0C, // RETURN
    ];
    for &code in KEYS {
        sys.press_key(code);
        for _ in 0..3 {
            sys.run_frame();
        }
        sys.release_key();
        for _ in 0..6 {
            sys.run_frame();
        }
    }
    // Let BASIC evaluate and print the result.
    for _ in 0..30 {
        sys.run_frame();
    }

    // The answer "42" must appear in screen RAM (display codes: '4'=$14,
    // '2'=$12). Its presence proves the whole keyboard path: POKEY KBCODE +
    // keyboard IRQ, the OS conversion to ATASCII, and BASIC executing the line.
    let ram = sys.ram();
    let dlist = u16::from(ram[0x0230]) | (u16::from(ram[0x0231]) << 8);
    let screen = first_lms_target(ram, dlist).expect("display list has an LMS");
    let found = (0..40 * 24 - 1).any(|j| ram[screen + j] == 0x14 && ram[screen + j + 1] == 0x12);
    assert!(
        found,
        "typed `PRINT 6*7` did not yield `42` on screen — keyboard path broken"
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
