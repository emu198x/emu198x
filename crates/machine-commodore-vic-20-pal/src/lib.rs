//! Fixed VIC-20 PAL board over shared wiring.
use common_commodore_vic_20::{Board, Vic20Core, Vic20Model};
pub use common_commodore_vic_20::{Vic20Key, Vic20RamExpansion};
/// This board's fixed wiring.
pub struct PalBoard;
impl Board for PalBoard {
    const MODEL: Vic20Model = Vic20Model::Pal;
    const PAL: bool = true;
}
/// The concrete PAL machine.
pub type Vic20 = Vic20Core<PalBoard>;
