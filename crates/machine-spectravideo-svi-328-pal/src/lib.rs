//! SVI-328 PAL board; shared wiring is statically bound here.
use common_spectravideo_svi_328::{Board, SviCore, SviRegion, VdpRegion};
pub use common_spectravideo_svi_328::{IoEvent, NUM_KEY_ROWS};
/// Fixed PAL board wiring.
pub struct PalBoard;
impl Board for PalBoard {
    const REGION: SviRegion = SviRegion::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}
/// This concrete board's machine.
pub type Svi328 = SviCore<PalBoard>;
