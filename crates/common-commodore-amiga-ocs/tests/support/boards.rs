use engine::{A530Config, AgnusRegion, Configuration, OcsBoard, OcsCore, RamConfig};
#[derive(Clone, Copy, Debug)]
pub struct Pal;
impl Configuration for Pal {
 const REGION: AgnusRegion = AgnusRegion::Pal;
 const BOARD: OcsBoard = OcsBoard::A500Rev5;
 const MACHINE_ID: &'static str = "test-Pal";
}
#[derive(Clone, Copy, Debug)]
pub struct Ntsc;
impl Configuration for Ntsc {
 const REGION: AgnusRegion = AgnusRegion::Ntsc;
 const BOARD: OcsBoard = OcsBoard::A500Rev5;
 const MACHINE_ID: &'static str = "test-Ntsc";
}
#[derive(Clone, Copy, Debug)]
pub struct FatPal;
impl Configuration for FatPal {
 const REGION: AgnusRegion = AgnusRegion::Pal;
 const BOARD: OcsBoard = OcsBoard::A500Rev6A;
 const MACHINE_ID: &'static str = "test-FatPal";
}
#[derive(Clone, Copy, Debug)]
pub struct FatNtsc;
impl Configuration for FatNtsc {
 const REGION: AgnusRegion = AgnusRegion::Ntsc;
 const BOARD: OcsBoard = OcsBoard::A500Rev6A;
 const MACHINE_ID: &'static str = "test-FatNtsc";
}
#[derive(Clone, Copy, Debug)]
pub struct A1000Pal;
impl Configuration for A1000Pal {
 const REGION: AgnusRegion = AgnusRegion::Pal;
 const BOARD: OcsBoard = OcsBoard::A1000;
 const MACHINE_ID: &'static str = "test-A1000Pal";
}
#[derive(Clone, Copy, Debug)]
pub struct A1000Ntsc;
impl Configuration for A1000Ntsc {
 const REGION: AgnusRegion = AgnusRegion::Ntsc;
 const BOARD: OcsBoard = OcsBoard::A1000;
 const MACHINE_ID: &'static str = "test-A1000Ntsc";
}
#[derive(Clone, Copy, Debug)]
pub struct A2000Pal;
impl Configuration for A2000Pal {
 const REGION: AgnusRegion = AgnusRegion::Pal;
 const BOARD: OcsBoard = OcsBoard::A2000CrRev6;
 const MACHINE_ID: &'static str = "test-A2000Pal";
}
#[derive(Clone, Copy, Debug)]
pub struct A2000Ntsc;
impl Configuration for A2000Ntsc {
 const REGION: AgnusRegion = AgnusRegion::Ntsc;
 const BOARD: OcsBoard = OcsBoard::A2000CrRev6;
 const MACHINE_ID: &'static str = "test-A2000Ntsc";
}
pub type AmigaOcs = OcsCore<Pal>;
pub trait LegacyConstructors {
 fn with_ram_config_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<Ntsc>;
 fn with_fat_agnus_ram_config(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<FatPal>;
 fn with_fat_agnus_ram_config_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<FatNtsc>;
 fn with_a1000_bootstrap_rom(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<A1000Pal>;
 fn with_a1000_bootstrap_rom_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<A1000Ntsc>;
 fn with_gvp_a530_config_ntsc(rom: Vec<u8>, cfg: RamConfig, a530: A530Config) -> OcsCore<Ntsc>;
}
impl LegacyConstructors for AmigaOcs {
 fn with_ram_config_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<Ntsc> { OcsCore::with_ram_config(rom, cfg) }
 fn with_fat_agnus_ram_config(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<FatPal> { OcsCore::with_ram_config(rom, cfg) }
 fn with_fat_agnus_ram_config_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<FatNtsc> { OcsCore::with_ram_config(rom, cfg) }
 fn with_a1000_bootstrap_rom(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<A1000Pal> { OcsCore::with_ram_config(rom, cfg) }
 fn with_a1000_bootstrap_rom_ntsc(rom: Vec<u8>, cfg: RamConfig) -> OcsCore<A1000Ntsc> { OcsCore::with_ram_config(rom, cfg) }
 fn with_gvp_a530_config_ntsc(rom: Vec<u8>, cfg: RamConfig, a530: A530Config) -> OcsCore<Ntsc> { OcsCore::with_gvp_a530_config(rom, cfg, a530) }
}
