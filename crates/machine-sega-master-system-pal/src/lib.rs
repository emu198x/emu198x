//! Fixed MasterSystemPal board over shared Sega wiring.
pub use common_sega_master_system::{
    CartridgeHeader, CartridgeMapper, CartridgeTerritory, FmUnit, IoEvent, SmsVariant, Ym2413,
    detect_mapper, normalize_cartridge,
};
use common_sega_master_system::{SmsBoard, SmsCore, SmsHardware};
/// This physical board's construction policy.
pub struct Board;
impl SmsBoard for Board {
    const HARDWARE: SmsHardware = SmsHardware::MasterSystemPal;
    const VARIANT: SmsVariant = SmsVariant::SmsPal;
}
/// This fixed physical machine.
pub type Sms = SmsCore<Board>;
