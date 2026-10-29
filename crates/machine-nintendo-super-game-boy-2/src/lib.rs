//! Fixed Sgb2 board over shared Game Boy wiring.
//! Game Boy execution only; full SNES/SGB hardware remains #326.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct Sgb2Board;
impl GameBoyBoard for Sgb2Board {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Sgb2;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::Sgb2;
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<Sgb2Board>;
