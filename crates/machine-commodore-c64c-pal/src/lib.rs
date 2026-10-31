//! Fixed owner for the implemented commodore-c64c-pal configuration.
use common_commodore_c64::{C64Model, board::Board};
/// Immutable board selection.
#[derive(Clone)]
pub struct Configuration;
impl Board for Configuration {
    const MODEL: C64Model = C64Model::PalC64c;
    const MACHINE_ID: &'static str = "commodore-c64c-pal";
}
/// Fixed machine; firmware and supported peripherals remain configurable.
pub type C64 = common_commodore_c64::board::C64Core<Configuration>;

#[cfg(test)]
mod tests;
