//! Bank-switched cartridges run to a verifiable state.
//!
//! Each image carries a `CART` header naming its scheme, a marker byte at
//! offset `$100` of every bank, and a small program that walks the banks
//! and copies each marker into RAM at `$0600` onwards. The machine boots
//! cart-only with no OS ROM, so execution starts at the cartridge base.

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

/// The start of the RAM the programs report into.
const REPORT: u16 = 0x0600;

fn with_header(type_id: u32, rom: Vec<u8>) -> Vec<u8> {
    let mut image = b"CART".to_vec();
    image.extend_from_slice(&type_id.to_be_bytes());
    image.extend_from_slice(&[0; 8]);
    image.extend_from_slice(&rom);
    image
}

/// `banks` banks of `bank_len` bytes, each holding its number at `$100`.
fn banked_rom(banks: usize, bank_len: usize) -> Vec<u8> {
    let mut rom = vec![0xFF; banks * bank_len];
    for bank in 0..banks {
        rom[bank * bank_len + 0x100] = bank as u8;
    }
    rom
}

fn run(image: Vec<u8>) -> Atari800xl {
    let mut machine = Atari800xl::new(None, None, Some(image), Atari800xlRegion::Ntsc, false)
        .expect("cart-only boot");
    for _ in 0..2 {
        machine.run_frame();
    }
    machine
}

fn report(machine: &Atari800xl, len: u16) -> Vec<u8> {
    (0..len).map(|i| machine.peek(REPORT + i)).collect()
}

/// XEGS 32 KB: four 8 KB banks, the last fixed at `$A000`. The program in
/// the fixed bank writes each bank number to `$D500` and reads the marker
/// through the `$8000` window; bank 0 starts with a jump up to it.
#[test]
fn xegs_cart_walks_its_banks_through_the_lower_window() {
    let mut rom = banked_rom(4, 0x2000);
    rom[0..3].copy_from_slice(&[0x4C, 0x00, 0xA0]); // jmp $A000
    let program: [u8; 20] = [
        0xA2, 0x00, // ldx #0
        0x8E, 0x00, 0xD5, // loop: stx $D500
        0xAD, 0x00, 0x81, // lda $8100
        0x9D, 0x00, 0x06, // sta $0600,x
        0xE8, // inx
        0xE0, 0x04, // cpx #4
        0xD0, 0xF2, // bne loop
        0x4C, 0x10, 0xA0, // done: jmp done
        0xEA,
    ];
    rom[0x6000..0x6000 + program.len()].copy_from_slice(&program);

    let machine = run(with_header(12, rom));
    assert_eq!(report(&machine, 4), vec![0, 1, 2, 3]);
    // The fixed bank is still there whatever the lower window shows.
    assert_eq!(machine.peek(0xA100), 3);
    assert_eq!(machine.peek(0x8100), 3);
}

/// MegaCart 64 KB: four 16 KB banks filling `$8000-$BFFF`. Every bank
/// carries the same program at `$8000`, so switching under the running
/// code lands on the same instruction in the next bank.
#[test]
fn megacart_walks_its_banks_under_the_running_program() {
    let mut rom = banked_rom(4, 0x4000);
    let program: [u8; 22] = [
        0xA6, 0x80, // ldx $80
        0xAD, 0x00, 0x81, // lda $8100
        0x9D, 0x00, 0x06, // sta $0600,x
        0xE8, // inx
        0x86, 0x80, // stx $80
        0xE0, 0x04, // cpx #4
        0xF0, 0x07, // beq done
        0x8A, // txa
        0x8D, 0x00, 0xD5, // sta $D500
        0x4C, 0x00, 0x80, // jmp $8000
              // done: jmp done lives at $8016 (see below)
    ];
    for bank in 0..4 {
        let base = bank * 0x4000;
        rom[base..base + program.len()].copy_from_slice(&program);
        rom[base + 0x16..base + 0x19].copy_from_slice(&[0x4C, 0x16, 0x80]);
    }

    let machine = run(with_header(28, rom));
    assert_eq!(report(&machine, 4), vec![0, 1, 2, 3]);
    assert_eq!(machine.peek(0x8100), 3);
}

/// OSS one-chip 16 KB: bank 0 fixed at `$B000`, the `$A000` window picked
/// by the address touched in `$D5xx`. Switching the cartridge off removes
/// both windows, so the fixed bank copies the program to RAM first; the
/// program then reads each select address, copies the marker, and finally
/// switches the cartridge off so RAM shows through.
#[test]
fn oss_cart_selects_banks_by_control_address_and_switches_off() {
    let mut rom = banked_rom(4, 0x1000);
    let mut program = Vec::new();
    for (i, select) in [0xD500u16, 0xD509, 0xD501, 0xD508].iter().enumerate() {
        program.extend_from_slice(&[0xAD, (*select & 0xFF) as u8, 0xD5]); // lda select
        program.extend_from_slice(&[0xAD, 0x00, 0xA1]); // lda $A100
        program.extend_from_slice(&[0x8D, i as u8, 0x06]); // sta $0600+i
    }
    let done = 0x0700 + program.len() as u16;
    program.extend_from_slice(&[0x4C, (done & 0xFF) as u8, (done >> 8) as u8]);
    let len = u8::try_from(program.len()).expect("short program");
    // Bank 0 sits at $B000; its marker is at $B100, the program body at $B200.
    rom[0x200..0x200 + program.len()].copy_from_slice(&program);
    let stub: [u8; 19] = [
        0x4C, 0x03, 0xB0, // jmp $B003
        0xA2, 0x00, // ldx #0
        0xBD, 0x00, 0xB2, // copy: lda $B200,x
        0x9D, 0x00, 0x07, // sta $0700,x
        0xE8, // inx
        0xE0, len, // cpx #len
        0xD0, 0xF5, // bne copy
        0x4C, 0x00, 0x07, // jmp $0700
    ];
    rom[..stub.len()].copy_from_slice(&stub);

    let mut machine = Atari800xl::new(
        None,
        None,
        Some(with_header(15, rom)),
        Atari800xlRegion::Ntsc,
        false,
    )
    .expect("cart-only boot");
    // What the window shows once the cartridge steps aside.
    machine.poke(0xA100, 0xEE);
    for _ in 0..2 {
        machine.run_frame();
    }
    assert_eq!(report(&machine, 4), vec![1, 2, 3, 0xEE]);
    // The fixed bank went with it.
    assert_eq!(machine.peek(0xB003), 0);
}
