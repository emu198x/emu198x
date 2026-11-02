//! Fixed Amiga a1000 PAL configuration over shared wiring.
use common_commodore_amiga_ocs::{AgnusRegion, Configuration, OcsBoard, OcsCore};
#[derive(Clone, Copy, Debug)]
pub struct Board;
impl Configuration for Board {
    const REGION: AgnusRegion = AgnusRegion::Pal;
    const BOARD: OcsBoard = OcsBoard::A1000;
    const MACHINE_ID: &'static str = "commodore-amiga-a1000-pal";
}
pub type AmigaA1000 = OcsCore<Board>;

#[cfg(test)]
mod tests;
