//! Fixed Amiga A500-PLUS PAL board configuration.
use common_commodore_amiga_ecs::Configuration as BoardConfiguration;
pub use common_commodore_amiga_ecs::{
    AgnusRegion, AmigaEcsSnapshot, FB_HEIGHT, FB_WIDTH, RamConfig,
};
/// Fixed region and onboard hardware fitting.
#[derive(Clone, Debug)]
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const HAS_GAYLE: bool = false;
    const MACHINE_ID: &'static str = "commodore-amiga-a500-plus-pal";
}
/// Concrete fixed board owner.
pub type AmigaA500Plus = common_commodore_amiga_ecs::EcsCore<Configuration>;

#[cfg(test)]
mod tests;
