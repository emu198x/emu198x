//! Fixed Amiga a500-rev6a NTSC configuration over shared wiring.
use common_commodore_amiga_ocs::{AgnusRegion, Configuration, OcsBoard, OcsCore};
#[derive(Clone, Copy, Debug)]
pub struct Board;
impl Configuration for Board {
    const REGION: AgnusRegion = AgnusRegion::Ntsc;
    const BOARD: OcsBoard = OcsBoard::A500Rev6A;
    const MACHINE_ID: &'static str = "commodore-amiga-a500-rev6a-ntsc";
}
pub type AmigaA500Rev6A = OcsCore<Board>;

#[cfg(test)]
mod tests;
