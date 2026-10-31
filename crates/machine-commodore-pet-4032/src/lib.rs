//! Fixed owner for Commodore PET 4032.
pub use common_commodore_pet::PetKey;
use common_commodore_pet::{Board, owner::Configuration as BoardConfiguration};
/// Immutable physical board selection.
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const BOARD: Board = Board::Pet4032;
    const MACHINE_ID: &'static str = "commodore-pet-4032";
}
/// Fixed machine with validated firmware and restored state.
pub type Pet = common_commodore_pet::owner::PetCore<Configuration>;

#[cfg(test)]
mod tests;
