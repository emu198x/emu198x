//! Original 8 KiB PET 2001, assembly 320008 with 6540 ROMs and 6550 RAM.
pub use common_commodore_pet::PetKey;
use common_commodore_pet::{Board, owner::Configuration as BoardConfiguration};
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const BOARD: Board = Board::Pet2001Original8k;
    const MACHINE_ID: &'static str = "commodore-pet-2001-320008";
}
pub type Pet = common_commodore_pet::owner::PetCore<Configuration>;

#[cfg(test)]
mod tests;
