//! Bank truth table from the TS2068 Technical Manual §§2.1.8.1, 2.1.13.5.
use common_timex_2068::{
    MemoryBus,
    memory::{MemorySource, MemoryTimex},
};
fn memory() -> MemoryTimex {
    let mut m = MemoryTimex::new();
    let mut home = vec![0x10; 16384];
    home[8192..].fill(0x11);
    m.load_rom_data(&home);
    let mut exrom = vec![0xe0; 8192];
    exrom[4096..].fill(0xe1);
    m.load_exrom_data(&exrom);
    for chunk in 2..8u16 {
        for offset in [0, 0x123, 0x1fff] {
            m.write(chunk * 8192 + offset, 0x20 + chunk as u8);
        }
    }
    m
}
#[test]
fn all_chunk_selectors_combine_with_the_external_source_bit() {
    let mut m = memory();
    for selector in 0..=255u8 {
        for exrom in [false, true] {
            m.write_f4(selector);
            m.set_exrom_enabled(exrom);
            for chunk in 0..8u16 {
                for offset in [0, 0x123, 0x1fff] {
                    let address = chunk * 8192 + offset;
                    let selected = selector & (1 << chunk) != 0;
                    let (source, value) = if selected {
                        if exrom {
                            (MemorySource::Exrom, if offset < 4096 { 0xe0 } else { 0xe1 })
                        } else {
                            (MemorySource::EmptyDock, 0xff)
                        }
                    } else if chunk < 2 {
                        (MemorySource::HomeRom, 0x10 + chunk as u8)
                    } else {
                        (MemorySource::HomeRam, 0x20 + chunk as u8)
                    };
                    assert_eq!(
                        m.mapped_source(address),
                        source,
                        "F4={selector:02x}, FF7={exrom}, addr={address:04x}"
                    );
                    assert_eq!(
                        m.read(address),
                        value,
                        "F4={selector:02x}, FF7={exrom}, addr={address:04x}"
                    );
                }
            }
        }
    }
}
#[test]
fn external_windows_are_readonly_and_do_not_change_hidden_home_ram() {
    let mut m = memory();
    for exrom in [false, true] {
        m.set_exrom_enabled(exrom);
        m.write_f4(0xff);
        for chunk in 0..8u16 {
            m.write(chunk * 8192 + 0x123, 0x99);
        }
        m.write_f4(0);
        for chunk in 0..8u16 {
            assert_eq!(
                m.read(chunk * 8192 + 0x123),
                if chunk < 2 {
                    0x10 + chunk as u8
                } else {
                    0x20 + chunk as u8
                }
            );
        }
    }
    m.set_exrom_enabled(true);
    m.write_f4(0xff);
    for chunk in 0..8u16 {
        assert_eq!(m.read(chunk * 8192 + 0x123), 0xe0);
    }
}
#[test]
fn video_fetches_keep_home_ram_when_the_cpu_selects_external_memory() {
    let mut m = memory();
    for exrom in [false, true] {
        m.set_exrom_enabled(exrom);
        m.write_f4(0xff);
        for chunk in [2, 3u16] {
            for offset in [0, 0x123, 0x1fff] {
                let address = chunk * 8192 + offset;
                assert_eq!(
                    m.read_screen(address),
                    0x20 + chunk as u8,
                    "ULA fetch {address:04x}"
                );
                assert_ne!(
                    m.read(address),
                    m.read_screen(address),
                    "CPU and ULA use different bank selects"
                );
            }
        }
    }
}
