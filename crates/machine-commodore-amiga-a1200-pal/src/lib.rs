//! Fixed Amiga A1200 PAL configuration; individual PCB populations remain unqualified.
use common_commodore_amiga_aga::Configuration as BoardConfiguration;
pub use common_commodore_amiga_aga::{
    AgnusRegion, AmigaA1200Snapshot, FB_HEIGHT, FB_WIDTH, RamConfig,
};
/// Fixed board identity and oscillator region.
#[derive(Clone, Debug)]
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const MACHINE_ID: &'static str = "commodore-amiga-a1200-pal";
}
/// Concrete regional A1200 owner.
pub type AmigaA1200 = common_commodore_amiga_aga::A1200Core<Configuration>;

#[cfg(test)]
mod tests;
