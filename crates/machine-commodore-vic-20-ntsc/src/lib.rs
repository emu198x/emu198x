//! Fixed VIC-20 NTSC board over shared wiring.
use common_commodore_vic_20::{Board, Vic20Core, Vic20Model};
pub use common_commodore_vic_20::{Vic20Key, Vic20RamExpansion};
/// This board's fixed wiring.
pub struct NtscBoard;
impl Board for NtscBoard {
    const MODEL: Vic20Model = Vic20Model::Ntsc;
    const PAL: bool = false;
}
/// The concrete NTSC machine.
pub type Vic20 = Vic20Core<NtscBoard>;
