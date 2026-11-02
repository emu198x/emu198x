//! Fixed Amiga a500-rev5 PAL configuration over shared wiring.
use common_commodore_amiga_ocs::{AgnusRegion, Configuration, OcsBoard, OcsCore};
#[derive(Clone, Copy, Debug)]
pub struct Board;
impl Configuration for Board {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const BOARD: OcsBoard = OcsBoard::A500Rev5;
    const MACHINE_ID: &'static str = "commodore-amiga-a500-rev5-pal";
}
pub type AmigaA500Rev5 = OcsCore<Board>;

#[cfg(test)]
mod tests;
