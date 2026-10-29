//! Fixed Cgb board over shared Game Boy wiring.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct CgbBoard;
impl GameBoyBoard for CgbBoard {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Cgb;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::Cgb;
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<CgbBoard>;
