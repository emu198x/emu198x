//! Concrete BBC Master128 motherboard.
use common_acorn_bbc_micro::{BbcCore, Board, M6502};
pub use common_acorn_bbc_micro::{BbcModel, Disk};
/// Fixed Master128 board wiring.
pub struct Master128Board;
impl Board for Master128Board {
    const MODEL: BbcModel = BbcModel::Master128;
    const EXTRA_RAM_LEN: usize = 32768;
    const SIDEWAYS_RAM: u16 = 240;
    fn new_cpu() -> M6502 {
        M6502::new_65c12()
    }
}
/// This board's machine.
pub type BbcMicro = BbcCore<Master128Board>;
