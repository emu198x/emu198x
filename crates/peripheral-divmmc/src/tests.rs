use super::*;

const CONTROL: u16 = 0x00E3;
const CARD_SELECT: u16 = 0x00E7;
const SPI: u16 = 0x00EB;

/// A stand-in for the EEPROM: each byte is the low byte of its address
/// plus `$80`, so a read shows where it came from.
fn interface() -> DivMmc {
    let eeprom: Vec<u8> = (0..DIVMMC_PAGE_BYTES)
        .map(|i| (i as u8).wrapping_add(0x80))
        .collect();
    let mut divmmc = DivMmc::new();
    divmmc.attach(&eeprom).expect("an 8 KB EEPROM attaches");
    divmmc
}

/// Fills each RAM bank with its own number, through CONMEM.
fn number_the_banks(divmmc: &mut DivMmc) {
    for bank in 0..DIVMMC_RAM_BANKS as u8 {
        divmmc.write(CONTROL, CONMEM | bank);
        divmmc.write_memory(0x2000, bank);
    }
    divmmc.write(CONTROL, 0);
}

#[test]
fn the_eeprom_must_be_8k() {
    let mut divmmc = DivMmc::new();
    assert_eq!(divmmc.attach(&[0; 3]), Err(DivMmcEepromError { actual: 3 }));
    assert!(!divmmc.is_attached());
    assert!(
        DivMmcEepromError { actual: 3 }
            .to_string()
            .contains("8192-byte")
    );
}

#[test]
fn an_unplugged_interface_does_nothing() {
    let mut divmmc = DivMmc::new();
    assert!(!divmmc.claims_port(CONTROL));
    assert!(!divmmc.claims_read(SPI));
    divmmc.after_m1(0x0000);
    assert!(!divmmc.is_paged());
}

#[test]
fn the_ports_decode_the_low_byte_and_only_spi_reads() {
    let divmmc = interface();
    for port in [CONTROL, CARD_SELECT, SPI, 0x12E3, 0xFFEB] {
        assert!(divmmc.claims_port(port), "{port:#06x}");
    }
    assert!(!divmmc.claims_port(0x00E5));
    assert!(divmmc.claims_read(0x34EB));
    assert!(!divmmc.claims_read(CONTROL));
    assert!(!divmmc.claims_read(CARD_SELECT));
}

#[test]
fn entry_points_page_in_after_their_fetch() {
    for trap in [0x0000, 0x0008, 0x0038, 0x0066, 0x04C6, 0x0562] {
        let mut divmmc = interface();
        divmmc.on_m1(trap);
        assert!(!divmmc.is_paged(), "{trap:#06x}: not before the fetch");
        divmmc.after_m1(trap);
        assert!(divmmc.is_paged(), "{trap:#06x}");
    }
}

#[test]
fn the_3dxx_page_pages_in_before_its_fetch() {
    let mut divmmc = interface();
    divmmc.on_m1(0x3D2F);
    assert!(divmmc.is_paged());
    assert_eq!(divmmc.read_memory(0x0000), Some(0x80), "the EEPROM");
}

#[test]
fn the_1ff8_block_pages_out_after_its_fetch() {
    let mut divmmc = interface();
    divmmc.after_m1(0x0000);
    divmmc.on_m1(0x1FFB);
    assert!(divmmc.is_paged());
    divmmc.after_m1(0x1FFB);
    assert!(!divmmc.is_paged());
    divmmc.after_m1(0x1FF7);
    assert!(!divmmc.is_paged(), "$1FF7 is no trap");
}

#[test]
fn other_fetches_change_nothing() {
    let mut divmmc = interface();
    for addr in [0x0001, 0x0067, 0x3C00, 0x3E00, 0x2000] {
        divmmc.on_m1(addr);
        divmmc.after_m1(addr);
    }
    assert!(!divmmc.is_paged());
}

#[test]
fn automapping_needs_the_jumper_or_mapram() {
    let mut divmmc = interface();
    divmmc.eeprom_write_protect = false;
    divmmc.after_m1(0x0000);
    assert!(!divmmc.is_paged(), "an unprotected EEPROM is being flashed");
    divmmc.write(CONTROL, MAPRAM);
    assert!(divmmc.is_paged(), "MAPRAM allows it again");
}

#[test]
fn conmem_pages_in_regardless() {
    let mut divmmc = interface();
    divmmc.write(CONTROL, CONMEM);
    assert!(divmmc.is_paged());
    divmmc.write(CONTROL, 0);
    assert!(!divmmc.is_paged());
}

#[test]
fn conmem_maps_the_eeprom_and_a_writable_bank() {
    let mut divmmc = interface();
    number_the_banks(&mut divmmc);
    divmmc.write(CONTROL, CONMEM | 3);
    assert_eq!(divmmc.read_memory(0x0010), Some(0x90));
    assert_eq!(divmmc.read_memory(0x2000), Some(3));
    assert!(divmmc.write_memory(0x0010, 0x11));
    assert_eq!(
        divmmc.read_memory(0x0010),
        Some(0x90),
        "the jumper protects it"
    );
    divmmc.eeprom_write_protect = false;
    divmmc.write_memory(0x0010, 0x11);
    assert_eq!(divmmc.eeprom()[0x10], 0x11, "flashed");
    assert_eq!(divmmc.read_memory(0x4000), None);
    assert!(!divmmc.write_memory(0x4000, 0));
}

#[test]
fn the_bank_number_wraps_at_sixteen() {
    let mut divmmc = interface();
    number_the_banks(&mut divmmc);
    divmmc.write(CONTROL, CONMEM | 0x3F);
    assert_eq!(divmmc.read_memory(0x2000), Some(15));
}

#[test]
fn mapram_maps_bank_3_read_only_at_0000() {
    let mut divmmc = interface();
    number_the_banks(&mut divmmc);
    divmmc.write(CONTROL, MAPRAM | 3);
    divmmc.after_m1(0x0000);
    assert_eq!(divmmc.read_memory(0x0000), Some(3));
    assert_eq!(divmmc.read_memory(0x2000), Some(3));
    divmmc.write_memory(0x0000, 0x55);
    divmmc.write_memory(0x2000, 0x55);
    assert_eq!(divmmc.read_memory(0x2000), Some(3), "bank 3 is read-only");
    divmmc.write(CONTROL, MAPRAM | 2);
    divmmc.write_memory(0x2000, 0x55);
    assert_eq!(divmmc.read_memory(0x2000), Some(0x55));
    assert_eq!(divmmc.ram_bank(2).expect("bank 2")[0], 0x55);
}

#[test]
fn without_conmem_or_mapram_the_eeprom_is_read_only() {
    let mut divmmc = interface();
    divmmc.eeprom_write_protect = false;
    divmmc.write(CONTROL, MAPRAM);
    divmmc.write(CONTROL, 0);
    assert_eq!(divmmc.control(), MAPRAM, "MAPRAM cannot be cleared");
    let mut plain = interface();
    plain.after_m1(0x0000);
    assert!(plain.write_memory(0x0001, 0));
    assert_eq!(plain.read_memory(0x0001), Some(0x81));
}

#[test]
fn reset_keeps_only_mapram() {
    let mut divmmc = interface();
    divmmc.write(CONTROL, CONMEM | MAPRAM | 5);
    divmmc.reset();
    assert_eq!(divmmc.control(), MAPRAM);
    assert!(!divmmc.is_paged());
}

#[test]
fn the_card_answers_only_when_selected() {
    let mut divmmc = interface();
    divmmc.card.insert(vec![0; SD_IMAGE_UNIT]).expect("512 KB");
    for byte in [0x40, 0, 0, 0, 0, 0x95] {
        divmmc.write(SPI, byte);
    }
    assert_eq!(divmmc.read(SPI), 0xFF, "no card selected");
    divmmc.write(CARD_SELECT, 0xFE);
    for byte in [0x40, 0, 0, 0, 0, 0x95] {
        divmmc.write(SPI, byte);
    }
    assert_eq!(divmmc.read(SPI), 0x01, "idle");
    divmmc.write(CARD_SELECT, 0xFF);
    assert_eq!(divmmc.read(SPI), 0xFF);
    assert_eq!(divmmc.read(CONTROL), 0xFF);
}
