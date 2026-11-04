//! A fixed PCB and ULA fitting, independent of RAM population and case.

/// Construction identity supplied by one concrete machine owner.
pub trait Variant48kClass: 'static {
    /// Stable physical machine identifier, shared by its RAM profiles.
    const MODEL_ID: &'static str;
    /// Exact supported PCB and fitted ULA.
    const BOARD: crate::PhysicalBoard;
    /// External A0/IORQ gate; the early Issue 1 fitting lacks it.
    const SPIDER: bool = Self::BOARD.has_spider();
}

// Isolated ULA/core tests retain their old configurable fixtures. These types
// and the revision-selecting helpers are absent from the shipping library.
#[cfg(test)]
pub(crate) trait LegacyUlaSelection: Variant48kClass {}

#[cfg(test)]
macro_rules! test_marker {
    ($name:ident, $id:literal, $board:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub(crate) struct $name;
        impl Variant48kClass for $name {
            const MODEL_ID: &'static str = $id;
            const BOARD: crate::PhysicalBoard = crate::PhysicalBoard::$board;
        }
        impl LegacyUlaSelection for $name {}
    };
}
#[cfg(test)]
test_marker!(Spectrum16kMarker, "test-spectrum-16k", Issue3);
#[cfg(test)]
test_marker!(Spectrum48kMarker, "test-spectrum-48k", Issue3);
