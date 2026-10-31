//! Fixed owner for Commodore PET 2001n-32k.
pub use common_commodore_pet::PetKey;
use common_commodore_pet::{Board, owner::Configuration as BoardConfiguration};
/// Immutable physical board selection.
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const BOARD: Board = Board::Pet2001n32k;
    const MACHINE_ID: &'static str = "commodore-pet-2001n-32k";
}
/// Fixed machine with validated firmware and restored state.
pub type Pet = common_commodore_pet::owner::PetCore<Configuration>;

#[cfg(test)]
mod tests;
