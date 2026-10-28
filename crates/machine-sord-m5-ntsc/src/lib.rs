//! Sord M5 NTSC board. Shared wiring is statically bound below.
use common_sord_m5::{Board, M5Core, M5Region, VdpRegion};
pub use common_sord_m5::{IoEvent, NUM_KEY_ROWS, VDP_INT_CTC_CHANNEL};
/// Fixed NTSC board wiring; no runtime region selector.
pub struct NtscBoard;
impl Board for NtscBoard {
    const REGION: M5Region = M5Region::Ntsc;
    const VDP_REGION: VdpRegion = VdpRegion::Ntsc;
    const PSG_CLOCK_HZ: u32 = 3_579_545;
    const TSTATES_PER_FRAME: u64 = 228 * 262;
    const FRAMEBUFFER_HEIGHT: u32 = 240;
}
/// The concrete NTSC machine.
pub type SordM5 = M5Core<NtscBoard>;
