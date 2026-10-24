//! An MSX2 booting real firmware: C-BIOS's MSX2 build.
//!
//! [C-BIOS](https://github.com/cbios/cbios) is a clean-room MSX BIOS under
//! a BSD licence. Its MSX2 build is three ROMs: the main ROM, the SUB-ROM
//! and a 16 KB logo ROM that sits at `$8000` in the main ROM's slot. The
//! layout is C-BIOS's own openMSX configuration (`C-BIOS_MSX2_EU`): the
//! main and logo ROMs in slot 0, the SUB-ROM in subslot 0 and a 512 KB
//! memory mapper in subslot 2 of an expanded slot 3. C-BIOS finds the
//! SUB-ROM, the mapper and the clock by probing, draws its logo in Graphic 4
//! (SCREEN 5) with the V9938's palette, then falls back to a text screen to
//! say there is no cartridge. A wrong decode of any of those shows.
//!
//! Provisioned from the corpora store; `EMU198X_ROMS_ROOT` points at the
//! firmware root, and this joins `microsoft-msx/` onto it.

use std::path::PathBuf;

use machine_msx::{MemoryMapper, Msx, MsxLayout, MsxRegion, SlotAddress, V9938Mode};

fn roms() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("microsoft-msx"))
}

fn layout() -> MsxLayout {
    MsxLayout {
        bios: SlotAddress::primary(0),
        sub_rom: Some(SlotAddress::expanded(3, 0)),
        ram: Vec::new(),
        mapper: Some(MemoryMapper {
            slot: SlotAddress::expanded(3, 2),
            size_kb: 512,
        }),
        cartridge_ports: [SlotAddress::primary(1), SlotAddress::primary(2)],
        expanded: [false, false, false, true],
    }
}

/// The number of distinct colours on screen, and the share of the screen,
/// in percent, the commonest one covers.
fn colours(machine: &Msx) -> (usize, usize) {
    let mut pixels = machine.framebuffer().to_vec();
    pixels.sort_unstable();
    let mut runs: Vec<usize> = pixels.chunk_by(|a, b| a == b).map(<[u32]>::len).collect();
    runs.sort_unstable();
    let commonest = runs.last().copied().unwrap_or(0);
    (runs.len(), commonest * 100 / pixels.len())
}

#[test]
#[ignore = "FIXTURE: needs C-BIOS at <EMU198X_ROMS_ROOT>/microsoft-msx/cbios_{main_msx2_eu,logo_msx2,sub}.rom"]
fn cbios_msx2_draws_its_logo_then_its_text_screen() {
    let Some(dir) = roms() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    let read = |name: &str| std::fs::read(dir.join(name));
    let (Ok(mut bios), Ok(logo), Ok(sub)) = (
        read("cbios_main_msx2_eu.rom"),
        read("cbios_logo_msx2.rom"),
        read("cbios_sub.rom"),
    ) else {
        emu198x_test_skip::skip!("C-BIOS MSX2 not staged in {}", dir.display());
    };
    bios.extend_from_slice(&logo);

    let mut machine = Msx::msx2(bios, sub, MsxRegion::Pal, layout());
    for _ in 0..100 {
        machine.run_frame();
    }
    let vdp = machine.v9938().expect("an MSX2 has a V9938");
    assert_eq!(vdp.mode(), V9938Mode::G4, "the logo is drawn in SCREEN 5");
    assert_eq!(vdp.register(9) & 0x02, 0x02, "the EU BIOS selects 50 Hz");
    let (count, share) = colours(&machine);
    assert!(
        count >= 10,
        "the logo's gradients need the palette; {count} colours"
    );
    assert!(
        (50..95).contains(&share),
        "the logo sits on a plain ground; it covers {share}%"
    );

    for _ in 100..300 {
        machine.run_frame();
    }
    let vdp = machine.v9938().expect("an MSX2 has a V9938");
    assert_eq!(
        vdp.mode(),
        V9938Mode::G1,
        "the message is drawn in SCREEN 1"
    );
    let (count, share) = colours(&machine);
    assert_eq!(count, 2, "text in one colour on another");
    assert!(
        (80..99).contains(&share),
        "the text covers a real part of the screen; the ground is {share}%"
    );
}
