//! ColecoVision PAL board; shared wiring is statically bound here.
use common_coleco_colecovision::{Board, CvCore, CvRegion, VdpRegion};
pub use common_coleco_colecovision::{CvController, IoEvent, KeypadKey};
/// Fixed PAL board wiring.
pub struct PalBoard;
impl Board for PalBoard {
    const REGION: CvRegion = CvRegion::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}
/// This concrete board's machine.
pub type ColecoVision = CvCore<PalBoard>;
