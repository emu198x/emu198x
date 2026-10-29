//! Concrete BBC BPlus motherboard.
use common_acorn_bbc_micro::{BbcCore, Board, M6502};
pub use common_acorn_bbc_micro::{BbcModel, Disk};
/// Fixed BPlus board wiring.
pub struct BPlusBoard;
impl Board for BPlusBoard {
    const MODEL: BbcModel = BbcModel::ModelBPlus;
    const EXTRA_RAM_LEN: usize = 32768;
    const SIDEWAYS_RAM: u16 = 0;
    fn new_cpu() -> M6502 {
        M6502::new()
    }
}
/// This board's machine.
pub type BbcMicro = BbcCore<BPlusBoard>;
