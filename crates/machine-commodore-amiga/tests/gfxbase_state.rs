//! Sample GfxBase fields at frame 250 in both configs to find what
//! differs between working (slow-RAM) and broken (chip-only).
//!
//! GfxBase address (= A1 in the VBL handler at $FC6D6C):
//!   - chip-only: $0000221E
//!   - slow-RAM:  $00C01E1E
//!
//! Per AmigaOS includes/graphics/gfxbase.h, GfxBase has these fields:
//!   +$22  ActiView      struct View *  ← the routine in $FCD4CC checks this!
//!   +$26  copinit       struct copinit *
//!   +$2A  cia           UWORD *
//!   +$2E  blitter       APTR
//!   +$32  LOFlist       UWORD *  ← cause of the bug
//!   +$36  SHFlist       UWORD *
//!   +$3A  blthd         struct bltnode *
//!   +$3E  bltht         struct bltnode *
//!   +$42  bsblthd       struct bltnode *
//!   +$46  bsbltht       struct bltnode *
//!   +$4A  vbsrv         struct Interrupt
//!   +$5C  timsrv        struct Interrupt
//!   +$6E  bltsrv        struct Interrupt
//!   +$80  TextFonts     struct List
//!   +$8E  DefaultFont   struct TextFont *
//!   +$92  Modes         UWORD
//!   +$94  VBlank        BYTE
//!   +$95  Debug         BYTE
//!   +$96  BeamSync      WORD
//!   +$98  system_bplcon0 WORD
//!   +$9A  SpriteReserved UBYTE
//!   +$9B  bytereserved  UBYTE
//!   +$9C  Flags         UWORD

use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

fn rom() -> Vec<u8> {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    std::fs::read(&path).expect("read kick13.rom")
}

fn read_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(amiga.memory.read_word(addr)) << 16)
        | u32::from(amiga.memory.read_word(addr.wrapping_add(2)))
}

fn read_word(amiga: &Amiga, addr: u32) -> u16 {
    amiga.memory.read_word(addr)
}

fn dump_gfxbase(label: &str, amiga: &Amiga, gfxbase: u32) {
    eprintln!("===== {label}: GfxBase = ${gfxbase:08X} =====");
    eprintln!("  +$22 ActiView      = ${:08X}  {}",
        read_long(amiga, gfxbase + 0x22),
        if read_long(amiga, gfxbase + 0x22) == 0 { "← NULL" } else { "" });
    eprintln!("  +$26 copinit       = ${:08X}", read_long(amiga, gfxbase + 0x26));
    eprintln!("  +$2A cia           = ${:08X}", read_long(amiga, gfxbase + 0x2A));
    eprintln!("  +$2E blitter       = ${:08X}", read_long(amiga, gfxbase + 0x2E));
    eprintln!("  +$32 LOFlist       = ${:08X}", read_long(amiga, gfxbase + 0x32));
    eprintln!("  +$36 SHFlist       = ${:08X}", read_long(amiga, gfxbase + 0x36));
    eprintln!("  +$3A blthd         = ${:08X}", read_long(amiga, gfxbase + 0x3A));
    eprintln!("  +$92 Modes         = ${:04X}", read_word(amiga, gfxbase + 0x92));
    eprintln!("  +$94 VBlank        = ${:02X}", read_word(amiga, gfxbase + 0x94) >> 8);
    eprintln!("  +$95 Debug         = ${:02X}", read_word(amiga, gfxbase + 0x94) & 0xFF);
    eprintln!("  +$98 system_bplcon0= ${:04X}", read_word(amiga, gfxbase + 0x98));
    eprintln!("  +$9C Flags         = ${:04X}", read_word(amiga, gfxbase + 0x9C));

    // If ActiView is non-NULL, dump it.
    let actiview = read_long(amiga, gfxbase + 0x22);
    if actiview != 0 && actiview < 0x100_0000 {
        eprintln!("  ActiView (struct View *) at ${actiview:08X}:");
        eprintln!("    +$0 ViewPort     = ${:08X}", read_long(amiga, actiview + 0x0));
        eprintln!("    +$4 LOFCprList   = ${:08X}", read_long(amiga, actiview + 0x4));
        eprintln!("    +$8 SHFCprList   = ${:08X}", read_long(amiga, actiview + 0x8));
        eprintln!("    +$C DyOffset     = ${:04X}", read_word(amiga, actiview + 0xC));
        eprintln!("    +$E DxOffset     = ${:04X}", read_word(amiga, actiview + 0xE));
        eprintln!("    +$10 Modes       = ${:04X}", read_word(amiga, actiview + 0x10));

        // If LOFCprList is non-NULL, dump it.
        let loflist = read_long(amiga, actiview + 0x4);
        if loflist != 0 && loflist < 0x100_0000 {
            eprintln!("  LOFCprList (struct cprlist *) at ${loflist:08X}:");
            eprintln!("    +$0 Next        = ${:08X}", read_long(amiga, loflist + 0x0));
            eprintln!("    +$4 start       = ${:08X}", read_long(amiga, loflist + 0x4));
            eprintln!("    +$8 MaxCount    = ${:04X}", read_word(amiga, loflist + 0x8));
        }
    }
    eprintln!();
}

#[test]
#[ignore]
fn dump_gfxbase_state_chip_only() {
    let mut amiga = Amiga::new(rom());
    for _ in 0..250 { amiga.run_frame(); }
    dump_gfxbase("chip-only", &amiga, 0x0000_221E);
}

#[test]
#[ignore]
fn dump_gfxbase_state_with_slow_ram() {
    let mut amiga = Amiga::new_with_slow_ram(rom(), 512 * 1024);
    for _ in 0..250 { amiga.run_frame(); }
    dump_gfxbase("slow-RAM", &amiga, 0x00C0_1E1E);
}
