//! Cartridge autostart boot invariant for the C64 runtime.
//!
//! Builds a synthetic 8K generic cartridge carrying the `CBM80` autostart
//! signature and a cold-start routine that writes two screen codes straight
//! into screen RAM, then drives it through the full runtime media path:
//! `load_media` → `reset` → run. Asserting those bytes land in screen RAM
//! proves the whole chain — CRT parse, PLA banking (`EXROM`/`GAME`), the
//! reset-time re-insert, and the KERNAL's `$8004` cartridge scan handing
//! control to the cartridge cold-start vector.
//!
//! ROM-backed and `#[ignore]`'d; resolves the KERNAL/BASIC/CHARGEN from
//! `~/.emu198x/roms/commodore-c64/`.

use std::error::Error;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_commodore_c64::{C64Runtime, Model};

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn home_c64_rom_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-c64");
    if path.exists() { Some(path) } else { None }
}

/// Build an 8K generic autostart cartridge (`.crt`) that writes two screen
/// codes into screen RAM and hangs.
fn autostart_hi_crt() -> Vec<u8> {
    let mut rom = vec![0u8; 0x2000];
    // Cold- and warm-start vectors both point at the code at $8009.
    rom[0x00..0x02].copy_from_slice(&0x8009u16.to_le_bytes());
    rom[0x02..0x04].copy_from_slice(&0x8009u16.to_le_bytes());
    // "CBM80" autostart signature (PETSCII) at $8004.
    rom[0x04..0x09].copy_from_slice(&[0xC3, 0xC2, 0xCD, 0x38, 0x30]);
    // Cold-start routine at $8009: write screen codes 'H' (8) and 'I' (9) to
    // $0400/$0401, then hang. No KERNAL init needed — the test reads screen RAM
    // directly, so this proves autostart handed control to the cartridge.
    let code: [u8; 14] = [
        0x78, // SEI
        0xA9, 0x08, // LDA #$08  ('H')
        0x8D, 0x00, 0x04, // STA $0400
        0xA9, 0x09, // LDA #$09  ('I')
        0x8D, 0x01, 0x04, // STA $0401
        0x4C, 0x14, 0x80, // JMP $8014 (hang)
    ];
    rom[0x09..0x09 + code.len()].copy_from_slice(&code);

    // Wrap the ROM image in a .crt container: 64-byte header + one CHIP packet.
    let mut crt = Vec::new();
    crt.extend_from_slice(b"C64 CARTRIDGE   ");
    crt.extend_from_slice(&0x40u32.to_be_bytes()); // header length
    crt.extend_from_slice(&0x0100u16.to_be_bytes()); // version
    crt.extend_from_slice(&0u16.to_be_bytes()); // hardware type 0 (generic)
    crt.push(0); // EXROM asserted (low) → 8K/16K
    crt.push(1); // GAME not asserted → 8K
    crt.extend_from_slice(&[0u8; 6]); // reserved
    crt.extend_from_slice(&[0u8; 32]); // name
    crt.extend_from_slice(b"CHIP");
    crt.extend_from_slice(&((0x10 + rom.len()) as u32).to_be_bytes());
    crt.extend_from_slice(&0u16.to_be_bytes()); // ROM
    crt.extend_from_slice(&0u16.to_be_bytes()); // bank 0
    crt.extend_from_slice(&0x8000u16.to_be_bytes()); // load address
    crt.extend_from_slice(&(rom.len() as u16).to_be_bytes());
    crt.extend_from_slice(&rom);
    crt
}

/// Waypoint: a real KERNAL detects an inserted autostart cartridge and runs it.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-c64/{kernal,basic,chargen}.rom"]
fn autostart_cartridge_prints_to_screen() -> Result<(), Box<dyn Error>> {
    let Some(rom_dir) = home_c64_rom_dir() else {
        eprintln!("skip: no C64 ROM dir");
        return Ok(());
    };
    let kernal = std::fs::read(rom_dir.join("kernal.rom"))?;
    let basic = std::fs::read(rom_dir.join("basic.rom"))?;
    let chargen = std::fs::read(rom_dir.join("chargen.rom"))?;

    let mut runtime = C64Runtime::new(Model::C64PalBreadbin, kernal, basic, chargen, None)?;

    let crt = autostart_hi_crt();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &crt));
    runtime.load_media(&media)?;
    // Power-cycle so the KERNAL runs its reset-time cartridge scan; the runtime
    // re-inserts the retained image into the rebuilt machine.
    runtime.reset(ResetKind::Hard);

    let mut host = null_host();
    // A few frames is ample: the cold-start prints before the first raster.
    let pal_frame_ticks: u64 = 985_248 / 50;
    runtime.run_until(MachineTime::new(20 * pal_frame_ticks), &mut host)?;

    // Screen codes for 'H' (8) and 'I' (9) should sit adjacent near the top of
    // screen RAM ($0400) once the cartridge has printed.
    let machine = runtime.machine();
    let mut found = false;
    for offset in 0..40u16 {
        if machine.memory().ram_read(0x0400 + offset) == 8
            && machine.memory().ram_read(0x0401 + offset) == 9
        {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "autostart cartridge should print 'HI' into screen RAM"
    );
    Ok(())
}
