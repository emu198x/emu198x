//! Fixed Dmg board over shared Game Boy wiring.
pub use common_nintendo_game_boy::{ApuChannel, AudioControls, BootProfile};
use common_nintendo_game_boy::{GameBoyBoard, GameBoyCore, GameBoyHardware};
/// This board's construction policy.
pub struct DmgBoard;
impl GameBoyBoard for DmgBoard {
    const HARDWARE: GameBoyHardware = GameBoyHardware::Dmg;
    const DEFAULT_BOOT_PROFILE: BootProfile = BootProfile::DmgAbc;
    fn accepts_profile(profile: BootProfile) -> bool {
        matches!(profile, BootProfile::Dmg0 | BootProfile::DmgAbc)
    }
}
/// This fixed board's Game Boy execution machine.
pub type GameBoy = GameBoyCore<DmgBoard>;
