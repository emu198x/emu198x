#![allow(dead_code)]
use common_commodore_amiga_aga::{A1200Core, AgnusRegion, Configuration, RamConfig};
#[derive(Clone, Debug)]
pub struct LegacyPal;
impl Configuration for LegacyPal {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const MACHINE_ID: &'static str = "test-a1200-pal";
    const DEFAULT_RAM: RamConfig = RamConfig::bare();
}
pub type AmigaA1200 = A1200Core<LegacyPal>;
