//! SHAKER (Longshot) on the CPC — AMSDOS extraction and injection.
//!
//! SHAKER is the CPC's hardware-accuracy suite. It exercises the Gate Array
//! and the CRTC 6845 across their manufacturing variants, and — the reason it
//! is here — it measures **interrupt acceptance timing**:
//!
//! ```text
//! DELAY BETWEEN HSYNC (C0=R2) AND INTERRUPTION (IM1)
//! TEST INT ON INST DEC DE       :#xx (CRTC 3+4:#58/ OTHERS:#59)
//! Unbreakable DD Prefix on Pending Int #xx (Exp#00)
//! ```
//!
//! That is the axis the Spectrum cannot vary. The Spectrum's ULA asserts
//! `/INT` on a raster schedule; the CPC's Gate Array raises it from a 6-bit
//! HSync counter driven by the CRTC. The decision record
//! `knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md`
//! is adopted *pending* exactly this cross-machine check.
//!
//! Two of the five modules carry the interrupt tests — `SHAKE26B` and
//! `SHAKE26D`. The other three are CRTC and video work.
//!
//! ## Why injection rather than a disc
//!
//! SHAKER ships as an Extended DSK and the CPC464 modelled here has no FDC,
//! which is correct for the hardware — the 464 is tape-only. Rather than block
//! on a µPD765 and a 6128-class variant (#951), this reads the AMSDOS
//! catalogue directly and injects the binary at its recorded load address, the
//! same shape as `machine-sinclair-zx-spectrum-48k`'s `z80test` harness.
//!
//! ## What is not here yet
//!
//! Reading SHAKER's *results*. It does not print through the firmware: a
//! `TXT OUTPUT` (`&BB5A`) trap catches the boot banner — `" BASIC 1.0"`,
//! `"Ready"` — and then nothing at all, because a suite measuring video timing
//! writes screen memory directly. Scoring needs a CPC screen-text decoder:
//! mode 1, 8×8 cells, matched against the firmware font. Until that exists,
//! these tests establish that the suite loads and runs, not what it concludes.
//!
//! ```text
//! cargo test -p machine-amstrad-cpc --test shaker -- --ignored
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_amstrad_cpc::AmstradCpc;
use nec_upd765a::DiskImage;

/// CPC DATA format: nine 512-byte sectors per track, IDs `&C1..&C9`.
const SECTORS_PER_TRACK: usize = 9;
const FIRST_SECTOR_ID: u8 = 0xC1;
/// An AMSDOS allocation block is 1 KB — two sectors.
const SECTORS_PER_BLOCK: usize = 2;
/// The catalogue lives in blocks 0 and 1: track 0, sectors `&C1..&C4`.
const CATALOGUE_SECTORS: usize = 4;
/// One directory entry, and the record size its `RC` field counts in.
const DIR_ENTRY_LEN: usize = 32;
const RECORD_LEN: usize = 128;
/// Every AMSDOS binary carries a 128-byte header.
const AMSDOS_HEADER_LEN: usize = 128;

/// Frames to let the firmware reach its BASIC prompt — the state AMSDOS
/// would have launched the binary from.
const BOOT_FRAMES: usize = 150;

/// A file lifted out of the catalogue, with its AMSDOS header decoded.
#[derive(Debug)]
struct AmsdosFile {
    /// Address the file expects to be loaded at.
    load: u16,
    /// Address to call once it is there.
    entry: u16,
    /// Payload, header already stripped.
    body: Vec<u8>,
}

fn dsk_path() -> PathBuf {
    if let Some(p) = env::var_os("EMU198X_CPC_SHAKER_DSK") {
        return PathBuf::from(p);
    }
    PathBuf::from(env::var("HOME").expect("HOME"))
        .join(".emu198x/test-data/amstrad-cpc/shaker26.dsk")
}

fn firmware_path() -> PathBuf {
    if let Some(p) = env::var_os("EMU198X_CPC_ROM") {
        return PathBuf::from(p);
    }
    PathBuf::from(env::var("HOME").expect("HOME")).join(".emu198x/roms/amstrad-cpc/cpc464.rom")
}

/// Read one 1 KB allocation block.
fn read_block(img: &DiskImage, block: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(SECTORS_PER_BLOCK * 512);
    for i in 0..SECTORS_PER_BLOCK {
        let logical = usize::from(block) * SECTORS_PER_BLOCK + i;
        let track = u8::try_from(logical / SECTORS_PER_TRACK).expect("track fits");
        let id = FIRST_SECTOR_ID + u8::try_from(logical % SECTORS_PER_TRACK).expect("id fits");
        let sector = img
            .sector(track, 0, id)
            .unwrap_or_else(|| panic!("block {block} wants track {track} sector ${id:02X}"));
        out.extend_from_slice(&sector.data);
    }
    out
}

/// Pull a named file out of the AMSDOS catalogue.
///
/// Entries are keyed by `(name, extent)`; a file larger than 16 KB spans
/// several extents, concatenated in extent order. `RC` gives the record count
/// for the extent, which bounds the last block — without it the file would run
/// on to a block boundary and pick up whatever followed it.
fn extract(img: &DiskImage, want: &str) -> Option<AmsdosFile> {
    let mut catalogue = Vec::new();
    for i in 0..CATALOGUE_SECTORS {
        let id = FIRST_SECTOR_ID + u8::try_from(i).expect("id fits");
        catalogue.extend_from_slice(&img.sector(0, 0, id)?.data);
    }

    let mut extents: Vec<(u16, usize, Vec<u8>)> = Vec::new();
    for entry in catalogue.chunks_exact(DIR_ENTRY_LEN) {
        // User 0 only. `$E5` marks a deleted entry; anything else is another
        // user area.
        if entry[0] != 0 {
            continue;
        }
        // Bit 7 of each name/extension byte is an attribute flag, not text.
        let name: String = entry[1..9].iter().map(|b| char::from(b & 0x7F)).collect();
        let ext: String = entry[9..12].iter().map(|b| char::from(b & 0x7F)).collect();
        if format!("{}.{}", name.trim_end(), ext.trim_end()) != want {
            continue;
        }
        // The extent number is split: low bits in EX, high bits in S2.
        let extent = u16::from(entry[12]) + u16::from(entry[14]) * 32;
        let blocks: Vec<u8> = entry[16..32].iter().copied().filter(|&b| b != 0).collect();
        extents.push((extent, usize::from(entry[15]), blocks));
    }
    if extents.is_empty() {
        return None;
    }
    extents.sort_by_key(|(extent, _, _)| *extent);

    let mut raw = Vec::new();
    for (_, records, blocks) in &extents {
        let mut chunk = Vec::new();
        for &b in blocks {
            chunk.extend_from_slice(&read_block(img, b));
        }
        chunk.truncate(records * RECORD_LEN);
        raw.extend_from_slice(&chunk);
    }

    assert!(
        raw.len() > AMSDOS_HEADER_LEN,
        "{want}: only {} bytes, no room for an AMSDOS header",
        raw.len()
    );
    let header = &raw[..AMSDOS_HEADER_LEN];

    // Validate before trusting any field. The checksum covers bytes
    // `$00..$42`; a mismatch means either the layout assumed here is wrong or
    // the file is headerless, and the load address below would be garbage
    // either way.
    let sum: u32 = header[..0x43].iter().map(|&b| u32::from(b)).sum();
    let stored = u32::from(header[0x43]) | (u32::from(header[0x44]) << 8);
    assert_eq!(
        sum & 0xFFFF,
        stored,
        "{want}: AMSDOS header checksum mismatch — not a headered binary?"
    );
    assert_eq!(header[0x12], 2, "{want}: expected a binary (type 2)");

    let load = u16::from(header[0x15]) | (u16::from(header[0x16]) << 8);
    let entry = u16::from(header[0x1A]) | (u16::from(header[0x1B]) << 8);
    let length = usize::from(header[0x18]) | (usize::from(header[0x19]) << 8);

    Some(AmsdosFile {
        load,
        entry,
        body: raw[AMSDOS_HEADER_LEN..][..length].to_vec(),
    })
}

/// Boot the firmware, inject `module`, and enter it on an instruction
/// boundary.
fn boot_and_enter(firmware: &[u8], module: &AmsdosFile) -> AmstradCpc {
    let mut cpc = AmstradCpc::new(firmware).expect("build machine");
    for _ in 0..BOOT_FRAMES {
        cpc.run_frame();
    }
    for (i, &b) in module.body.iter().enumerate() {
        cpc.poke(module.load.wrapping_add(u16::try_from(i).expect("fits")), b);
    }

    // Enter on a real instruction boundary.
    //
    // `run_frame` stops on a frame boundary, which says nothing about where
    // the CPU is inside an instruction. Writing `PC` mid-instruction lets the
    // in-flight instruction finish against the *new* PC and swallow the
    // injected binary's first bytes — that is #943, on the Spectrum.
    //
    // An `m1` edge is not a good enough proxy for that boundary. Tried here
    // across every entry phase, and the `JP` sitting at the entry point never
    // executed once: `PC` walked byte-by-byte through the module as though it
    // were NOPs. `instruction_complete()` is the signal that actually means
    // what is needed, and with it the module takes over cleanly.
    let mut guard = 0;
    while !cpc.z80().instruction_complete() {
        cpc.advance_tstates(1);
        guard += 1;
        assert!(guard < 256, "no instruction boundary within 256 t-states");
    }
    cpc.z80_mut().regs.pc = module.entry;
    cpc
}

/// Non-zero bytes in the 16 KB screen at `&C000`.
fn screen_ink(cpc: &AmstradCpc) -> usize {
    (0xC000u32..=0xFFFF)
        .filter(|&a| cpc.peek(u16::try_from(a).expect("fits")) != 0)
        .count()
}

#[test]
#[ignore = "needs shaker26.dsk — run with --ignored"]
fn the_interrupt_modules_come_out_of_the_catalogue() {
    let path = dsk_path();
    if !path.exists() {
        emu198x_test_skip::skip!("shaker26.dsk not staged (EMU198X_CPC_SHAKER_DSK)");
    }
    let img = format_amstrad_dsk::parse(&fs::read(&path).expect("read dsk")).expect("parse dsk");

    // The two modules carrying interrupt tests. Both are entered at the same
    // address despite loading a page apart, which is what a relocating stub
    // looks like — worth knowing before injecting either.
    for (name, want_load) in [("SHAKE26B.BIN", 0x3900u16), ("SHAKE26D.BIN", 0x3A00)] {
        let f = extract(&img, name).unwrap_or_else(|| panic!("{name} not in the catalogue"));
        assert_eq!(f.load, want_load, "{name} load address");
        assert_eq!(f.entry, 0x4042, "{name} entry address");
        assert!(
            !f.body.is_empty() && usize::from(f.load) + f.body.len() <= 0x10000,
            "{name}: {} bytes at ${:04X} does not fit in the address space",
            f.body.len(),
            f.load
        );
    }
}

#[test]
#[ignore = "needs shaker26.dsk and the CPC464 firmware — run with --ignored"]
fn shaker_module_d_takes_over_and_runs() {
    let (dsk, rom) = (dsk_path(), firmware_path());
    if !dsk.exists() || !rom.exists() {
        emu198x_test_skip::skip!("shaker26.dsk or cpc464.rom not staged");
    }
    let img = format_amstrad_dsk::parse(&fs::read(&dsk).expect("read dsk")).expect("parse dsk");
    let module = extract(&img, "SHAKE26D.BIN").expect("SHAKE26D.BIN");
    let firmware = fs::read(&rom).expect("read firmware");

    let mut cpc = boot_and_enter(&firmware, &module);
    let ink_at_entry = screen_ink(&cpc);

    // Ten seconds of CPC time. SHAKER's tests are long — it is built to run on
    // real hardware and be watched.
    const RUN_TSTATES: u32 = 40_000_000;
    let (mut lo, mut hi) = (0xFFFFu16, 0u16);
    let mut in_firmware = 0u64;
    for _ in 0..RUN_TSTATES {
        cpc.advance_tstates(1);
        let pc = cpc.z80().regs.pc;
        lo = lo.min(pc);
        hi = hi.max(pc);
        if pc < 0x3900 {
            in_firmware += 1;
        }
    }

    // It never falls back into the firmware. Entering on an `m1` edge instead
    // spent 98.6% of the run below `$3900` — the firmware idling at its prompt
    // while the injected binary was never reached. That is the difference
    // between "loaded" and "running", and it is worth asserting rather than
    // eyeballing a screenshot.
    assert_eq!(
        in_firmware, 0,
        "SHAKE26D dropped into firmware ROM for {in_firmware} of {RUN_TSTATES} \
         t-states; PC ranged ${lo:04X}..${hi:04X}"
    );
    assert!(
        lo >= module.load && hi < 0xC000,
        "PC ranged ${lo:04X}..${hi:04X}, outside the module at ${:04X}",
        module.load
    );

    // And it paints its own menu. Ink counting could only say that *something*
    // was drawn; decoding says what, so this is the assertion worth making.
    let screen = cpc.screen_text();
    let joined = screen.join("\n");
    eprintln!("--- SHAKE26D screen ---\n{joined}\n---");

    assert!(
        joined.contains("CPC SHAKER 2.6 MODULE D"),
        "SHAKE26D did not reach its own menu; screen was:\n{joined}"
    );
    assert!(
        screen_ink(&cpc) > ink_at_entry,
        "screen unchanged since entry ({ink_at_entry} bytes)"
    );

    // The interrupt tests live behind `(I) SHAKER KILLER 2`, whose own
    // warning — `SK 2-UNRELIABLE INTERRUPT SYSTEM BETWEEN CPCs` — is the
    // caveat to carry into any figure it reports. Reaching them needs the
    // menu driven by key, which is the next step and not this one.
    assert!(
        joined.contains("SHAKER KILLER 2"),
        "the menu entry carrying the interrupt tests is missing:\n{joined}"
    );
}

/// Frames to hold a key so the suite's own polling loop sees it.
const KEY_HOLD_FRAMES: usize = 10;

/// Run frames until `needle` appears on the decoded screen.
fn run_until_screen_has(cpc: &mut AmstradCpc, needle: &str, limit: usize) -> Option<Vec<String>> {
    for _ in 0..limit {
        cpc.run_frame();
        let rows = cpc.screen_text();
        if rows.iter().any(|r| r.contains(needle)) {
            return Some(rows);
        }
    }
    None
}

/// Press one key, hold it, release it.
fn tap(cpc: &mut AmstradCpc, c: char) {
    assert!(cpc.press_char(c), "no CPC key for {c:?}");
    for _ in 0..KEY_HOLD_FRAMES {
        cpc.run_frame();
    }
    cpc.release_char(c);
    for _ in 0..KEY_HOLD_FRAMES {
        cpc.run_frame();
    }
}

/// Drive SHAKER's menu into `SHAKER KILLER 2` and read its interrupt page.
///
/// This is the instrument #942 wants: it measures where the interrupt lands
/// relative to a named instruction, against a `/INT` asserted by the Gate
/// Array's HSync counter rather than by a raster. The Spectrum cannot vary
/// that axis.
///
/// The page is captured, not yet scored. SHAKER renders hex nibbles `A`-`F`
/// as `:;<=>?` — the `add a,'0'` shortcut with no `>9` correction — so a
/// reported `#<<` is `#CC`. That reading is inferred from the glyphs rather
/// than from SHAKER's source, so the values are printed for a human to check
/// against the Compendium before any of them is treated as a verdict on this
/// emulator. Confirming it is what turns this from a capture into a gate.
///
/// The page's own first line is the standing caveat:
/// `SK 2-UNRELIABLE INTERRUPT SYSTEM BETWEEN CPCs`. A disagreement is not
/// automatically a defect here until a target CPC variant is named.
#[test]
#[ignore = "needs shaker26.dsk and the CPC464 firmware — run with --ignored"]
fn shaker_killer_2_reports_its_interrupt_measurements() {
    let (dsk, rom) = (dsk_path(), firmware_path());
    if !dsk.exists() || !rom.exists() {
        emu198x_test_skip::skip!("shaker26.dsk or cpc464.rom not staged");
    }
    let img = format_amstrad_dsk::parse(&fs::read(&dsk).expect("read dsk")).expect("parse dsk");
    let module = extract(&img, "SHAKE26D.BIN").expect("SHAKE26D.BIN");
    let firmware = fs::read(&rom).expect("read firmware");

    let mut cpc = boot_and_enter(&firmware, &module);

    let menu = run_until_screen_has(&mut cpc, "CPC SHAKER 2.6 MODULE D", 600)
        .expect("SHAKE26D never drew its menu");
    // SHAKER prints the CRTC type it detected. Its expectations are
    // enumerated per type, so no result means anything without it.
    let crtc = menu
        .iter()
        .rev()
        .find(|r| r.starts_with("CRTC "))
        .cloned()
        .unwrap_or_else(|| "unknown".to_owned());
    eprintln!("[SHAKER] detected {crtc}");
    // The target is a 464, which fits an HD6845S — CPC type 0. SHAKER detects
    // the part by reading registers back, so this is real software checking
    // the claim rather than a comment asserting it. It read `CRTC 2` until
    // `Crtc6845Variant::Hd6845s` made R12/R13 read back.
    assert_eq!(
        crtc, "CRTC 0",
        "SHAKER should detect a 464's HD6845S as type 0"
    );

    tap(&mut cpc, 'I');
    // Hardware-timing measurements, not instant checks. The page settles well
    // inside this and does not change over four times as long.
    for _ in 0..1_200 {
        cpc.run_frame();
    }

    let rows = cpc.screen_text();
    eprintln!(
        "--- SHAKER KILLER 2 (detected {crtc}) ---\n{}\n---",
        rows.join("\n")
    );
    let joined = rows.join("\n");

    assert!(
        !joined.contains("CPC SHAKER 2.6 MODULE D"),
        "still on the menu — `I` did not select SHAKER KILLER 2:\n{joined}"
    );
    // The four interrupt measurements this page exists to report. Their
    // presence is the gate; their values need the Compendium.
    for expected in [
        "TEST INT ON INST SET n,(IX+n')",
        "TEST INT ON INST CP (IX+n)",
        "TEST INT ON INST DEC DE",
        "Unbreakable DD Prefix on Pending Int",
    ] {
        assert!(
            joined.contains(expected),
            "SHAKER KILLER 2 did not report {expected:?}:\n{joined}"
        );
    }
}
