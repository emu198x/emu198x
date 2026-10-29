//! Fixed Agb board over shared Game Boy wiring.
//! CGB compatibility execution only; this is not a full GBA emulator.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct AgbBoard;
impl GameBoyBoard for AgbBoard {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Agb;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::Agb;
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<AgbBoard>;
