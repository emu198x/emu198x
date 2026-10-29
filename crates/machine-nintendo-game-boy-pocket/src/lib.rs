//! Fixed Mgb board over shared Game Boy wiring.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct MgbBoard;
impl GameBoyBoard for MgbBoard {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Mgb;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::Mgb;
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<MgbBoard>;
