//! Shared snapshot types for ZX Spectrum format parsers.
//!
//! Both `format-sinclair-zx-spectrum-z80` (.z80) and `format-sinclair-zx-spectrum-sna`
//! (.sna) parsers produce the same `Snapshot` value. Future snapshot formats land here
//! too. The previous name `Z80Snapshot` was misleading — the type represents Spectrum
//! machine state, not Z80-file-format state.

/// Parsed Spectrum snapshot — machine-agnostic representation of the captured state.
///
/// Produced by every snapshot format parser; consumed by every machine crate via
/// `apply_snapshot`-style methods.
#[derive(Clone, Debug)]
pub struct Snapshot {
    // Z80 registers
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub af_alt: u16,
    pub bc_alt: u16,
    pub de_alt: u16,
    pub hl_alt: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub im: u8,
    pub iff1: bool,
    pub iff2: bool,

    /// Border colour (0-7).
    pub border: u8,

    /// Hardware model.
    pub model: SnapshotModel,

    /// Port $7FFD value (128K paging state). 0 for 48K snapshots.
    pub port_7ffd: u8,
    /// Port $1FFD value (+2A/+3 paging). 0 if not applicable.
    pub port_1ffd: u8,
    /// Port $FFFD value (AY register select). 0 if not applicable.
    pub ay_register: u8,
    /// AY register contents (16 bytes).
    pub ay_regs: [u8; 16],

    /// Memory pages: (page_number, 16384 bytes).
    /// Page numbering: 0-7 = RAM banks, 8 = ROM 0, etc.
    /// For 48K v1 snapshots: pages 5, 2, 0 (= $4000, $8000, $C000).
    pub pages: Vec<(u8, Vec<u8>)>,
}

/// Which Spectrum model the snapshot targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotModel {
    Spectrum48K,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus3,
    Pentagon128,
    Scorpion256,
}
