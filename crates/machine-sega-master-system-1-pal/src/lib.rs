//! Fixed MasterSystem1Pal board over shared Sega wiring.
pub use common_sega_master_system::{
    CartridgeHeader, CartridgeMapper, CartridgeTerritory, FmUnit, IoEvent, SmsVariant, Ym2413,
    detect_mapper, normalize_cartridge,
};
use common_sega_master_system::{SmsBoard, SmsCore, SmsHardware};
/// This physical board's construction policy.
pub struct Board;
impl SmsBoard for Board {
    const HARDWARE: SmsHardware = SmsHardware::MasterSystem1Pal;
    const VARIANT: SmsVariant = SmsVariant::Sms1Pal;
}
/// This fixed physical machine.
pub type Sms = SmsCore<Board>;
