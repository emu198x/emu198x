//! Spectrum-family machine catalogue.
//!
//! The `Model` enum enumerates every variant the crate covers; `profile_for`
//! builds the `MachineProfile` descriptor for one; `profiles()` returns the
//! full catalogue. Runtime behaviour lives in `runtime`, `board_runtime` and
//! `variants`; this module owns only metadata.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, FirmwareSource, MachineId,
    MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy, known_capability,
};

/// Supported Spectrum family models.
///
/// Every variant has a concrete machine owner and a `MachineCore` wrapper.
/// Fixed 48K-class boards share the rich runtime query and ROM-glyph decoding
/// machinery; the remaining boards use their family-class implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// ZX Spectrum Issue 1 (16K, PAL).
    SpectrumIssue1Ram16kPal,
    /// ZX Spectrum Issue 1 (48K, PAL).
    SpectrumIssue1Ram48kPal,
    /// ZX Spectrum Issue 2 (16K, PAL).
    SpectrumIssue2Ram16kPal,
    /// ZX Spectrum Issue 2 (48K, PAL).
    SpectrumIssue2Ram48kPal,
    /// ZX Spectrum Issue 3 (16K, PAL).
    SpectrumIssue3Ram16kPal,
    /// ZX Spectrum Issue 3 (48K, PAL).
    SpectrumIssue3Ram48kPal,
    /// ZX Spectrum Issue 3B (16K, PAL).
    SpectrumIssue3BRam16kPal,
    /// ZX Spectrum Issue 3B (48K, PAL).
    SpectrumIssue3BRam48kPal,
    /// ZX Spectrum Issue 4A (16K, PAL).
    SpectrumIssue4ARam16kPal,
    /// ZX Spectrum Issue 4A (48K, PAL).
    SpectrumIssue4ARam48kPal,
    /// ZX Spectrum Issue 4B (16K, PAL).
    SpectrumIssue4BRam16kPal,
    /// ZX Spectrum Issue 4B (48K, PAL).
    SpectrumIssue4BRam48kPal,
    /// ZX Spectrum Issue 5 / 6C001E-6 (16K, PAL).
    SpectrumIssue5Ula6c001e6Ram16kPal,
    /// ZX Spectrum Issue 5 / 6C001E-6 (48K, PAL).
    SpectrumIssue5Ula6c001e6Ram48kPal,
    /// ZX Spectrum Issue 5 / 6C001E-7 (16K, PAL).
    SpectrumIssue5Ula6c001e7Ram16kPal,
    /// ZX Spectrum Issue 5 / 6C001E-7 (48K, PAL).
    SpectrumIssue5Ula6c001e7Ram48kPal,
    /// ZX Spectrum Issue 6A / Ferranti (16K, PAL).
    SpectrumIssue6ARam16kPal,
    /// ZX Spectrum Issue 6A / Ferranti (48K, PAL).
    SpectrumIssue6ARam48kPal,
    /// ZX Spectrum 128K (PAL).
    Spectrum128KPal,
    /// ZX Spectrum +2 (PAL).
    SpectrumPlus2,
    /// ZX Spectrum +2A (PAL).
    SpectrumPlus2A,
    /// ZX Spectrum +2B (PAL).
    SpectrumPlus2B,
    /// ZX Spectrum +3 (PAL).
    SpectrumPlus3,
    /// Pentagon 128.
    Pentagon128,
    /// Scorpion ZS-256.
    ScorpionZS256,
    /// Timex TC2048.
    TimexTC2048,
    /// Timex TC2068.
    TimexTC2068,
    /// Timex TS2068 (NTSC).
    TimexTS2068,
}

impl Model {
    /// Physical owner for the 48K-class boards; RAM and case do not identify a PCB.
    #[must_use]
    pub const fn physical_board(
        self,
    ) -> Option<common_sinclair_zx_spectrum_48k_class::PhysicalBoard> {
        use common_sinclair_zx_spectrum_48k_class::PhysicalBoard;
        match self {
            Self::SpectrumIssue1Ram16kPal => Some(PhysicalBoard::Issue1),
            Self::SpectrumIssue1Ram48kPal => Some(PhysicalBoard::Issue1),
            Self::SpectrumIssue2Ram16kPal => Some(PhysicalBoard::Issue2),
            Self::SpectrumIssue2Ram48kPal => Some(PhysicalBoard::Issue2),
            Self::SpectrumIssue3Ram16kPal => Some(PhysicalBoard::Issue3),
            Self::SpectrumIssue3Ram48kPal => Some(PhysicalBoard::Issue3),
            Self::SpectrumIssue3BRam16kPal => Some(PhysicalBoard::Issue3B),
            Self::SpectrumIssue3BRam48kPal => Some(PhysicalBoard::Issue3B),
            Self::SpectrumIssue4ARam16kPal => Some(PhysicalBoard::Issue4A),
            Self::SpectrumIssue4ARam48kPal => Some(PhysicalBoard::Issue4A),
            Self::SpectrumIssue4BRam16kPal => Some(PhysicalBoard::Issue4B),
            Self::SpectrumIssue4BRam48kPal => Some(PhysicalBoard::Issue4B),
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => Some(PhysicalBoard::Issue5Ula6c001e6),
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => Some(PhysicalBoard::Issue5Ula6c001e6),
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => Some(PhysicalBoard::Issue5Ula6c001e7),
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => Some(PhysicalBoard::Issue5Ula6c001e7),
            Self::SpectrumIssue6ARam16kPal => Some(PhysicalBoard::Issue6A),
            Self::SpectrumIssue6ARam48kPal => Some(PhysicalBoard::Issue6A),
            _ => None,
        }
    }

    /// Fitted RAM for a classic physical board profile.
    #[must_use]
    pub const fn ram_kib(self) -> Option<u16> {
        match self {
            Self::SpectrumIssue1Ram16kPal => Some(16),
            Self::SpectrumIssue1Ram48kPal => Some(48),
            Self::SpectrumIssue2Ram16kPal => Some(16),
            Self::SpectrumIssue2Ram48kPal => Some(48),
            Self::SpectrumIssue3Ram16kPal => Some(16),
            Self::SpectrumIssue3Ram48kPal => Some(48),
            Self::SpectrumIssue3BRam16kPal => Some(16),
            Self::SpectrumIssue3BRam48kPal => Some(48),
            Self::SpectrumIssue4ARam16kPal => Some(16),
            Self::SpectrumIssue4ARam48kPal => Some(48),
            Self::SpectrumIssue4BRam16kPal => Some(16),
            Self::SpectrumIssue4BRam48kPal => Some(48),
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => Some(16),
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => Some(48),
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => Some(16),
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => Some(48),
            Self::SpectrumIssue6ARam16kPal => Some(16),
            Self::SpectrumIssue6ARam48kPal => Some(48),
            _ => None,
        }
    }

    /// Stable model identifier for this model.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::SpectrumIssue1Ram16kPal => "sinclair-zx-spectrum-issue1",
            Self::SpectrumIssue1Ram48kPal => "sinclair-zx-spectrum-issue1",
            Self::SpectrumIssue2Ram16kPal => "sinclair-zx-spectrum-issue2",
            Self::SpectrumIssue2Ram48kPal => "sinclair-zx-spectrum-issue2",
            Self::SpectrumIssue3Ram16kPal => "sinclair-zx-spectrum-issue3",
            Self::SpectrumIssue3Ram48kPal => "sinclair-zx-spectrum-issue3",
            Self::SpectrumIssue3BRam16kPal => "sinclair-zx-spectrum-issue3b",
            Self::SpectrumIssue3BRam48kPal => "sinclair-zx-spectrum-issue3b",
            Self::SpectrumIssue4ARam16kPal => "sinclair-zx-spectrum-issue4a",
            Self::SpectrumIssue4ARam48kPal => "sinclair-zx-spectrum-issue4a",
            Self::SpectrumIssue4BRam16kPal => "sinclair-zx-spectrum-issue4b",
            Self::SpectrumIssue4BRam48kPal => "sinclair-zx-spectrum-issue4b",
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => "sinclair-zx-spectrum-issue5-6c001e6",
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => "sinclair-zx-spectrum-issue5-6c001e6",
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => "sinclair-zx-spectrum-issue5-6c001e7",
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => "sinclair-zx-spectrum-issue5-6c001e7",
            Self::SpectrumIssue6ARam16kPal => "sinclair-zx-spectrum-issue6a",
            Self::SpectrumIssue6ARam48kPal => "sinclair-zx-spectrum-issue6a",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k",
            Self::SpectrumPlus2 => "sinclair-zx-spectrum-plus2",
            Self::SpectrumPlus2A => "sinclair-zx-spectrum-plus2a",
            Self::SpectrumPlus2B => "sinclair-zx-spectrum-plus2b",
            Self::SpectrumPlus3 => "sinclair-zx-spectrum-plus3",
            Self::Pentagon128 => "pentagon-128",
            Self::ScorpionZS256 => "scorpion-zs256",
            Self::TimexTC2048 => "timex-tc2048",
            Self::TimexTC2068 => "timex-tc2068",
            Self::TimexTS2068 => "timex-ts2068",
        }
    }

    /// Stable profile identifier for this model.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::SpectrumIssue1Ram16kPal => "sinclair-zx-spectrum-issue1-16k-pal",
            Self::SpectrumIssue1Ram48kPal => "sinclair-zx-spectrum-48k-issue1-pal",
            Self::SpectrumIssue2Ram16kPal => "sinclair-zx-spectrum-issue2-16k-pal",
            Self::SpectrumIssue2Ram48kPal => "sinclair-zx-spectrum-48k-issue2-pal",
            Self::SpectrumIssue3Ram16kPal => "sinclair-zx-spectrum-issue3-16k-pal",
            Self::SpectrumIssue3Ram48kPal => "sinclair-zx-spectrum-issue3-48k-pal",
            Self::SpectrumIssue3BRam16kPal => "sinclair-zx-spectrum-issue3b-16k-pal",
            Self::SpectrumIssue3BRam48kPal => "sinclair-zx-spectrum-issue3b-48k-pal",
            Self::SpectrumIssue4ARam16kPal => "sinclair-zx-spectrum-issue4a-16k-pal",
            Self::SpectrumIssue4ARam48kPal => "sinclair-zx-spectrum-issue4a-48k-pal",
            Self::SpectrumIssue4BRam16kPal => "sinclair-zx-spectrum-issue4b-16k-pal",
            Self::SpectrumIssue4BRam48kPal => "sinclair-zx-spectrum-issue4b-48k-pal",
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => {
                "sinclair-zx-spectrum-issue5-6c001e6-16k-pal"
            }
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => {
                "sinclair-zx-spectrum-issue5-6c001e6-48k-pal"
            }
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => {
                "sinclair-zx-spectrum-issue5-6c001e7-16k-pal"
            }
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => {
                "sinclair-zx-spectrum-issue5-6c001e7-48k-pal"
            }
            Self::SpectrumIssue6ARam16kPal => "sinclair-zx-spectrum-issue6a-16k-pal",
            Self::SpectrumIssue6ARam48kPal => "sinclair-zx-spectrum-issue6a-48k-pal",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k-pal",
            Self::SpectrumPlus2 => "sinclair-zx-spectrum-plus2-pal",
            Self::SpectrumPlus2A => "sinclair-zx-spectrum-plus2a-pal",
            Self::SpectrumPlus2B => "sinclair-zx-spectrum-plus2b-pal",
            Self::SpectrumPlus3 => "sinclair-zx-spectrum-plus3-pal",
            Self::Pentagon128 => "pentagon-128-pal",
            Self::ScorpionZS256 => "scorpion-zs256-pal",
            Self::TimexTC2048 => "timex-tc2048-pal",
            Self::TimexTC2068 => "timex-tc2068-pal",
            Self::TimexTS2068 => "timex-ts2068-ntsc",
        }
    }

    /// User-facing display name for this model.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SpectrumIssue1Ram16kPal => "ZX Spectrum Issue 1 (16K, PAL)",
            Self::SpectrumIssue1Ram48kPal => "ZX Spectrum Issue 1 (48K, PAL)",
            Self::SpectrumIssue2Ram16kPal => "ZX Spectrum Issue 2 (16K, PAL)",
            Self::SpectrumIssue2Ram48kPal => "ZX Spectrum Issue 2 (48K, PAL)",
            Self::SpectrumIssue3Ram16kPal => "ZX Spectrum Issue 3 (16K, PAL)",
            Self::SpectrumIssue3Ram48kPal => "ZX Spectrum Issue 3 (48K, PAL)",
            Self::SpectrumIssue3BRam16kPal => "ZX Spectrum Issue 3B (16K, PAL)",
            Self::SpectrumIssue3BRam48kPal => "ZX Spectrum Issue 3B (48K, PAL)",
            Self::SpectrumIssue4ARam16kPal => "ZX Spectrum Issue 4A (16K, PAL)",
            Self::SpectrumIssue4ARam48kPal => "ZX Spectrum Issue 4A (48K, PAL)",
            Self::SpectrumIssue4BRam16kPal => "ZX Spectrum Issue 4B (16K, PAL)",
            Self::SpectrumIssue4BRam48kPal => "ZX Spectrum Issue 4B (48K, PAL)",
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => "ZX Spectrum Issue 5 / 6C001E-6 (16K, PAL)",
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => "ZX Spectrum Issue 5 / 6C001E-6 (48K, PAL)",
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => "ZX Spectrum Issue 5 / 6C001E-7 (16K, PAL)",
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => "ZX Spectrum Issue 5 / 6C001E-7 (48K, PAL)",
            Self::SpectrumIssue6ARam16kPal => "ZX Spectrum Issue 6A / Ferranti (16K, PAL)",
            Self::SpectrumIssue6ARam48kPal => "ZX Spectrum Issue 6A / Ferranti (48K, PAL)",
            Self::Spectrum128KPal => "ZX Spectrum 128K (PAL)",
            Self::SpectrumPlus2 => "ZX Spectrum +2 (PAL)",
            Self::SpectrumPlus2A => "ZX Spectrum +2A (PAL)",
            Self::SpectrumPlus2B => "ZX Spectrum +2B (PAL)",
            Self::SpectrumPlus3 => "ZX Spectrum +3 (PAL)",
            Self::Pentagon128 => "Pentagon 128",
            Self::ScorpionZS256 => "Scorpion ZS-256",
            Self::TimexTC2048 => "Timex TC2048",
            Self::TimexTC2068 => "Timex TC2068",
            Self::TimexTS2068 => "Timex TS2068 (NTSC)",
        }
    }

    /// Every model in catalogue order: the SOLID 8 (16K → 48K → + →
    /// 128K → +2 → +2A → +2B → +3) then the five exotics (Pentagon →
    /// Scorpion → TC2048 → TC2068 → TS2068). Stable order matters for
    /// the window's variant menu.
    pub const ALL: [Self; 28] = [
        Self::SpectrumIssue1Ram16kPal,
        Self::SpectrumIssue1Ram48kPal,
        Self::SpectrumIssue2Ram16kPal,
        Self::SpectrumIssue2Ram48kPal,
        Self::SpectrumIssue3Ram16kPal,
        Self::SpectrumIssue3Ram48kPal,
        Self::SpectrumIssue3BRam16kPal,
        Self::SpectrumIssue3BRam48kPal,
        Self::SpectrumIssue4ARam16kPal,
        Self::SpectrumIssue4ARam48kPal,
        Self::SpectrumIssue4BRam16kPal,
        Self::SpectrumIssue4BRam48kPal,
        Self::SpectrumIssue5Ula6c001e6Ram16kPal,
        Self::SpectrumIssue5Ula6c001e6Ram48kPal,
        Self::SpectrumIssue5Ula6c001e7Ram16kPal,
        Self::SpectrumIssue5Ula6c001e7Ram48kPal,
        Self::SpectrumIssue6ARam16kPal,
        Self::SpectrumIssue6ARam48kPal,
        Self::Spectrum128KPal,
        Self::SpectrumPlus2,
        Self::SpectrumPlus2A,
        Self::SpectrumPlus2B,
        Self::SpectrumPlus3,
        Self::Pentagon128,
        Self::ScorpionZS256,
        Self::TimexTC2048,
        Self::TimexTC2068,
        Self::TimexTS2068,
    ];

    /// Every variant id, in [`Self::ALL`] order.
    pub const VARIANT_IDS: [&'static str; 28] = [
        "spectrum_issue1_16k",
        "spectrum_48k_issue1",
        "spectrum_issue2_16k",
        "spectrum_48k_issue2",
        "spectrum_issue3_16k",
        "spectrum_issue3_48k",
        "spectrum_issue3b_16k",
        "spectrum_issue3b_48k",
        "spectrum_issue4a_16k",
        "spectrum_issue4a_48k",
        "spectrum_issue4b_16k",
        "spectrum_issue4b_48k",
        "spectrum_issue5_6c001e6_16k",
        "spectrum_issue5_6c001e6_48k",
        "spectrum_issue5_6c001e7_16k",
        "spectrum_issue5_6c001e7_48k",
        "spectrum_issue6a_16k",
        "spectrum_issue6a_48k",
        "spectrum_128k",
        "spectrum_plus2",
        "spectrum_plus2a",
        "spectrum_plus2b",
        "spectrum_plus3",
        "pentagon_128",
        "scorpion_zs256",
        "timex_tc2048",
        "timex_tc2068",
        "timex_ts2068",
    ];

    /// The snake-case id `set_machine` steps, the `--machine` flag and the
    /// window's variant menu use for this model.
    #[must_use]
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::SpectrumIssue1Ram16kPal => "spectrum_issue1_16k",
            Self::SpectrumIssue1Ram48kPal => "spectrum_48k_issue1",
            Self::SpectrumIssue2Ram16kPal => "spectrum_issue2_16k",
            Self::SpectrumIssue2Ram48kPal => "spectrum_48k_issue2",
            Self::SpectrumIssue3Ram16kPal => "spectrum_issue3_16k",
            Self::SpectrumIssue3Ram48kPal => "spectrum_issue3_48k",
            Self::SpectrumIssue3BRam16kPal => "spectrum_issue3b_16k",
            Self::SpectrumIssue3BRam48kPal => "spectrum_issue3b_48k",
            Self::SpectrumIssue4ARam16kPal => "spectrum_issue4a_16k",
            Self::SpectrumIssue4ARam48kPal => "spectrum_issue4a_48k",
            Self::SpectrumIssue4BRam16kPal => "spectrum_issue4b_16k",
            Self::SpectrumIssue4BRam48kPal => "spectrum_issue4b_48k",
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => "spectrum_issue5_6c001e6_16k",
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => "spectrum_issue5_6c001e6_48k",
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => "spectrum_issue5_6c001e7_16k",
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => "spectrum_issue5_6c001e7_48k",
            Self::SpectrumIssue6ARam16kPal => "spectrum_issue6a_16k",
            Self::SpectrumIssue6ARam48kPal => "spectrum_issue6a_48k",
            Self::Spectrum128KPal => "spectrum_128k",
            Self::SpectrumPlus2 => "spectrum_plus2",
            Self::SpectrumPlus2A => "spectrum_plus2a",
            Self::SpectrumPlus2B => "spectrum_plus2b",
            Self::SpectrumPlus3 => "spectrum_plus3",
            Self::Pentagon128 => "pentagon_128",
            Self::ScorpionZS256 => "scorpion_zs256",
            Self::TimexTC2048 => "timex_tc2048",
            Self::TimexTC2068 => "timex_tc2068",
            Self::TimexTS2068 => "timex_ts2068",
        }
    }

    /// The model a variant id names.
    #[must_use]
    pub fn from_variant_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.variant_id() == id)
    }

    /// The short label the window's Machine menu and title show.
    #[must_use]
    pub const fn menu_label(self) -> &'static str {
        match self {
            Self::SpectrumIssue1Ram16kPal => "ZX Spectrum Issue 1 / 16K",
            Self::SpectrumIssue1Ram48kPal => "ZX Spectrum Issue 1 / 48K",
            Self::SpectrumIssue2Ram16kPal => "ZX Spectrum Issue 2 / 16K",
            Self::SpectrumIssue2Ram48kPal => "ZX Spectrum Issue 2 / 48K",
            Self::SpectrumIssue3Ram16kPal => "ZX Spectrum Issue 3 / 16K",
            Self::SpectrumIssue3Ram48kPal => "ZX Spectrum Issue 3 / 48K",
            Self::SpectrumIssue3BRam16kPal => "ZX Spectrum Issue 3B / 16K",
            Self::SpectrumIssue3BRam48kPal => "ZX Spectrum Issue 3B / 48K",
            Self::SpectrumIssue4ARam16kPal => "ZX Spectrum Issue 4A / 16K",
            Self::SpectrumIssue4ARam48kPal => "ZX Spectrum Issue 4A / 48K",
            Self::SpectrumIssue4BRam16kPal => "ZX Spectrum Issue 4B / 16K",
            Self::SpectrumIssue4BRam48kPal => "ZX Spectrum Issue 4B / 48K",
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => "ZX Spectrum Issue 5 / 6C001E-6 / 16K",
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => "ZX Spectrum Issue 5 / 6C001E-6 / 48K",
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => "ZX Spectrum Issue 5 / 6C001E-7 / 16K",
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => "ZX Spectrum Issue 5 / 6C001E-7 / 48K",
            Self::SpectrumIssue6ARam16kPal => "ZX Spectrum Issue 6A / Ferranti / 16K",
            Self::SpectrumIssue6ARam48kPal => "ZX Spectrum Issue 6A / Ferranti / 48K",
            Self::Spectrum128KPal => "ZX Spectrum 128",
            Self::SpectrumPlus2 => "ZX Spectrum +2",
            Self::SpectrumPlus2A => "ZX Spectrum +2A",
            Self::SpectrumPlus2B => "ZX Spectrum +2B",
            Self::SpectrumPlus3 => "ZX Spectrum +3",
            Self::Pentagon128 => "Pentagon 128",
            Self::ScorpionZS256 => "Scorpion ZS-256",
            Self::TimexTC2048 => "Timex TC2048",
            Self::TimexTC2068 => "Timex TC2068",
            Self::TimexTS2068 => "Timex TS2068",
        }
    }

    /// The ROM images this model boots and their conventional file names
    /// under `~/.emu198x/roms`.
    ///
    /// 16K/48K/+ share `sinclair-zx-spectrum-48k/48.rom`; 128K, +2, +2A/+3,
    /// and +2B each have their own bundle directory. The +2A and +3 share
    /// `amstrad-zx-spectrum-plus3/plus3-{0..3}.rom` (ROM v4.0); the +2B
    /// uses its own `amstrad-zx-spectrum-plus2b/plus3-{0..3}.rom` (ROM
    /// v4.1). The exotics live under `pentagon-128/`, `scorpion-zs256/`,
    /// `timex-tc2048/`, and `timex-ts2068/`.
    #[must_use]
    pub fn firmware_sources(self) -> Vec<FirmwareSource> {
        match self {
            Self::SpectrumIssue1Ram16kPal
            | Self::SpectrumIssue1Ram48kPal
            | Self::SpectrumIssue2Ram16kPal
            | Self::SpectrumIssue2Ram48kPal
            | Self::SpectrumIssue3Ram16kPal
            | Self::SpectrumIssue3Ram48kPal
            | Self::SpectrumIssue3BRam16kPal
            | Self::SpectrumIssue3BRam48kPal
            | Self::SpectrumIssue4ARam16kPal
            | Self::SpectrumIssue4ARam48kPal
            | Self::SpectrumIssue4BRam16kPal
            | Self::SpectrumIssue4BRam48kPal
            | Self::SpectrumIssue5Ula6c001e6Ram16kPal
            | Self::SpectrumIssue5Ula6c001e6Ram48kPal
            | Self::SpectrumIssue5Ula6c001e7Ram16kPal
            | Self::SpectrumIssue5Ula6c001e7Ram48kPal
            | Self::SpectrumIssue6ARam16kPal
            | Self::SpectrumIssue6ARam48kPal => {
                vec![FirmwareSource::required(
                    "sinclair-zx-spectrum-48k-rom",
                    &["sinclair-zx-spectrum-48k/48.rom"],
                )]
            }
            Self::Spectrum128KPal => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-128k-rom-0",
                    &["sinclair-zx-spectrum-128k/128-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-128k-rom-1",
                    &["sinclair-zx-spectrum-128k/128-1.rom"],
                ),
            ],
            Self::SpectrumPlus2 => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus2-rom-0",
                    &["amstrad-zx-spectrum-plus2/plus2-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus2-rom-1",
                    &["amstrad-zx-spectrum-plus2/plus2-1.rom"],
                ),
            ],
            Self::SpectrumPlus2A | Self::SpectrumPlus3 => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-0",
                    &["amstrad-zx-spectrum-plus3/plus3-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-1",
                    &["amstrad-zx-spectrum-plus3/plus3-1.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-2",
                    &["amstrad-zx-spectrum-plus3/plus3-2.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-3",
                    &["amstrad-zx-spectrum-plus3/plus3-3.rom"],
                ),
            ],
            Self::SpectrumPlus2B => vec![
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-0",
                    &["amstrad-zx-spectrum-plus2b/plus3-0.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-1",
                    &["amstrad-zx-spectrum-plus2b/plus3-1.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-2",
                    &["amstrad-zx-spectrum-plus2b/plus3-2.rom"],
                ),
                FirmwareSource::required(
                    "sinclair-zx-spectrum-plus3-rom-3",
                    &["amstrad-zx-spectrum-plus2b/plus3-3.rom"],
                ),
            ],
            Self::Pentagon128 => vec![
                FirmwareSource::required("pentagon-rom-0", &["pentagon-128/pentagon-0.rom"]),
                FirmwareSource::required("pentagon-rom-1", &["pentagon-128/pentagon-1.rom"]),
            ],
            Self::ScorpionZS256 => vec![
                FirmwareSource::required("scorpion-rom-0", &["scorpion-zs256/scorpion-0.rom"]),
                FirmwareSource::required("scorpion-rom-1", &["scorpion-zs256/scorpion-1.rom"]),
                FirmwareSource::required("scorpion-rom-2", &["scorpion-zs256/scorpion-2.rom"]),
                FirmwareSource::required("scorpion-rom-3", &["scorpion-zs256/scorpion-3.rom"]),
            ],
            Self::TimexTC2048 => vec![FirmwareSource::required(
                "timex-tc2048-rom",
                &["timex-tc2048/tc2048.rom"],
            )],
            Self::TimexTC2068 | Self::TimexTS2068 => vec![
                FirmwareSource::required("timex-ts2068-rom-0", &["timex-ts2068/ts2068.rom"]),
                FirmwareSource::required("timex-ts2068-rom-1", &["timex-ts2068/exrom.rom"]),
            ],
        }
    }

    /// Year of original product release, not a PCB introduction date.
    #[must_use]
    pub const fn release_year(self) -> u16 {
        match self {
            Self::SpectrumIssue1Ram16kPal => 1982,
            Self::SpectrumIssue1Ram48kPal => 1982,
            Self::SpectrumIssue2Ram16kPal => 1982,
            Self::SpectrumIssue2Ram48kPal => 1982,
            Self::SpectrumIssue3Ram16kPal => 1982,
            Self::SpectrumIssue3Ram48kPal => 1982,
            Self::SpectrumIssue3BRam16kPal => 1982,
            Self::SpectrumIssue3BRam48kPal => 1982,
            Self::SpectrumIssue4ARam16kPal => 1982,
            Self::SpectrumIssue4ARam48kPal => 1982,
            Self::SpectrumIssue4BRam16kPal => 1982,
            Self::SpectrumIssue4BRam48kPal => 1982,
            Self::SpectrumIssue5Ula6c001e6Ram16kPal => 1982,
            Self::SpectrumIssue5Ula6c001e6Ram48kPal => 1982,
            Self::SpectrumIssue5Ula6c001e7Ram16kPal => 1982,
            Self::SpectrumIssue5Ula6c001e7Ram48kPal => 1982,
            Self::SpectrumIssue6ARam16kPal => 1982,
            Self::SpectrumIssue6ARam48kPal => 1982,
            Self::Spectrum128KPal => 1985,
            Self::SpectrumPlus2 => 1986,
            Self::SpectrumPlus2A => 1986,
            Self::SpectrumPlus2B => 1988,
            Self::SpectrumPlus3 => 1986,
            Self::Pentagon128 => 1989,
            Self::ScorpionZS256 => 1991,
            Self::TimexTC2048 => 1985,
            Self::TimexTC2068 => 1983,
            Self::TimexTS2068 => 1983,
        }
    }
}

/// Returns the full Spectrum family catalogue. All 11 entries have a
/// working machine crate; only `Spectrum48KPal` currently has a
/// MachineCore runtime wrapper.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    Model::ALL.into_iter().map(profile_for).collect()
}

/// Tape-only single-deck media slot — every Spectrum variant ships
/// with this. The +3 also gets a separate Disk slot inside its arm.
fn tape_slot() -> MediaSlot {
    MediaSlot::new(
        "tape-1",
        "Tape Deck",
        MediaKind::Tape,
        false,
        WritebackPolicy::InMemoryOnly,
    )
}

/// Capability bundle for the AY-PSG-and-banked-memory Sinclair models
/// (128K, +2). The +2A/+3 use this as a baseline and add disk-related
/// capabilities on top.
fn ay_capabilities() -> CapabilitySet {
    CapabilitySet::with_all([
        known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
        known_capability("ay-audio"),
        known_capability("banked-memory"),
        known_capability("keyboard-matrix"),
        known_capability("tape-input"),
        known_capability("tape-transport-control"),
        known_capability("snapshot-import"),
        known_capability("scripted-input"),
        known_capability("memory-watch"),
        known_capability("port-io"),
        known_capability("basic-program-load"),
        known_capability("tape-autoload"),
        known_capability("variant-switch"),
    ])
}

fn classic_profile(
    model: Model,
    board: common_sinclair_zx_spectrum_48k_class::PhysicalBoard,
    ram_kib: u16,
) -> MachineProfile {
    MachineProfile {
        machine_id: match board {
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue1 => {
                MachineId::from("sinclair-zx-spectrum-issue1")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue2 => {
                MachineId::from("sinclair-zx-spectrum-issue2")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3 => {
                MachineId::from("sinclair-zx-spectrum-issue3")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3B => {
                MachineId::from("sinclair-zx-spectrum-issue3b")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4A => {
                MachineId::from("sinclair-zx-spectrum-issue4a")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4B => {
                MachineId::from("sinclair-zx-spectrum-issue4b")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e6 => {
                MachineId::from("sinclair-zx-spectrum-issue5-6c001e6")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e7 => {
                MachineId::from("sinclair-zx-spectrum-issue5-6c001e7")
            }
            common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue6A => {
                MachineId::from("sinclair-zx-spectrum-issue6a")
            }
        },
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Spectrum,
        region: Region::Pal,
        release_year: 1982,
        summary: format!(
            "{}; {}; {} KiB RAM.",
            board.issue(),
            board.ula_part(),
            ram_kib
        )
        .into(),
        clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
        firmware: vec![FirmwareRequirement::new(
            "sinclair-zx-spectrum-48k-rom",
            "ZX Spectrum 48K ROM",
            false,
        )],
        media_slots: vec![tape_slot()],
        capabilities: CapabilitySet::with_all([
            known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
            known_capability("cycle-profile"),
            known_capability("beeper-audio"),
            known_capability("keyboard-matrix"),
            known_capability("snapshot-export"),
            known_capability("tape-input"),
            known_capability("tape-transport-control"),
            known_capability("snapshot-import"),
            known_capability("scripted-input"),
            known_capability("memory-watch"),
            known_capability("port-io"),
            known_capability("basic-program-load"),
            known_capability("tape-autoload"),
            known_capability("variant-switch"),
        ]),
    }
}

/// Returns the profile metadata for one Spectrum model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::SpectrumIssue1Ram16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue1, 16),
        Model::SpectrumIssue1Ram48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue1, 48),
        Model::SpectrumIssue2Ram16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue2, 16),
        Model::SpectrumIssue2Ram48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue2, 48),
        Model::SpectrumIssue3Ram16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3, 16),
        Model::SpectrumIssue3Ram48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3, 48),
        Model::SpectrumIssue3BRam16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3B, 16),
        Model::SpectrumIssue3BRam48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue3B, 48),
        Model::SpectrumIssue4ARam16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4A, 16),
        Model::SpectrumIssue4ARam48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4A, 48),
        Model::SpectrumIssue4BRam16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4B, 16),
        Model::SpectrumIssue4BRam48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue4B, 48),
        Model::SpectrumIssue5Ula6c001e6Ram16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e6, 16),
        Model::SpectrumIssue5Ula6c001e6Ram48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e6, 48),
        Model::SpectrumIssue5Ula6c001e7Ram16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e7, 16),
        Model::SpectrumIssue5Ula6c001e7Ram48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue5Ula6c001e7, 48),
        Model::SpectrumIssue6ARam16kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue6A, 16),
        Model::SpectrumIssue6ARam48kPal => classic_profile(model, common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue6A, 48),
        Model::Spectrum128KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "128K PAL follow-on profile with banked memory, AY audio, and tape-era baseline media."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: vec![
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-0",
                    "ZX Spectrum 128K ROM 0",
                    false,
                ),
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-1",
                    "ZX Spectrum 128K ROM 1",
                    false,
                ),
            ],
            media_slots: vec![tape_slot()],
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus2 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "Sinclair-branded Amstrad-built 128K-compatible. Same chip set as the 128K plus a built-in tape deck."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: vec![
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-plus2-rom-0",
                    "ZX Spectrum +2 ROM 0",
                    false,
                ),
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-plus2-rom-1",
                    "ZX Spectrum +2 ROM 1",
                    false,
                ),
            ],
            media_slots: vec![tape_slot()],
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus2A | Model::SpectrumPlus2B => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "Amstrad-built +2A / +2B with the 40077 gate array, 4 ROMs, and extended `$1FFD` paging. No floppy drive."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("sinclair-zx-spectrum-plus3-rom-{i}"),
                        format!("ZX Spectrum +2A/+3 ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps
            },
        },
        Model::SpectrumPlus3 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "Amstrad-built +3 with the 40077 gate array, 4 ROMs, extended `$1FFD` paging, and a built-in 3\" floppy drive driven by an NEC µPD765A."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("sinclair-zx-spectrum-plus3-rom-{i}"),
                        format!("ZX Spectrum +3 ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![
                tape_slot(),
                MediaSlot::new(
                    "disk-a",
                    "Floppy Drive A:",
                    MediaKind::Disk,
                    false,
                    WritebackPolicy::InMemoryOnly,
                ),
            ],
            capabilities: {
                let mut caps = ay_capabilities();
                caps.insert(known_capability("cycle-profile"));
                caps.insert(known_capability("disk-input"));
                caps.insert(known_capability(emu198x_shell::capability::ids::MEDIA_EXPORT));
                caps
            },
        },
        Model::Pentagon128 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary: "Russian Spectrum clone with no contention, AY, and Beta 128 disk interface."
                .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![
                FirmwareRequirement::new("pentagon-rom-0", "Pentagon ROM 0", false),
                FirmwareRequirement::new("pentagon-rom-1", "Pentagon ROM 1", false),
            ],
            media_slots: vec![tape_slot()],
            capabilities: { let mut caps = ay_capabilities(); caps.insert(known_capability("cycle-profile")); caps },
        },
        Model::ScorpionZS256 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "Russian extended Spectrum: 256 KB RAM in 16 banks, 4 ROMs, AY, no contention, Beta disk."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: (0..4)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("scorpion-rom-{i}"),
                        format!("Scorpion ROM {i}"),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: { let mut caps = ay_capabilities(); caps.insert(known_capability("cycle-profile")); caps },
        },
        Model::TimexTC2048 => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: model.release_year(),
            summary:
                "Portuguese 48K-compatible with the SCLD chip — 8 video modes, full I/O decoding, no AY."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "timex-tc2048-rom",
                "Timex TC2048 ROM",
                false,
            )],
            media_slots: vec![tape_slot()],
            capabilities: CapabilitySet::with_all([
                known_capability(emu198x_shell::capability::ids::AUDIO_OUTPUT),
                known_capability("cycle-profile"),
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
                known_capability("memory-watch"),
                known_capability("port-io"),
                known_capability("basic-program-load"),
                known_capability("tape-autoload"),
                known_capability("variant-switch"),
            ]),
        },
        Model::TimexTC2068 | Model::TimexTS2068 => MachineProfile {
            machine_id: MachineId::from(model.model_id()),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: if matches!(model, Model::TimexTS2068) {
                Region::Ntsc
            } else {
                Region::Pal
            },
            release_year: model.release_year(),
            summary:
                format!("{}: SCLD video, DOCK/EXROM paging via `$F4`, AY on `$F5`/`$F6`.", model.display_name()).into(),
            clock: ClockDesc::new(
                "master-cycle",
                ClockRate::from_hz(if matches!(model, Model::TimexTS2068) {
                    14_112_000
                } else {
                    14_000_000
                }),
            ),
            firmware: (0..2)
                .map(|i| {
                    FirmwareRequirement::new(
                        format!("timex-ts2068-rom-{i}"),
                        format!("{} ROM {i}", model.display_name()),
                        false,
                    )
                })
                .collect(),
            media_slots: vec![tape_slot()],
            capabilities: { let mut caps = ay_capabilities(); caps.insert(known_capability("cycle-profile")); caps },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plus3_advertises_live_media_export() {
        for model in Model::ALL {
            assert_eq!(
                profile_for(model).capabilities.contains(&known_capability(
                    emu198x_shell::capability::ids::MEDIA_EXPORT
                )),
                model == Model::SpectrumPlus3,
                "{model:?}"
            );
        }
    }

    #[test]
    fn variant_ids_round_trip_for_every_model() {
        for (model, id) in Model::ALL.into_iter().zip(Model::VARIANT_IDS) {
            assert_eq!(model.variant_id(), id);
            assert_eq!(Model::from_variant_id(id), Some(model));
        }
        assert_eq!(Model::from_variant_id("spectrum_999k"), None);
    }

    #[test]
    fn every_firmware_source_is_a_profile_requirement() {
        // `from_firmware` validates the set against the profile, so a
        // source the profile does not declare could never boot.
        for model in Model::ALL {
            let profile = profile_for(model);
            let sources = model.firmware_sources();
            assert!(!sources.is_empty(), "{} has no ROMs", model.variant_id());
            for source in &sources {
                assert!(
                    profile.firmware.iter().any(|req| req.id == source.id),
                    "{} source {} is not in its profile",
                    model.variant_id(),
                    source.id
                );
            }
            assert_eq!(
                sources.len(),
                profile.firmware.len(),
                "{} declares more firmware than it sources",
                model.variant_id()
            );
        }
    }

    #[test]
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn spectrum_48k_uses_documented_master_clock() {
        let profile = profile_for(Model::SpectrumIssue3Ram48kPal);
        assert_eq!(profile.clock.unit.as_ref(), "master-cycle");
        assert_eq!(profile.clock.rate.numerator_hz, 14_000_000);
        assert_eq!(profile.clock.rate.denominator_hz, 1);
    }

    #[test]
    fn all_profiles_require_firmware() {
        for profile in profiles() {
            assert!(
                !profile.firmware.is_empty(),
                "{} should declare firmware",
                profile.display_name
            );
        }
    }

    #[test]
    fn ram_profiles_share_one_physical_identity_per_board() {
        let boards: Vec<_> = Model::ALL
            .into_iter()
            .filter(|m| m.physical_board().is_some())
            .collect();
        assert_eq!(boards.len(), 18);
        let mut owners = std::collections::BTreeMap::<&str, Vec<u16>>::new();
        for model in boards {
            assert_eq!(profile_for(model).machine_id.as_str(), model.model_id());
            owners
                .entry(model.model_id())
                .or_default()
                .push(model.ram_kib().expect("physical RAM"));
        }
        assert_eq!(owners.len(), 9);
        for populations in owners.values_mut() {
            populations.sort_unstable();
            assert_eq!(populations.as_slice(), &[16, 48]);
        }
        assert_eq!(
            Model::SpectrumIssue3Ram48kPal.model_id(),
            "sinclair-zx-spectrum-issue3"
        );
        for retired in ["spectrum_16k", "spectrum_48k", "spectrum_plus"] {
            assert_eq!(Model::from_variant_id(retired), None);
        }
    }

    #[test]
    fn release_year_covers_every_documented_variant() {
        // 1982-1991 covers the entire family — 1983 is the earliest
        // (Timex) and 1991 is the latest (Scorpion ZS-256). Without this
        // test the standalone TimexTC2068/Plus2B/Pentagon arms never run.
        assert_eq!(Model::SpectrumIssue3Ram16kPal.release_year(), 1982);
        assert_eq!(Model::SpectrumIssue3Ram48kPal.release_year(), 1982);
        assert_eq!(Model::TimexTC2068.release_year(), 1983);
        assert_eq!(Model::TimexTS2068.release_year(), 1983);
        assert_eq!(Model::Spectrum128KPal.release_year(), 1985);
        assert_eq!(Model::TimexTC2048.release_year(), 1985);
        assert_eq!(Model::SpectrumPlus2.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus2A.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus3.release_year(), 1986);
        assert_eq!(Model::SpectrumPlus2B.release_year(), 1988);
        assert_eq!(Model::Pentagon128.release_year(), 1989);
        assert_eq!(Model::ScorpionZS256.release_year(), 1991);
    }

    #[test]
    fn display_name_returns_a_human_readable_string_per_variant() {
        // Drives every arm of `display_name`.
        assert_eq!(
            Model::SpectrumIssue3Ram16kPal.display_name(),
            "ZX Spectrum Issue 3 (16K, PAL)"
        );
        assert_eq!(
            Model::SpectrumIssue3Ram48kPal.display_name(),
            "ZX Spectrum Issue 3 (48K, PAL)"
        );
        assert_eq!(
            Model::Spectrum128KPal.display_name(),
            "ZX Spectrum 128K (PAL)"
        );
        assert_eq!(Model::SpectrumPlus2.display_name(), "ZX Spectrum +2 (PAL)");
        assert_eq!(
            Model::SpectrumPlus2A.display_name(),
            "ZX Spectrum +2A (PAL)"
        );
        assert_eq!(
            Model::SpectrumPlus2B.display_name(),
            "ZX Spectrum +2B (PAL)"
        );
        assert_eq!(Model::SpectrumPlus3.display_name(), "ZX Spectrum +3 (PAL)");
        assert_eq!(Model::Pentagon128.display_name(), "Pentagon 128");
        assert_eq!(Model::ScorpionZS256.display_name(), "Scorpion ZS-256");
        assert_eq!(Model::TimexTC2048.display_name(), "Timex TC2048");
        assert_eq!(Model::TimexTC2068.display_name(), "Timex TC2068");
        assert_eq!(Model::TimexTS2068.display_name(), "Timex TS2068 (NTSC)");
    }
}
