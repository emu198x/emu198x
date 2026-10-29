//! ColecoVision NTSC board; shared wiring is statically bound here.
use common_coleco_colecovision::{Board, CvCore, CvRegion, VdpRegion};
pub use common_coleco_colecovision::{CvController, IoEvent, KeypadKey};
/// Fixed NTSC board wiring.
pub struct NtscBoard;
impl Board for NtscBoard {
    const REGION: CvRegion = CvRegion::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
/// This concrete board's machine.
pub type ColecoVision = CvCore<NtscBoard>;
