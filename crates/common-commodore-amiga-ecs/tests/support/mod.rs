use common_commodore_amiga_ecs::{AgnusRegion, Configuration, EcsCore, RamConfig};
pub struct LegacyPal;
impl Configuration for LegacyPal {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const HAS_GAYLE: bool = false;
    const MACHINE_ID: &'static str = "test-ecs-pal";
    const DEFAULT_RAM: RamConfig = RamConfig::bare();
}
pub type AmigaEcs = EcsCore<LegacyPal>;
