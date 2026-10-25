//! A cartridge's ROMs, in the layout each chip reads them.

/// A Neo Geo cartridge: one ROM region per bus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cartridge {
    /// P-ROM: the 68000's program, big-endian, the first megabyte at
    /// `$000000` and the rest banked into `$200000`.
    pub p: Vec<u8>,
    /// S-ROM: fix-layer tiles.
    pub s: Vec<u8>,
    /// M-ROM: the Z80's program.
    pub m: Vec<u8>,
    /// V-ROM read by ADPCM-A, and by ADPCM-B too when `v_b` is empty.
    pub v_a: Vec<u8>,
    /// V-ROM read by ADPCM-B, on boards that give it its own.
    pub v_b: Vec<u8>,
    /// C-ROM: sprite tiles, each pair byte-interleaved, odd ROM in even
    /// bytes.
    pub c: Vec<u8>,
}

impl Cartridge {
    /// The big-endian P-ROM word at `address` (rounded down to even), or
    /// `$FFFF` past the end.
    #[must_use]
    pub fn p_word(&self, address: u32) -> u16 {
        super::word_at(&self.p, address)
    }
}
