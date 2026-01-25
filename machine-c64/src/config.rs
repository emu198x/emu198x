//! C64 machine configuration.
//!
//! Defines the different C64 variants and their hardware configurations.
//! This allows accurate emulation of:
//! - C64 "breadbin" (1982) - original with 6581 SID
//! - C64C (1986) - revised with 8580 SID
//! - SX-64 (1984) - portable with built-in drive
//! - C64 GS (1990) - cartridge-only game console

/// Video timing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TimingMode {
    /// PAL (Europe, Australia) - 50Hz, 312 lines, 63 cycles/line
    #[default]
    Pal,
    /// NTSC (North America, Japan) - 60Hz, 263 lines, 65 cycles/line
    Ntsc,
    /// PAL-N (South America) - 50Hz with NTSC-like timing
    PalN,
}

impl TimingMode {
    /// CPU clock frequency in Hz.
    pub const fn cpu_clock(self) -> u32 {
        match self {
            TimingMode::Pal => 985248,
            TimingMode::Ntsc => 1022727,
            TimingMode::PalN => 1023440,
        }
    }

    /// Cycles per raster line.
    pub const fn cycles_per_line(self) -> u32 {
        match self {
            TimingMode::Pal => 63,
            TimingMode::Ntsc => 65,
            TimingMode::PalN => 65,
        }
    }

    /// Total raster lines per frame.
    pub const fn lines_per_frame(self) -> u32 {
        match self {
            TimingMode::Pal => 312,
            TimingMode::Ntsc => 263,
            TimingMode::PalN => 312,
        }
    }

    /// Cycles per frame.
    pub const fn cycles_per_frame(self) -> u32 {
        self.cycles_per_line() * self.lines_per_frame()
    }

    /// Frames per second.
    pub const fn fps(self) -> f32 {
        match self {
            TimingMode::Pal => 50.125,
            TimingMode::Ntsc => 59.826,
            TimingMode::PalN => 50.125,
        }
    }

    /// First visible raster line.
    pub const fn first_visible_line(self) -> u16 {
        match self {
            TimingMode::Pal => 16,
            TimingMode::Ntsc => 13,
            TimingMode::PalN => 16,
        }
    }

    /// Last visible raster line.
    pub const fn last_visible_line(self) -> u16 {
        match self {
            TimingMode::Pal => 287,
            TimingMode::Ntsc => 252,
            TimingMode::PalN => 287,
        }
    }
}

/// VIC-II chip revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VicRevision {
    /// 6567 R56A - early NTSC (rare)
    Vic6567R56A,
    /// 6567 R8 - common NTSC
    Vic6567R8,
    /// 6569 R1 - early PAL
    Vic6569R1,
    /// 6569 R3 - common PAL (most breadbins)
    #[default]
    Vic6569R3,
    /// 8562 - late NTSC (C64C)
    Vic8562,
    /// 8565 - late PAL (C64C)
    Vic8565,
}

impl VicRevision {
    /// Get the timing mode for this VIC revision.
    pub const fn timing_mode(self) -> TimingMode {
        match self {
            VicRevision::Vic6567R56A | VicRevision::Vic6567R8 | VicRevision::Vic8562 => {
                TimingMode::Ntsc
            }
            VicRevision::Vic6569R1 | VicRevision::Vic6569R3 | VicRevision::Vic8565 => {
                TimingMode::Pal
            }
        }
    }

    /// Whether this is a "new" VIC (8562/8565).
    pub const fn is_new_vic(self) -> bool {
        matches!(self, VicRevision::Vic8562 | VicRevision::Vic8565)
    }
}

/// SID chip model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidRevision {
    /// 6581 - original SID with warm, gritty filter
    #[default]
    Mos6581,
    /// 8580 - revised SID with cleaner filter, different digi
    Mos8580,
}

/// C64 machine variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MachineVariant {
    /// Original C64 "breadbin" (1982) - PAL
    #[default]
    C64Pal,
    /// Original C64 "breadbin" (1982) - NTSC
    C64Ntsc,
    /// C64C (1986) - PAL, revised hardware
    C64CPal,
    /// C64C (1986) - NTSC, revised hardware
    C64CNtsc,
    /// SX-64 (1984) - portable PAL
    Sx64Pal,
    /// SX-64 (1984) - portable NTSC
    Sx64Ntsc,
    /// C64 Game System (1990) - cartridge-only PAL
    C64Gs,
}

impl MachineVariant {
    /// Get the default configuration for this variant.
    pub const fn config(self) -> MachineConfig {
        match self {
            MachineVariant::C64Pal => MachineConfig {
                vic: VicRevision::Vic6569R3,
                sid: SidRevision::Mos6581,
                has_keyboard: true,
                has_cassette_port: true,
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::C64Ntsc => MachineConfig {
                vic: VicRevision::Vic6567R8,
                sid: SidRevision::Mos6581,
                has_keyboard: true,
                has_cassette_port: true,
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::C64CPal => MachineConfig {
                vic: VicRevision::Vic8565,
                sid: SidRevision::Mos8580,
                has_keyboard: true,
                has_cassette_port: true,
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::C64CNtsc => MachineConfig {
                vic: VicRevision::Vic8562,
                sid: SidRevision::Mos8580,
                has_keyboard: true,
                has_cassette_port: true,
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::Sx64Pal => MachineConfig {
                vic: VicRevision::Vic6569R3,
                sid: SidRevision::Mos6581,
                has_keyboard: true,
                has_cassette_port: false, // SX-64 has no cassette port
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::Sx64Ntsc => MachineConfig {
                vic: VicRevision::Vic6567R8,
                sid: SidRevision::Mos6581,
                has_keyboard: true,
                has_cassette_port: false,
                has_user_port: true,
                has_expansion_port: true,
                has_iec_port: true,
            },
            MachineVariant::C64Gs => MachineConfig {
                vic: VicRevision::Vic8565,
                sid: SidRevision::Mos8580,
                has_keyboard: false, // Game System has no keyboard
                has_cassette_port: false,
                has_user_port: false,
                has_expansion_port: true, // Cartridge slot
                has_iec_port: false,
            },
        }
    }
}

/// Machine hardware configuration.
#[derive(Clone, Copy, Debug)]
pub struct MachineConfig {
    /// VIC-II revision
    pub vic: VicRevision,
    /// SID revision
    pub sid: SidRevision,
    /// Whether the machine has a keyboard
    pub has_keyboard: bool,
    /// Whether the machine has a cassette port
    pub has_cassette_port: bool,
    /// Whether the machine has a user port
    pub has_user_port: bool,
    /// Whether the machine has an expansion port (cartridge)
    pub has_expansion_port: bool,
    /// Whether the machine has an IEC port (disk drive)
    pub has_iec_port: bool,
}

impl MachineConfig {
    /// Get the timing mode from the VIC revision.
    pub const fn timing_mode(&self) -> TimingMode {
        self.vic.timing_mode()
    }

    /// Get the CPU clock speed.
    pub const fn cpu_clock(&self) -> u32 {
        self.timing_mode().cpu_clock()
    }

    /// Get cycles per frame.
    pub const fn cycles_per_frame(&self) -> u32 {
        self.timing_mode().cycles_per_frame()
    }
}

impl Default for MachineConfig {
    fn default() -> Self {
        MachineVariant::default().config()
    }
}
