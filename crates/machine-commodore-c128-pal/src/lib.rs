//! Fixed owner for the implemented commodore-c128-pal configuration.
use common_commodore_c128::{C128Model, board::Board};
/// Immutable board selection.
#[derive(Clone)]
pub struct Configuration;
impl Board for Configuration {
    const MODEL: C128Model = C128Model::Pal;
    const MACHINE_ID: &'static str = "commodore-c128-pal";
}
/// Fixed machine; firmware and supported peripherals remain configurable.
pub type C128 = common_commodore_c128::board::C128Core<Configuration>;

#[cfg(test)]
mod tests;
