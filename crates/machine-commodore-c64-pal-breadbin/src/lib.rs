//! Fixed owner for the implemented commodore-c64-pal-breadbin configuration.
use common_commodore_c64::{C64Model, board::Board};
/// Immutable board selection.
#[derive(Clone)]
pub struct Configuration;
impl Board for Configuration {
    const MODEL: C64Model = C64Model::PalBreadbin;
    const MACHINE_ID: &'static str = "commodore-c64-pal-breadbin";
}
/// Fixed machine; firmware and supported peripherals remain configurable.
pub type C64 = common_commodore_c64::board::C64Core<Configuration>;

#[cfg(test)]
mod tests;
