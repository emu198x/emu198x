//! Concrete BBC ModelB motherboard.
use common_acorn_bbc_micro::{BbcCore, Board, M6502};
pub use common_acorn_bbc_micro::{BbcModel, Disk};
/// Fixed ModelB board wiring.
pub struct ModelBBoard;
impl Board for ModelBBoard {
    const MODEL: BbcModel = BbcModel::ModelB;
    const EXTRA_RAM_LEN: usize = 0;
    const SIDEWAYS_RAM: u16 = 0;
    fn new_cpu() -> M6502 {
        M6502::new()
    }
}
/// This board's machine.
pub type BbcMicro = BbcCore<ModelBBoard>;
