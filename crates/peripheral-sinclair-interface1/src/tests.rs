use super::*;

const MDR: u16 = 0x00E7;
const CONTROL: u16 = 0x00EF;
const COMMS: u16 = 0x00F7;

/// A stand-in for the shadow ROM: each byte is the low byte of its
/// address plus one, so a read shows where it came from.
fn interface() -> Interface1 {
    let rom: Vec<u8> = (0..INTERFACE1_ROM_BYTES)
        .map(|i| (i as u8).wrapping_add(1))
        .collect();
    let mut if1 = Interface1::new();
    if1.attach(&rom).expect("an 8 KB ROM attaches");
    if1
}

/// Ten sectors where every byte is its offset within the sector, plus
/// the sector number, so any byte read back can be placed.
fn image(write_protected: bool) -> MdrImage {
    let mut bytes: Vec<u8> = (0..10 * SECTOR_LEN)
        .map(|i| ((i % SECTOR_LEN) as u8).wrapping_add((i / SECTOR_LEN) as u8))
        .collect();
    bytes.push(u8::from(write_protected));
    MdrImage::parse(&bytes).expect("ten sectors")
}

/// Shifts one bit along the drive chain: `on` starts drive 1's motor.
fn clock_in(if1: &mut Interface1, on: bool) {
    let data = u8::from(!on);
    if1.write(CONTROL, 0x02 | data);
    if1.write(CONTROL, data);
}

/// Polls status, as the ROM does, until the drive reports the sync
/// before a block.
fn find_block(if1: &mut Interface1) {
    for _ in 0..64 {
        if if1.read(CONTROL) & 0x06 == 0 {
            return;
        }
    }
    panic!("no block found");
}

/// Reads past the block under the head, whatever its length.
fn skip_block(if1: &mut Interface1) {
    if1.read(CONTROL);
    for _ in 0..DATA_BLOCK_BYTES {
        if1.read(MDR);
    }
}

#[test]
fn the_rom_must_be_8k() {
    let mut if1 = Interface1::new();
    assert_eq!(if1.attach(&[0; 10]), Err(Interface1RomError { actual: 10 }));
    assert!(!if1.is_attached());
    assert!(
        Interface1RomError { actual: 10 }
            .to_string()
            .contains("8192-byte")
    );
}

#[test]
fn an_unplugged_interface_neither_pages_nor_claims_ports() {
    let mut if1 = Interface1::new();
    if1.on_m1(0x0008);
    assert!(!if1.is_paged());
    assert!(!if1.claims_port(MDR));
}

#[test]
fn the_error_restart_pages_the_rom_in_before_its_fetch() {
    let mut if1 = interface();
    assert_eq!(if1.read_memory(0x0008), None);
    if1.on_m1(0x0008);
    assert!(if1.is_paged());
    assert_eq!(if1.read_memory(0x0008), Some(0x09));
    assert_eq!(if1.read_memory(0x2008), Some(0x09), "mirrored at $2000");
    assert_eq!(if1.read_memory(0x4000), None);
}

#[test]
fn close_pages_in_too_and_other_fetches_do_not() {
    let mut if1 = interface();
    if1.on_m1(0x0009);
    if1.on_m1(0x1707);
    assert!(!if1.is_paged());
    if1.on_m1(0x1708);
    assert!(if1.is_paged());
}

#[test]
fn the_rom_pages_out_after_the_fetch_from_0700() {
    let mut if1 = interface();
    if1.on_m1(0x0008);
    if1.on_m1(0x0700);
    assert!(if1.is_paged(), "the opcode at $0700 is the shadow ROM's");
    if1.after_m1(0x0700);
    assert!(!if1.is_paged());
}

#[test]
fn the_shadow_rom_ignores_writes() {
    let mut if1 = interface();
    if1.on_m1(0x0008);
    assert!(if1.write_memory(0x1000, 0x55));
    assert_eq!(if1.read_memory(0x1000), Some(0x01));
    assert!(!if1.write_memory(0x5000, 0x55));
}

#[test]
fn ports_decode_on_a3_and_a4() {
    let if1 = interface();
    for port in [MDR, CONTROL, COMMS, 0x0000, 0xFF08, 0x1234 & !0x0018] {
        assert!(if1.claims_port(port), "{port:#06x}");
    }
    for port in [0x00FE, 0x001F, 0x7FFD, 0x00FF] {
        assert!(!if1.claims_port(port), "{port:#06x}");
    }
}

#[test]
fn the_idle_status_has_dtr_and_busy_low() {
    let mut if1 = interface();
    assert_eq!(if1.read(CONTROL), 0xE7);
    if1.dtr = true;
    if1.busy = true;
    assert_eq!(if1.read(CONTROL), 0xFF);
}

#[test]
fn the_comms_port_reads_its_input_lines() {
    let mut if1 = interface();
    assert_eq!(if1.read(COMMS), 0x7E, "idle TxD and net read low");
    if1.tx = true;
    if1.net_in = true;
    assert_eq!(if1.read(COMMS), 0xFF);
    if1.write(COMMS, 0x01);
    assert!(if1.net_out());
    if1.write(CONTROL, 0x10);
    assert!(if1.cts());
}

#[test]
fn the_drive_chain_shifts_on_the_falling_clock() {
    let mut if1 = interface();
    clock_in(&mut if1, true);
    assert!(if1.motor_on(0));
    clock_in(&mut if1, false);
    assert!(!if1.motor_on(0));
    assert!(if1.motor_on(1), "drive 2 is now selected");
    for _ in 0..6 {
        clock_in(&mut if1, false);
    }
    assert!(if1.motor_on(7));
    clock_in(&mut if1, false);
    assert!((0..MICRODRIVE_COUNT).all(|d| !if1.motor_on(d)));
}

#[test]
fn a_running_drive_reads_a_header_then_a_data_block() {
    let mut if1 = interface();
    let image = image(false);
    assert!(if1.insert(0, &image));
    clock_in(&mut if1, true);
    find_block(&mut if1);
    let sector = image.sector(0).expect("sector 0");
    let header: Vec<u8> = (0..HEADER_LEN).map(|_| if1.read(MDR)).collect();
    assert_eq!(header, sector[..HEADER_LEN]);
    assert_eq!(if1.read(MDR), sector[HEADER_LEN - 1], "the block is spent");

    find_block(&mut if1);
    let data: Vec<u8> = (0..DATA_BLOCK_BYTES).map(|_| if1.read(MDR)).collect();
    assert_eq!(data, sector[HEADER_LEN..]);

    find_block(&mut if1);
    assert_eq!(if1.read(MDR), image.sector(1).expect("sector 1")[0]);
}

#[test]
fn the_gap_reads_high_before_the_sync() {
    let mut if1 = interface();
    if1.insert(0, &image(false));
    clock_in(&mut if1, true);
    for _ in 0..GAP_SYNC_POLLS {
        assert_eq!(if1.read(CONTROL) & 0x06, 0x06, "in the gap");
    }
    assert_eq!(if1.read(CONTROL) & 0x06, 0x00, "at the sync");
}

#[test]
fn the_tape_is_a_loop() {
    let mut if1 = interface();
    let image = image(false);
    if1.insert(0, &image);
    clock_in(&mut if1, true);
    for _ in 0..image.sector_count() * 2 {
        skip_block(&mut if1);
    }
    find_block(&mut if1);
    let header: Vec<u8> = (0..HEADER_LEN).map(|_| if1.read(MDR)).collect();
    assert_eq!(header, image.sector(0).expect("sector 0")[..HEADER_LEN]);
}

#[test]
fn a_stopped_drive_reads_nothing() {
    let mut if1 = interface();
    if1.insert(0, &image(false));
    assert_eq!(if1.read(MDR), 0xFF);
    assert_eq!(if1.read(CONTROL) & 0x07, 0x07);
}

#[test]
fn write_protect_reads_low_on_bit_0() {
    let mut if1 = interface();
    if1.insert(0, &image(true));
    clock_in(&mut if1, true);
    assert_eq!(if1.read(CONTROL) & 0x01, 0x00);
}

#[test]
fn a_blank_cartridge_shows_no_blocks_until_written() {
    let mut if1 = interface();
    assert!(if1.insert_blank(0, 10));
    clock_in(&mut if1, true);
    for _ in 0..40 {
        assert_eq!(if1.read(CONTROL) & 0x06, 0x06);
    }

    // The ROM writes a preamble of ten $00 and two $FF, then the block.
    for _ in 0..10 {
        if1.write(MDR, 0x00);
    }
    if1.write(MDR, 0xFF);
    if1.write(MDR, 0xFF);
    let header: Vec<u8> = (1..=HEADER_LEN as u8).collect();
    for &byte in &header {
        if1.write(MDR, byte);
    }
    assert!(if1.cartridge_modified(0));

    // Once round the loop, the block is found and reads back.
    for _ in 0..10 * 2 - 1 {
        skip_block(&mut if1);
    }
    find_block(&mut if1);
    let read: Vec<u8> = (0..HEADER_LEN).map(|_| if1.read(MDR)).collect();
    assert_eq!(read, header);

    let saved = if1.eject(0).expect("a cartridge");
    assert_eq!(saved.sector(0).expect("sector 0")[..HEADER_LEN], header);
    assert!(if1.eject(0).is_none());
}

#[test]
fn drives_outside_the_chain_are_refused() {
    let mut if1 = interface();
    assert!(!if1.insert(8, &image(false)));
    assert!(!if1.insert_blank(8, 10));
    assert!(if1.cartridge(8).is_none());
    assert!(!if1.cartridge_modified(8));
}

#[test]
fn reset_stops_the_motors_and_pages_out_but_keeps_the_cartridges() {
    let mut if1 = interface();
    if1.insert(3, &image(false));
    clock_in(&mut if1, true);
    if1.on_m1(0x0008);
    if1.reset();
    assert!(!if1.is_paged());
    assert!(!if1.motor_on(0));
    assert_eq!(if1.cartridge(3), Some(image(false)));
    assert!(!if1.cartridge_modified(3));
}
