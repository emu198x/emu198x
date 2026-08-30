use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_msx::{MapperType, Model, MsxRuntime};

fn mapped_rom(addresses: &[u16]) -> Vec<u8> {
    let mut rom = vec![0; 128 * 1024];
    for (index, address) in addresses.iter().copied().enumerate() {
        let offset = index * 3;
        rom[offset] = 0x32;
        rom[offset + 1..offset + 3].copy_from_slice(&address.to_le_bytes());
    }
    rom
}

#[test]
fn standard_media_path_detects_megarom_mapper() {
    let mut runtime =
        MsxRuntime::new(Model::Msx1Ntsc, vec![0; 32 * 1024]).expect("runtime should construct");
    let rom = mapped_rom(&[0x5000, 0x9000, 0xb000]);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));

    runtime.load_media(&media).expect("cartridge should load");
    assert_eq!(
        runtime.machine().expect("machine").cart1_mapper(),
        MapperType::KonamiScc
    );
}

#[test]
fn explicit_mapper_selection_remains_an_override() {
    let mut runtime =
        MsxRuntime::new(Model::Msx1Ntsc, vec![0; 32 * 1024]).expect("runtime should construct");
    let rom = mapped_rom(&[0x5000, 0x9000, 0xb000]);

    runtime.insert_cartridge1(rom, MapperType::Ascii16);
    assert_eq!(
        runtime.machine().expect("machine").cart1_mapper(),
        MapperType::Ascii16
    );
}
