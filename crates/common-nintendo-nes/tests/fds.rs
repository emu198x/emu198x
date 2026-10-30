//! The Famicom Disk System RAM adapter in the machine: a synthetic BIOS
//! reads a disk side through the disk IRQ. No Nintendo BIOS or commercial
//! disk image is needed.

use common_nintendo_nes::Nes;
use format_nintendo_famicom_fds::{DISK_SIGNATURE, SIDE_LEN};
use nintendo_nes_cartridge::{DiskDrive, Fds};

/// A side holding only a disk info block.
fn side() -> Vec<u8> {
    let mut side = vec![0x01];
    side.extend_from_slice(&DISK_SIGNATURE);
    side.resize(56, 0x55);
    side.resize(SIDE_LEN, 0);
    side
}

/// A BIOS that starts the drive in read mode with the disk IRQ on, then
/// stores the first 16 bytes the IRQ delivers at `$0300`.
fn bios() -> Vec<u8> {
    let mut bios = vec![0xEA; 8192];
    let program = [
        0xA9, 0x40, // LDA #$40
        0x8D, 0x17, 0x40, // STA $4017: no APU frame IRQ
        0xA9, 0xC5, // LDA #$C5: IRQ on, ready, read, motor on
        0x8D, 0x25, 0x40, // STA $4025
        0xA2, 0x00, // LDX #0
        0x58, // CLI
        0x4C, 0x0D, 0xE0, // JMP $E00D
        // IRQ at $E010
        0xAD, 0x31, 0x40, // LDA $4031 (acknowledges)
        0xE0, 0x10, // CPX #16
        0xB0, 0x04, // BCS +4
        0x9D, 0x00, 0x03, // STA $0300,X
        0xE8, // INX
        0x40, // RTI
    ];
    bios[..program.len()].copy_from_slice(&program);
    bios[0x1FFA..].copy_from_slice(&[0x1B, 0xE0, 0x00, 0xE0, 0x10, 0xE0]);
    bios
}

#[test]
fn a_bios_reads_the_disk_info_block_through_the_disk_irq() {
    let mut fds = Fds::new(&bios(), &[side()]);
    fds.insert(0);
    let mut nes = Nes::new(Box::new(fds));
    // Spin-up and the lead-in gap take about 580,000 CPU cycles.
    for _ in 0..25 {
        nes.run_frame();
    }
    let read: Vec<u8> = (0..16).map(|i| nes.peek(0x0300 + i)).collect();
    assert_eq!(read[0], 0x01, "the block type byte comes first");
    assert_eq!(&read[1..15], &DISK_SIGNATURE);
    assert_eq!(read[15], 0x55);
}

#[test]
fn without_a_disk_no_irq_arrives() {
    let mut nes = Nes::new(Box::new(Fds::new(&bios(), &[side()])));
    for _ in 0..25 {
        nes.run_frame();
    }
    assert_eq!(nes.peek(0x0300), 0);
    assert_eq!(nes.peek(0x4032) & 0x01, 0x01, "the drive reports no disk");
}
