//! Sord M5 PAL board. Shared wiring is statically bound below.
use common_sord_m5::{Board, M5Core, M5Region, VdpRegion};
pub use common_sord_m5::{IoEvent, NUM_KEY_ROWS, VDP_INT_CTC_CHANNEL};
/// Fixed PAL board wiring; no runtime region selector.
pub struct PalBoard;
impl Board for PalBoard {
    const REGION: M5Region = M5Region::Pal;
    const VDP_REGION: VdpRegion = VdpRegion::Pal;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 313;
    const FRAMEBUFFER_HEIGHT: u32 = 288;
}
/// The concrete PAL machine.
pub type SordM5 = M5Core<PalBoard>;
