//! Fixed Sgb board over shared Game Boy wiring.
//! Game Boy execution only; full SNES/SGB hardware remains #326.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct SgbBoard;
impl GameBoyBoard for SgbBoard {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Sgb;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::Sgb;
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<SgbBoard>;
