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
}
