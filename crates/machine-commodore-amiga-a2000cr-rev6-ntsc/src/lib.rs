//! Fixed Amiga a2000cr-rev6 NTSC configuration over shared wiring.
use common_commodore_amiga_ocs::{AgnusRegion, Configuration, OcsBoard, OcsCore};
#[derive(Clone, Copy, Debug)]
pub struct Board;
impl Configuration for Board {
    const REGION: AgnusRegion = AgnusRegion::Ntsc;
    const BOARD: OcsBoard = OcsBoard::A2000CrRev6;
    const MACHINE_ID: &'static str = "commodore-amiga-a2000cr-rev6-ntsc";
}
pub type AmigaA2000CrRev6 = OcsCore<Board>;

#[cfg(test)]
mod tests;
