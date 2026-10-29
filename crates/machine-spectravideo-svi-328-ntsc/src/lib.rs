//! SVI-328 NTSC board; shared wiring is statically bound here.
use common_spectravideo_svi_328::{Board, SviCore, SviRegion, VdpRegion};
pub use common_spectravideo_svi_328::{IoEvent, NUM_KEY_ROWS};
/// Fixed NTSC board wiring.
pub struct NtscBoard;
impl Board for NtscBoard {
    const REGION: SviRegion = SviRegion::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
/// This concrete board's machine.
pub type Svi328 = SviCore<NtscBoard>;
