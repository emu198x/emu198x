//! Dump bytes around specific Kickstart addresses we care about.

use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    // Kickstart 1.3 is 256KB at $FC0000-$FFFFFF.
    let rom_base = 0x00FC_0000u32;

    let targets = [0x00FC_0F90u32, 0x00FC_0F80, 0x00FC_0FA0, 0x00FC_0F70];
    for addr in targets {
        let offset = (addr - rom_base) as usize;
        print!("{addr:08X}:");
        for i in 0..32 {
            print!(" {:02X}", kickstart[offset + i]);
        }
        println!();
    }

    // Also dump trackdisk BeginIO region and the Unit pointer address we
    // saw in the trace.
    let more = [0x00FE_9C3Eu32, 0x00FE_9C60, 0x00FE_9C80, 0x00FE_9CA0, 0x00FE_9CC0];
    for addr in more {
        let offset = (addr - rom_base) as usize;
        print!("{addr:08X}:");
        for i in 0..32 {
            print!(" {:02X}", kickstart[offset + i]);
        }
        println!();
    }
}
