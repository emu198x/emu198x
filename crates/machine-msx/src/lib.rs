//! MSX1 and MSX2 home computer machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-msx` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — slot system, PPI-driven
//! keyboard matrix, I/O port map, MegaROM mapper variants — but the
//! wiring is written against [`emu198x_zilog_z80::Z80`]'s public pin fields
//! and `bus_request()` collapse.
//!
//! # The MSX standard
//!
//! The MSX (1983) is a standardised home computer designed by ASCII
//! Corporation and Microsoft. Manufactured by Sony, Philips, Yamaha,
//! Toshiba, Sanyo, Matsushita, Spectravideo, and over a dozen others.
//! Strong in Japan, the Netherlands, Spain, Argentina, Brazil, and
//! the Soviet Union; smaller but real Code198x curriculum target.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 1.789773 MHz (consumed via our
//!   [`emu198x_gi_ay_3_8912`] crate — the 8912 silicon is an 8910 with port
//!   B pins not bonded out; register 15 still exists in software and
//!   our crate stores/returns it correctly)
//! - **PPI:** Intel 8255 (Mode 0)
//! - **RAM:** 8-64 KB in slot 3 (this port: 64 KB)
//! - **ROM:** 32 KB Main-ROM (BIOS + MSX-BASIC 1.0) in slot 0
//!
//! # Slot system
//!
//! The Z80's 64 KB address space is divided into four 16 KB pages
//! (page 0 = $0000-$3FFF, ... page 3 = $C000-$FFFF). PPI port A
//! selects one of 4 primary slots per page, two bits per page:
//!
//! ```text
//! PPI port A:  [P3 P3 P2 P2 P1 P1 P0 P0]
//!               bit 7              bit 0
//! ```
//!
//! Where everything sits is the machine's [`MsxLayout`]. The default:
//!
//! - **Slot 0:** Main-ROM (BIOS) at pages 0-1
//! - **Slot 1:** Cartridge slot 1
//! - **Slot 2:** Cartridge slot 2
//! - **Slot 3:** Main RAM (64 KB)
//!
//! Real MSX1 machines vary: openMSX's 125 MSX1 configurations use 27
//! layouts. RAM may sit in slot 1, 2 or 3, share slot 0 with the BIOS, or
//! hide in a subslot of an expanded slot, and the cartridge ports move to
//! whichever slots are left. An expanded slot has a subslot register at
//! `$FFFF` while it is selected for page 3: two bits per page choose its
//! subslot, and a read returns the complement of what was written.
//!
//! # I/O port map
//!
//! | Port  | R/W   | Function                                        |
//! |-------|-------|-------------------------------------------------|
//! | `$98` | r/w   | VDP data                                        |
//! | `$99` | r/w   | VDP control (write) / status (read)             |
//! | `$A0` | write | PSG register select                             |
//! | `$A1` | write | PSG register data                               |
//! | `$A2` | read  | PSG register read                               |
//! | `$A8` | r/w   | PPI port A — primary slot select                |
//! | `$A9` | read  | PPI port B — keyboard column data               |
//! | `$AA` | r/w   | PPI port C — keyboard row + CAPS LED + cassette |
//! | `$AB` | write | PPI mode register                               |
//!
//! # Keyboard
//!
//! 11 rows × 8 columns matrix, active-low. PPI port C bits 0-3 drive
//! row select; reading PPI port B returns the column data for the
//! selected row. Standard MSX layout (row 0 = digits 0-7, row 1 =
//! 8/9/punctuation/letters, etc.). Host pokes the matrix via
//! [`Msx::press_key`] / [`Msx::release_key`].
//!
//! # Clock model
//!
//! Adopts the SG-1000's correct 3:2 VDP-dot-to-CPU-T-state phase
//! counter (CPU 3.579545 MHz, VDP dot 5.369 MHz). One iteration of
//! [`Msx::run_frame`] corresponds to one Z80 T-state; per iteration
//! the phase counter advances by 3 and yields one VDP dot whenever
//! it reaches 2. PSG ticks every other T-state (clock ÷ 2).
//!
//! # MSX2
//!
//! [`Msx::msx2`] builds the 1985 standard's machine. Against an MSX1 it
//! adds:
//!
//! - **VDP:** a Yamaha V9938 with 128 KB of VRAM in place of the TMS9918,
//!   on ports `$98-$9B` (`$9A` the palette, `$9B` indirect register
//!   writes). Its dot clock and 342-dot line are the TMS9918's, so it steps
//!   on the same 3:2 phase.
//! - **SUB-ROM:** 16 KB of BIOS extensions at `$0000-$3FFF` in the slot
//!   [`MsxLayout::sub_rom`] names, usually subslot 0 of an expanded slot 3.
//! - **Memory mapper:** RAM in 16 KB segments, one chosen for each page by
//!   the registers on ports `$FC-$FF` (pages 0 to 3). A segment number wraps
//!   at the mapper's size. Every machine modelled here reads all eight bits
//!   back, openMSX's `largest`; the S1985 engine's five-bit read-back is not
//!   modelled. Machines whose RAM is plain 64 KB leave [`MsxLayout::mapper`]
//!   empty and the ports read `$FF`.
//! - **Clock:** a Ricoh RP5C01 behind a register latch on port `$B4` and
//!   its data on `$B5`, stepped at its 16,384 Hz rate from the CPU clock, so
//!   time passes with emulated time.
//!
//! The BIOS may be longer than 32 KB: C-BIOS keeps its start-up logo in a
//! 16 KB ROM at `$8000` in the BIOS slot, which the machine maps by reading
//! the BIOS image wherever it is long enough.
//!
//! # Sound beyond the PSG
//!
//! MSX-MUSIC (a YM2413 on ports `$7C`/`$7D`), the FM-PAC cartridge and the
//! SCC cartridges are described in the `sound` module. Their output is mixed
//! with the PSG's by [`Msx::end_audio_frame`].

use emu198x_gi_ay_3_8912::{Ay3_8912, AyWriteRecord, AyWriteWatch};
use emu198x_intel_8255::Ppi8255;
pub use emu198x_konami_scc::Scc;
pub use emu198x_ricoh_rp5c01::{DateTime, Rp5c01};
use emu198x_ti_tms9918::{Tms9918, VdpRegion};
pub use emu198x_yamaha_v9938::{Mode as V9938Mode, V9938};
pub use emu198x_yamaha_ym2413::Ym2413;
use emu198x_z80_cadence::Z80Machine;
use emu198x_zilog_z80::{BusOp, Z80};
use serde::{Deserialize, Serialize};

mod sound;
pub use sound::FM_PAC_SRAM_SIZE;
use sound::{CartridgeSound, ExpansionMixer, MsxMusic};

/// VDP dot ticks per CPU T-state, numerator (TMS9918A on MSX).
///
/// Accumulated per CPU **half-cycle**, so the denominator is 4 rather than
/// 2: three dots per four half-cycles is the same 3:2 against T-states,
/// interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
/// VDP dot ticks per CPU half-cycle, denominator.
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;
/// CPU T-states per scanline (342 VDP dots × 2 / 3).
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

/// The Z80's clock, which steps the RP5C01's 16,384 Hz divider.
const CPU_CLOCK_HZ: u32 = 3_579_545;
const AY_CLOCK_HZ: u32 = 1_789_773;
const AY_SAMPLE_RATE: u32 = 48_000;
/// Pre-allocated PSG buffer for ~60 Hz host audio. At 48 kHz and
/// 60 fps the budget is 800 samples per frame; round up.
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// A slot: a primary slot, and in an expanded one the subslot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotAddress {
    pub primary: u8,
    pub secondary: Option<u8>,
}

impl SlotAddress {
    /// A primary slot that is not expanded.
    #[must_use]
    pub const fn primary(slot: u8) -> Self {
        Self {
            primary: slot,
            secondary: None,
        }
    }

    /// Subslot `secondary` of expanded slot `primary`.
    #[must_use]
    pub const fn expanded(primary: u8, secondary: u8) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
        }
    }
}

/// RAM at `start..=end` in one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RamRegion {
    pub slot: SlotAddress,
    pub start: u16,
    pub end: u16,
}

/// An MSX2 memory mapper: `size_kb` of RAM filling all four pages of one
/// slot, a 16 KB segment a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMapper {
    pub slot: SlotAddress,
    /// A power of two, at least 64.
    pub size_kb: u16,
}

/// Where the BIOS, the RAM and the cartridge ports sit.
///
/// The RAM regions must not overlap in the address space: they share one
/// 64 KB store, indexed by address. Every layout openMSX describes for an
/// MSX1 fits that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MsxLayout {
    /// The 32 KB Main-ROM at `$0000-$7FFF`.
    pub bios: SlotAddress,
    /// The MSX2 SUB-ROM at `$0000-$3FFF`, when there is one.
    pub sub_rom: Option<SlotAddress>,
    pub ram: Vec<RamRegion>,
    /// The MSX2 memory mapper, when the RAM is mapped.
    pub mapper: Option<MemoryMapper>,
    /// The slots behind cartridge ports 1 and 2.
    pub cartridge_ports: [SlotAddress; 2],
    /// Which primary slots are expanded into four subslots.
    pub expanded: [bool; 4],
}

impl Default for MsxLayout {
    /// The BIOS in slot 0, cartridges in 1 and 2, 64 KB of RAM in 3.
    fn default() -> Self {
        Self {
            bios: SlotAddress::primary(0),
            sub_rom: None,
            ram: vec![RamRegion {
                slot: SlotAddress::primary(3),
                start: 0x0000,
                end: 0xFFFF,
            }],
            mapper: None,
            cartridge_ports: [SlotAddress::primary(1), SlotAddress::primary(2)],
            expanded: [false; 4],
        }
    }
}

/// MSX video region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsxRegion {
    Ntsc,
    Pal,
}

/// MegaROM cartridge mapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapperType {
    /// Plain ROM (no banking), up to 32 KB at $4000-$BFFF (or
    /// $0000-$3FFF on smaller carts).
    Plain,
    /// Konami without SCC: 8 KB banks, written at $6000 / $8000 / $A000.
    Konami,
    /// Konami SCC: 8 KB banks at $5000 / $7000 / $9000 / $B000.
    KonamiScc,
    /// ASCII 8 KB: banks at $6000 / $6800 / $7000 / $7800.
    Ascii8,
    /// ASCII 16 KB: banks at $6000 / $7000.
    Ascii16,
    /// Panasonic's FM-PAC: MSX-MUSIC in a cartridge, with 8 KB of SRAM.
    /// The image is its 64 KB ROM.
    FmPac,
    /// Konami's Sound Cartridge: an SCC-I and 128 KB of RAM, which the
    /// image, if any, fills.
    SoundCartridge,
}

/// Infer one of the mapper families implemented by this machine from ROM
/// code. ROM headers do not identify mapper hardware, so images larger than
/// 32 KiB are scored by absolute `LD (nn),A` writes to known mapper registers.
/// This follows OpenMSX's `RomFactory::guessRomType`; a caller with database
/// metadata or an explicit user choice should prefer that stronger evidence.
#[must_use]
pub fn detect_mapper(rom: &[u8]) -> MapperType {
    // The FM-PAC's ROM carries the signature the MSX-MUSIC BIOS searches the
    // slots for, `OPLL` at `$401C`, after its own `PAC2`.
    if rom.get(0x18..0x20) == Some(b"PAC2OPLL".as_slice()) {
        return MapperType::FmPac;
    }
    if rom.len() <= 32 * 1024 {
        return MapperType::Plain;
    }

    let mut scores = [0_u32; 4];
    for instruction in rom.windows(3) {
        if instruction[0] != 0x32 {
            continue;
        }
        let address = u16::from_le_bytes([instruction[1], instruction[2]]);
        match address {
            0x5000 | 0x9000 | 0xb000 => scores[1] += 1,
            0x4000 | 0x8000 | 0xa000 => scores[0] += 1,
            0x6800 | 0x7800 => scores[2] += 1,
            0x6000 => {
                scores[0] += 1;
                scores[2] += 1;
                scores[3] += 1;
            }
            0x7000 => {
                scores[1] += 1;
                scores[2] += 1;
                scores[3] += 1;
            }
            0x77ff => scores[3] += 1,
            _ => {}
        }
    }
    // OpenMSX applies this correction because the shared $6000/$7000 writes
    // otherwise bias close results toward ASCII8.
    scores[2] = scores[2].saturating_sub(1);

    let candidates = [
        MapperType::Konami,
        MapperType::KonamiScc,
        MapperType::Ascii8,
        MapperType::Ascii16,
    ];
    scores
        .iter()
        .enumerate()
        .filter(|(_, score)| **score != 0)
        .max_by_key(|(index, score)| (**score, *index))
        .map_or(MapperType::Plain, |(index, _)| candidates[index])
}

/// A cartridge slot containing ROM + mapper bank registers.
#[derive(Serialize, Deserialize)]
struct CartridgeSlot {
    rom: Vec<u8>,
    mapper: MapperType,
    banks: [u8; 4],
    /// A sound chip, and the RAM or SRAM that comes with it.
    sound: CartridgeSound,
}

impl CartridgeSlot {
    fn new(rom: Vec<u8>, mapper: MapperType) -> Self {
        let sound = match mapper {
            MapperType::KonamiScc => CartridgeSound::konami_scc(),
            MapperType::FmPac => CartridgeSound::fm_pac(),
            MapperType::SoundCartridge => CartridgeSound::sound_cartridge(&rom),
            _ => CartridgeSound::None,
        };
        Self {
            rom,
            mapper,
            banks: [0, 1, 2, 3],
            sound,
        }
    }

    fn empty() -> Self {
        Self {
            rom: Vec::new(),
            mapper: MapperType::Plain,
            banks: [0; 4],
            sound: CartridgeSound::None,
        }
    }

    fn read(&self, addr: u16) -> u8 {
        if let Some(value) = self.sound.peek(&self.rom, &self.banks, addr) {
            return value;
        }
        if self.rom.is_empty() {
            return 0xFF;
        }
        // The SCC MegaROM mirrors page 2 into page 0 and page 1 into page 3.
        let addr = match (self.mapper, addr) {
            (MapperType::KonamiScc, 0x0000..=0x3FFF) => addr + 0x8000,
            (MapperType::KonamiScc, 0xC000..=0xFFFF) => addr - 0x8000,
            _ => addr,
        };
        match self.mapper {
            MapperType::Plain => {
                let offset = if addr >= 0x4000 {
                    (addr - 0x4000) as usize
                } else {
                    addr as usize
                };
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Konami | MapperType::KonamiScc | MapperType::Ascii8 => {
                let window = match addr {
                    0x4000..=0x5FFF => 0,
                    0x6000..=0x7FFF => 1,
                    0x8000..=0x9FFF => 2,
                    0xA000..=0xBFFF => 3,
                    _ => return 0xFF,
                };
                let bank = self.banks[window] as usize;
                let offset = bank * 8192 + (addr as usize & 0x1FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Ascii16 => {
                let window = usize::from(addr >= 0x8000);
                let bank = self.banks[window] as usize;
                let offset = bank * 16384 + (addr as usize & 0x3FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            // Their own hardware answers every read.
            MapperType::FmPac | MapperType::SoundCartridge => 0xFF,
        }
    }

    /// A CPU read's side effects: the SCC's deformation register.
    fn read_side_effects(&mut self, addr: u16) {
        let _ = self.sound.read(&self.rom, &self.banks, addr);
    }

    fn write(&mut self, addr: u16, value: u8) {
        if self.sound.write(&mut self.banks, addr, value) {
            return;
        }
        match self.mapper {
            MapperType::Konami => match addr {
                0x6000..=0x7FFF => self.banks[1] = value,
                0x8000..=0x9FFF => self.banks[2] = value,
                0xA000..=0xBFFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::KonamiScc => match addr {
                0x5000..=0x57FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                0x9000..=0x97FF => self.banks[2] = value,
                0xB000..=0xB7FF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii8 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x6800..=0x6FFF => self.banks[1] = value,
                0x7000..=0x77FF => self.banks[2] = value,
                0x7800..=0x7FFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii16 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                _ => {}
            },
            MapperType::Plain | MapperType::FmPac | MapperType::SoundCartridge => {}
        }
    }
}

/// The video chip: the MSX1's TMS9918 or the MSX2's V9938.
#[derive(Serialize, Deserialize)]
enum Video {
    Tms9918(Box<Tms9918>),
    V9938(Box<V9938>),
}

impl Video {
    fn tick(&mut self) -> bool {
        match self {
            Self::Tms9918(vdp) => vdp.tick(),
            Self::V9938(vdp) => vdp.tick(),
        }
    }

    fn interrupt(&self) -> bool {
        match self {
            Self::Tms9918(vdp) => vdp.interrupt,
            Self::V9938(vdp) => vdp.interrupt,
        }
    }

    fn framebuffer(&self) -> &[u32] {
        match self {
            Self::Tms9918(vdp) => vdp.framebuffer(),
            Self::V9938(vdp) => vdp.framebuffer(),
        }
    }

    fn framebuffer_width(&self) -> u32 {
        match self {
            Self::Tms9918(vdp) => vdp.framebuffer_width(),
            Self::V9938(vdp) => vdp.framebuffer_width(),
        }
    }

    fn framebuffer_height(&self) -> u32 {
        match self {
            Self::Tms9918(vdp) => vdp.framebuffer_height(),
            Self::V9938(vdp) => vdp.framebuffer_height(),
        }
    }

    fn read(&mut self, port: u8) -> u8 {
        match (self, port) {
            (Self::Tms9918(vdp), 0x98) => vdp.read_data(),
            (Self::Tms9918(vdp), 0x99) => vdp.read_status(),
            (Self::V9938(vdp), 0x98) => vdp.read_data(),
            (Self::V9938(vdp), 0x99) => vdp.read_status(),
            _ => 0xFF,
        }
    }

    fn write(&mut self, port: u8, value: u8) {
        match (self, port) {
            (Self::Tms9918(vdp), 0x98) => vdp.write_data(value),
            (Self::Tms9918(vdp), 0x99) => vdp.write_control(value),
            (Self::V9938(vdp), 0x98) => vdp.write_data(value),
            (Self::V9938(vdp), 0x99) => vdp.write_control(value),
            (Self::V9938(vdp), 0x9A) => vdp.write_palette(value),
            (Self::V9938(vdp), 0x9B) => vdp.write_indirect(value),
            _ => {}
        }
    }
}

/// The MSX2's additions: the SUB-ROM, the mapped RAM and its registers,
/// and the clock.
#[derive(Serialize, Deserialize)]
struct Msx2Parts {
    sub_rom: Vec<u8>,
    mapper_ram: Vec<u8>,
    /// The segment each page selects, as last written to `$FC-$FF`.
    mapper_registers: [u8; 4],
    rtc: Rp5c01,
    /// The RP5C01 register port `$B4` last selected.
    rtc_latch: u8,
    /// The RP5C01's divider phase against the CPU clock.
    rtc_phase: u32,
}

/// One captured I/O port access, for the debug trace.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u16,
    /// I/O port (low 8 bits of the address bus).
    pub port: u8,
    /// Byte written, or byte returned on a read.
    pub value: u8,
    /// `true` for `OUT`, `false` for `IN`.
    pub write: bool,
}

/// MSX1 machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the
/// AY-3-8910 PSG, the 8255 PPI, ROM, RAM, slot/bank state, keyboard, and
/// joystick state all carry live state. `io_trace` and `ay_watch` are
/// host-side debug buffers, not machine state, so they are skipped and
/// default on restore.
#[derive(Serialize, Deserialize)]
pub struct Msx {
    cpu: Z80,
    video: Video,
    psg: Ay3_8912,
    ppi: Ppi8255,
    bios_rom: Vec<u8>,
    cart1: CartridgeSlot,
    cart2: CartridgeSlot,
    ram: Vec<u8>,
    /// Where the BIOS, RAM and cartridge ports sit.
    layout: MsxLayout,
    /// The subslot register of each expanded primary slot.
    subslots: [u8; 4],
    /// Keyboard matrix: 11 rows × 8 columns, active-low (1 = released).
    keyboard: [u8; 11],
    region: MsxRegion,
    /// Progress of the standard MSX wait state in the current opcode-fetch
    /// (`M1`) bus cycle: 0 = not started, 2/1 = half-cycles remaining,
    /// 3 = complete.
    m1_wait_phase: u8,
    cpu_tstates: u64,
    /// The cadence driver's half-cycle phase; a restore lands on a T-state
    /// boundary, so it is not part of the snapshot.
    #[serde(skip)]
    cadence_hc: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    /// Toggles every T-state so the PSG ticks at CPU ÷ 2 (1.789 MHz).
    psg_phase: u8,
    frame_count: u64,
    /// Transient observation for a bounded native-frame request.
    #[serde(skip)]
    native_frame_ready: bool,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
    /// When `Some`, every write to the PSG data port ($A1) is captured
    /// for the shared `watch_ay_*` tools. Host-side debug only, not part
    /// of the snapshot.
    #[serde(skip)]
    ay_watch: Option<AyWriteWatch>,
    /// Active-low PSG port-A byte for each of the two joystick ports
    /// (`[0]` = port 1, `[1]` = port 2). The MSX reads the joystick through
    /// the AY-3-891x sound chip: register 14 (port A) presents the directions
    /// and triggers of the port that register 15 bit 6 selects. The standard
    /// per-bit layout is bit 0 up, 1 down, 2 left, 3 right, 4 trigger A,
    /// 5 trigger B, active low (`0` = pressed). Host input only, so it is
    /// not part of the snapshot. Source: MSX PSG joystick standard (no entry
    /// in the in-tree reference library at time of writing).
    joystick: [u8; 2],
    /// `None` on an MSX1.
    msx2: Option<Msx2Parts>,
    /// Built-in MSX-MUSIC, when the machine has it.
    msx_music: Option<MsxMusic>,
    /// The cartridges' and MSX-MUSIC's sound, averaged to the PSG's cadence.
    expansion: ExpansionMixer,
}

impl Msx {
    /// Create a new MSX1 with the given BIOS ROM. Both cart slots
    /// start empty; insert with [`Msx::insert_cart1`] /
    /// [`Msx::insert_cart2`].
    #[must_use]
    pub fn new(bios_rom: Vec<u8>, region: MsxRegion) -> Self {
        Self::with_layout(bios_rom, region, MsxLayout::default())
    }

    /// Create an MSX1 whose BIOS, RAM and cartridge ports sit where `layout`
    /// says.
    #[must_use]
    pub fn with_layout(bios_rom: Vec<u8>, region: MsxRegion, layout: MsxLayout) -> Self {
        let vdp_region = match region {
            MsxRegion::Ntsc => VdpRegion::Ntsc,
            MsxRegion::Pal => VdpRegion::Pal,
        };
        let tstates_per_frame = match region {
            MsxRegion::Ntsc => NTSC_TSTATES_PER_FRAME,
            MsxRegion::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            video: Video::Tms9918(Box::new(Tms9918::new(vdp_region))),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Self::new_ppi(),
            bios_rom,
            cart1: CartridgeSlot::empty(),
            cart2: CartridgeSlot::empty(),
            ram: vec![0u8; 65536],
            layout,
            subslots: [0; 4],
            keyboard: [0xFF; 11],
            region,
            m1_wait_phase: 0,
            cpu_tstates: 0,
            cadence_hc: 0,
            tstates_per_frame,
            vdp_phase: 0,
            psg_phase: 0,
            frame_count: 0,
            native_frame_ready: false,
            io_trace: None,
            ay_watch: None,
            joystick: [0xFF; 2],
            msx2: None,
            msx_music: None,
            expansion: ExpansionMixer::default(),
        }
    }

    /// Fit MSX-MUSIC: the FM-BIOS `rom` at `$4000-$7FFF` in `slot`, and a
    /// YM2413 on ports `$7C`/`$7D`.
    pub fn install_msx_music(&mut self, slot: SlotAddress, rom: Vec<u8>) {
        self.msx_music = Some(MsxMusic::new(slot, rom));
    }

    /// The built-in MSX-MUSIC's YM2413, when the machine has one.
    #[must_use]
    pub fn msx_music(&self) -> Option<&Ym2413> {
        self.msx_music.as_ref().map(|music| &music.ym)
    }

    fn cart(&self, slot: u8) -> Option<&CartridgeSlot> {
        match slot {
            1 => Some(&self.cart1),
            2 => Some(&self.cart2),
            _ => None,
        }
    }

    /// The SCC of the cartridge in slot 1 or 2, if it has one.
    #[must_use]
    pub fn cartridge_scc(&self, slot: u8) -> Option<&Scc> {
        self.cart(slot)?.sound.scc_chip()
    }

    /// The YM2413 of the FM-PAC in slot 1 or 2, if there is one.
    #[must_use]
    pub fn cartridge_ym2413(&self, slot: u8) -> Option<&Ym2413> {
        self.cart(slot)?.sound.ym2413()
    }

    /// The SRAM of the FM-PAC in slot 1 or 2, to save PAC data.
    #[must_use]
    pub fn fm_pac_sram(&self, slot: u8) -> Option<&[u8]> {
        self.cart(slot)?.sound.fm_pac_sram()
    }

    /// The SRAM of the FM-PAC in slot 1 or 2, to restore PAC data.
    pub fn fm_pac_sram_mut(&mut self, slot: u8) -> Option<&mut [u8]> {
        match slot {
            1 => self.cart1.sound.fm_pac_sram_mut(),
            2 => self.cart2.sound.fm_pac_sram_mut(),
            _ => None,
        }
    }

    /// Finish the frame's audio: the PSG's samples, with the cartridges'
    /// and MSX-MUSIC's mixed in, 48 kHz mono.
    /// Returns how many of `out`'s samples the frame produced; the rest are
    /// zero.
    pub fn end_audio_frame(&mut self, out: &mut [f32]) -> usize {
        self.psg.end_frame(out);
        self.expansion.end_frame(out).min(out.len())
    }

    /// Create an MSX2: a V9938, the SUB-ROM where `layout.sub_rom` says,
    /// the memory mapper `layout.mapper` describes, and the RP5C01.
    #[must_use]
    pub fn msx2(bios_rom: Vec<u8>, sub_rom: Vec<u8>, region: MsxRegion, layout: MsxLayout) -> Self {
        let mapper_bytes = layout
            .mapper
            .map_or(0, |mapper| usize::from(mapper.size_kb) * 1024);
        let mut machine = Self::with_layout(bios_rom, region, layout);
        machine.video = Video::V9938(Box::new(V9938::new(match region {
            MsxRegion::Ntsc => emu198x_yamaha_v9938::VdpRegion::Ntsc,
            MsxRegion::Pal => emu198x_yamaha_v9938::VdpRegion::Pal,
        })));
        machine.msx2 = Some(Msx2Parts {
            sub_rom,
            mapper_ram: vec![0; mapper_bytes],
            mapper_registers: [0; 4],
            rtc: Rp5c01::new(),
            rtc_latch: 0,
            rtc_phase: 0,
        });
        machine
    }

    /// Whether this is an MSX2.
    #[must_use]
    pub fn is_msx2(&self) -> bool {
        self.msx2.is_some()
    }

    /// The SUB-ROM bytes, on an MSX2.
    #[must_use]
    pub fn sub_rom(&self) -> Option<&[u8]> {
        self.msx2.as_ref().map(|parts| parts.sub_rom.as_slice())
    }

    /// The memory mapper's registers for pages 0-3, on an MSX2.
    #[must_use]
    pub fn mapper_registers(&self) -> Option<[u8; 4]> {
        self.msx2.as_ref().map(|parts| parts.mapper_registers)
    }

    /// The real-time clock, on an MSX2.
    #[must_use]
    pub fn rtc(&self) -> Option<&Rp5c01> {
        self.msx2.as_ref().map(|parts| &parts.rtc)
    }

    /// The real-time clock, on an MSX2, to set its time or restore its RAM.
    pub fn rtc_mut(&mut self) -> Option<&mut Rp5c01> {
        self.msx2.as_mut().map(|parts| &mut parts.rtc)
    }

    /// Set the digital joystick state for `port` (1 or 2). Composes the
    /// active-low PSG port-A byte the BIOS reads through register 14; the
    /// `fire` button drives trigger A. See the `joystick` field for the
    /// bit layout. Out-of-range ports are clamped to the valid pair.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        let idx = usize::from(port.clamp(1, 2) - 1);
        let mut byte = 0xFFu8;
        for (pressed, bit) in [
            (up, 0x01),
            (down, 0x02),
            (left, 0x04),
            (right, 0x08),
            (fire, 0x10),
        ] {
            if pressed {
                byte &= !bit;
            }
        }
        self.joystick[idx] = byte;
    }

    /// The active-low PSG port-A byte currently latched for `port` (1 or 2):
    /// the value the BIOS would read through register 14 with that port
    /// selected. Out-of-range ports clamp to the valid pair. For inspection
    /// and host-side input wiring.
    #[must_use]
    pub fn joystick_byte(&self, port: u8) -> u8 {
        self.joystick[usize::from(port.clamp(1, 2) - 1)]
    }

    /// Insert a cartridge into slot 1 with the given mapper.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart1 = CartridgeSlot::new(rom, mapper);
    }

    /// Insert a cartridge into slot 2.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart2 = CartridgeSlot::new(rom, mapper);
    }

    /// Installed BIOS bytes, including firmware restored from a snapshot.
    #[must_use]
    pub fn bios_rom(&self) -> &[u8] {
        &self.bios_rom
    }

    /// Installed cartridge bytes and mapper for physical slot 1 or 2.
    /// Empty or unknown slots return `None`.
    #[must_use]
    pub fn cartridge(&self, slot: u8) -> Option<(&[u8], MapperType)> {
        let cart = match slot {
            1 => &self.cart1,
            2 => &self.cart2,
            _ => return None,
        };
        (!cart.rom.is_empty()).then_some((cart.rom.as_slice(), cart.mapper))
    }

    /// Mapper currently selected for cartridge slot 1.
    #[must_use]
    pub fn cart1_mapper(&self) -> MapperType {
        self.cart1.mapper
    }

    /// Run one frame and return T-states consumed.
    /// Stop on the video chip's next raster wrap, not a rounded CPU budget.
    /// The two-field safety budget reports false rather than inventing a frame.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        let start = self.cpu_tstates;
        self.native_frame_ready = false;
        while !self.native_frame_ready && self.cpu_tstates - start < 2 * self.tstates_per_frame {
            self.tick_tstate();
        }
        self.cart1.sound.end_frame();
        self.cart2.sound.end_frame();
        if let Some(music) = &mut self.msx_music {
            drop(music.ym.take_samples());
        }
        if self.native_frame_ready {
            self.frame_count += 1;
        }
        (self.cpu_tstates - start, self.native_frame_ready)
    }

    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_tstates + self.tstates_per_frame;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        self.cart1.sound.end_frame();
        self.cart2.sound.end_frame();
        if let Some(music) = &mut self.msx_music {
            drop(music.ym.take_samples());
        }
        self.frame_count += 1;
        self.tstates_per_frame
    }

    /// Advance one Z80 T-state. The cadence — two CPU half-cycles, the
    /// interrupt pin fed before each — is `emu198x-z80-cadence`'s; the M1
    /// wait stretch wraps the CPU tick, the VDP accumulates per half-cycle
    /// after it, and the PSG ticks once per T-state, as the hand-rolled
    /// loop always had them.
    fn tick_tstate(&mut self) {
        self.advance_tstates(1);
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
                self.mem_read_side_effects(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // MSX BIOS sets IM 1 — INT fetches RST 38h via the
                // floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    /// Which primary slot is selected for the page containing `addr`. PPI
    /// port A holds two bits per page.
    fn primary_slot(&self, addr: u16) -> u8 {
        let page = (addr >> 14) as usize;
        (self.ppi.port_a() >> (page * 2)) & 0x03
    }

    /// The slot the page containing `addr` reaches: the primary slot, and
    /// in an expanded one the subslot its register selects for that page.
    fn resolve_slot(&self, addr: u16) -> SlotAddress {
        let primary = self.primary_slot(addr);
        if self.layout.expanded[usize::from(primary)] {
            let page = addr >> 14;
            let secondary = (self.subslots[usize::from(primary)] >> (page * 2)) & 0x03;
            SlotAddress::expanded(primary, secondary)
        } else {
            SlotAddress::primary(primary)
        }
    }

    /// The expanded slot whose subslot register `$FFFF` reaches, if page 3
    /// is in one.
    fn subslot_register(&self, addr: u16) -> Option<usize> {
        let primary = usize::from(self.primary_slot(addr));
        (addr == 0xFFFF && self.layout.expanded[primary]).then_some(primary)
    }

    fn in_ram(&self, slot: SlotAddress, addr: u16) -> bool {
        self.layout
            .ram
            .iter()
            .any(|region| region.slot == slot && (region.start..=region.end).contains(&addr))
    }

    fn mem_read(&self, addr: u16) -> u8 {
        if let Some(primary) = self.subslot_register(addr) {
            return !self.subslots[primary];
        }
        let slot = self.resolve_slot(addr);
        if slot == self.layout.bios && usize::from(addr) < self.bios_rom.len().max(0x8000) {
            self.bios_rom.get(addr as usize).copied().unwrap_or(0xFF)
        } else if let Some(value) = self.msx2_read(slot, addr) {
            value
        } else if let Some(value) = self
            .msx_music
            .as_ref()
            .filter(|music| music.slot == slot)
            .and_then(|music| music.read(addr))
        {
            value
        } else if slot == self.layout.cartridge_ports[0] {
            self.cart1.read(addr)
        } else if slot == self.layout.cartridge_ports[1] {
            self.cart2.read(addr)
        } else if self.in_ram(slot, addr) {
            self.ram[addr as usize]
        } else {
            0xFF
        }
    }

    /// What a CPU read does besides return a value: only an SCC's
    /// deformation register notices.
    fn mem_read_side_effects(&mut self, addr: u16) {
        if self.subslot_register(addr).is_some() {
            return;
        }
        let slot = self.resolve_slot(addr);
        if slot == self.layout.cartridge_ports[0] {
            self.cart1.read_side_effects(addr);
        } else if slot == self.layout.cartridge_ports[1] {
            self.cart2.read_side_effects(addr);
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        if let Some(primary) = self.subslot_register(addr) {
            self.subslots[primary] = value;
            return;
        }
        let slot = self.resolve_slot(addr);
        if self.msx2_write(slot, addr, value) {
            return;
        }
        if slot == self.layout.cartridge_ports[0] {
            self.cart1.write(addr, value);
        } else if slot == self.layout.cartridge_ports[1] {
            self.cart2.write(addr, value);
        } else if self.in_ram(slot, addr) {
            self.ram[addr as usize] = value;
        }
    }

    /// The offset into the mapped RAM that `addr` reaches.
    fn mapper_offset(parts: &Msx2Parts, addr: u16) -> usize {
        let segments = (parts.mapper_ram.len() / 0x4000).max(1);
        let segment = usize::from(parts.mapper_registers[usize::from(addr >> 14)]) % segments;
        segment * 0x4000 + usize::from(addr & 0x3FFF)
    }

    /// A read from the SUB-ROM or the mapped RAM, if `slot` holds either.
    fn msx2_read(&self, slot: SlotAddress, addr: u16) -> Option<u8> {
        let parts = self.msx2.as_ref()?;
        if self.layout.sub_rom == Some(slot) && addr < 0x4000 {
            return Some(
                parts
                    .sub_rom
                    .get(usize::from(addr))
                    .copied()
                    .unwrap_or(0xFF),
            );
        }
        if self.layout.mapper.map(|mapper| mapper.slot) == Some(slot) {
            let offset = Self::mapper_offset(parts, addr);
            return Some(parts.mapper_ram.get(offset).copied().unwrap_or(0xFF));
        }
        None
    }

    /// A write to the mapped RAM, if `slot` holds it. A write to the
    /// SUB-ROM goes nowhere.
    fn msx2_write(&mut self, slot: SlotAddress, addr: u16, value: u8) -> bool {
        let Some(parts) = self.msx2.as_mut() else {
            return false;
        };
        if self.layout.sub_rom == Some(slot) && addr < 0x4000 {
            return true;
        }
        if self.layout.mapper.map(|mapper| mapper.slot) == Some(slot) {
            let offset = Self::mapper_offset(parts, addr);
            if let Some(byte) = parts.mapper_ram.get_mut(offset) {
                *byte = value;
            }
            return true;
        }
        false
    }

    /// Where the BIOS, RAM and cartridge ports sit.
    #[must_use]
    pub fn layout(&self) -> &MsxLayout {
        &self.layout
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let pc = self.cpu.regs.pc;
        let value = match port as u8 {
            port @ 0x98..=0x9B => self.video.read(port),
            0xB5 => self
                .msx2
                .as_ref()
                .map_or(0xFF, |parts| parts.rtc.read(parts.rtc_latch) | 0xF0),
            port @ 0xFC..=0xFF => match (&self.msx2, self.layout.mapper) {
                (Some(parts), Some(_)) => parts.mapper_registers[usize::from(port & 3)],
                _ => 0xFF,
            },
            0xA2 => {
                // Reading PSG register 14 is the joystick read. Present the
                // selected port's active-low byte through the chip's port-A
                // input mask; register 15 bit 6 selects port 1 (0) or 2 (1).
                if self.psg.selected_register() == 14 {
                    let selected = usize::from((self.psg.registers()[15] >> 6) & 1);
                    self.psg.set_port_a_input_mask(self.joystick[selected]);
                }
                self.psg.read_data()
            }
            0xA8 => self.ppi.read(0),
            0xA9 => {
                // Port B = keyboard column for the row selected by
                // PPI port C bits 0-3.
                let row = usize::from(self.ppi.port_c() & 0x0F);
                if row < self.keyboard.len() {
                    self.keyboard[row]
                } else {
                    0xFF
                }
            }
            0xAA => self.ppi.read(2),
            0xAB => self.ppi.read(3),
            _ => 0xFF,
        };
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc,
                port: port as u8,
                value,
                write: false,
            });
        }
        value
    }

    fn io_write(&mut self, port: u16, value: u8) {
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc: self.cpu.regs.pc,
                port: port as u8,
                value,
                write: true,
            });
        }
        match port as u8 {
            port @ 0x98..=0x9B => self.video.write(port, value),
            0xB4 => {
                if let Some(parts) = &mut self.msx2 {
                    parts.rtc_latch = value & 0x0F;
                }
            }
            0xB5 => {
                if let Some(parts) = &mut self.msx2 {
                    parts.rtc.write(parts.rtc_latch, value);
                }
            }
            port @ 0xFC..=0xFF => {
                if let Some(parts) = &mut self.msx2 {
                    parts.mapper_registers[usize::from(port & 3)] = value;
                }
            }
            0x7C | 0x7D => {
                if let Some(music) = &mut self.msx_music {
                    music.ym.write((port & 1) as u8, value);
                }
                self.cart1.sound.write_io(port as u8, value);
                self.cart2.sound.write_io(port as u8, value);
            }
            0xA0 => self.psg.select_register(value),
            0xA1 => {
                if let Some(w) = &mut self.ay_watch {
                    w.record(self.cpu.regs.pc, self.psg.selected_register(), value);
                }
                self.psg.write_data(value);
            }
            0xA8 => self.ppi.write(0, value),
            0xAA => self.ppi.write(2, value),
            0xAB => self.ppi.write(3, value),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32) — the active display plus the canonical
    /// TV-visible border. A V9938's columns are half a dot wide, so its
    /// 256-wide modes fill two columns a pixel. Exact dimensions reported by
    /// [`framebuffer_width`](Self::framebuffer_width) and
    /// [`framebuffer_height`](Self::framebuffer_height).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.video.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.video.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.video.framebuffer_height()
    }

    /// Press a key at the given (row, bit) matrix cell.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given (row, bit) matrix cell.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] |= 1 << bit;
        }
    }

    /// Mutable keyboard matrix (active-low; 0 = pressed).
    pub fn keyboard_mut(&mut self) -> &mut [u8; 11] {
        &mut self.keyboard
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// The MSX1's VDP; `None` on an MSX2.
    #[must_use]
    pub fn tms9918(&self) -> Option<&Tms9918> {
        match &self.video {
            Video::Tms9918(vdp) => Some(vdp),
            Video::V9938(_) => None,
        }
    }

    /// The MSX2's VDP; `None` on an MSX1.
    #[must_use]
    pub fn v9938(&self) -> Option<&V9938> {
        match &self.video {
            Video::Tms9918(_) => None,
            Video::V9938(vdp) => Some(vdp),
        }
    }

    /// The scanline the VDP is drawing.
    #[must_use]
    pub fn vdp_scanline(&self) -> u16 {
        match &self.video {
            Video::Tms9918(vdp) => vdp.scanline(),
            Video::V9938(vdp) => vdp.line(),
        }
    }

    /// PSG reference.
    #[must_use]
    pub fn psg(&self) -> &Ay3_8912 {
        &self.psg
    }

    /// PSG mutable reference (for the host to call `end_frame` on per
    /// audio frame).
    pub fn psg_mut(&mut self) -> &mut Ay3_8912 {
        &mut self.psg
    }

    /// The PPI as the MSX board wires it. Until the BIOS sets the mode,
    /// every port is an input and its lines are undriven. The MSX
    /// standard has every page in primary slot 0 after reset, which is
    /// where the CPU must find the BIOS, so port A's slot-select lines
    /// read low; port C's lines do too.
    fn new_ppi() -> Ppi8255 {
        let mut ppi = Ppi8255::new();
        ppi.input_a = 0x00;
        ppi.input_c = 0x00;
        ppi
    }

    /// The upper PPI nibble actively drives these board controls in output
    /// mode. An input-configured latch is not an asserted output control.
    fn ppi_upper_output(&self) -> Option<u8> {
        (self.ppi.mode() & 0x08 == 0).then(|| self.ppi.port_c())
    }

    /// Driven CAPS-lamp control: C6 is active-low.
    #[must_use]
    pub fn caps_led_on(&self) -> bool {
        self.ppi_upper_output().is_some_and(|v| v & 0x40 == 0)
    }

    /// Driven cassette motor control: C4 is active-low.
    /// This exposes the line; it does not implement cassette transport.
    #[must_use]
    pub fn cassette_motor_on(&self) -> bool {
        self.ppi_upper_output().is_some_and(|v| v & 0x10 == 0)
    }

    /// Driven cassette write-data level on C5.
    #[must_use]
    pub fn cassette_output(&self) -> bool {
        self.ppi_upper_output().is_some_and(|v| v & 0x20 != 0)
    }

    /// Driven keyboard-click DAC level on C7.
    #[must_use]
    pub fn key_click_output(&self) -> bool {
        self.ppi_upper_output().is_some_and(|v| v & 0x80 != 0)
    }

    /// PPI reference.
    #[must_use]
    pub fn ppi(&self) -> &Ppi8255 {
        &self.ppi
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> MsxRegion {
        self.region
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Observe one byte on the Z80 bus without side effects (slot
    /// resolution against current PPI port A, then BIOS / cartridge /
    /// RAM read). Mirrors the private `mem_read`; exposed for host
    /// debugging tools (`memory_read` MCP, watch points, etc.).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Write one byte through the slot-resolved bus (RAM accepts it; BIOS /
    /// unmapped slots ignore it). For host debugging (`poke_*` MCP tools).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace.
    pub fn start_io_trace(&mut self) {
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }

    /// Start (or restart) capturing PSG register writes for `watch_ay_*`.
    /// Returns the log capacity (max records before writes are dropped).
    pub fn start_ay_write_watch(&mut self) -> u32 {
        let watch = AyWriteWatch::new();
        let cap = watch.cap() as u32;
        self.ay_watch = Some(watch);
        cap
    }

    /// Stop capturing PSG writes and drop the log.
    pub fn stop_ay_write_watch(&mut self) {
        self.ay_watch = None;
    }

    /// Captured PSG writes since the last `start_ay_write_watch`, or
    /// `None` when the watch is disarmed.
    #[must_use]
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.ay_watch.as_ref().map(AyWriteWatch::records)
    }

    /// Drop captured PSG writes while leaving the watch armed.
    pub fn clear_ay_write_watch_records(&mut self) {
        if let Some(w) = &mut self.ay_watch {
            w.clear();
        }
    }
}

impl Z80Machine for Msx {
    fn hc(&self) -> u64 {
        self.cadence_hc
    }

    fn hc_mut(&mut self) -> &mut u64 {
        &mut self.cadence_hc
    }

    // VDP INT → Z80 IRQ, fed before the tick, not after. The Z80
    // samples `/INT` at an instruction boundary during its own
    // tick, so setting the line afterwards hands it the VDP's
    // state from the previous half-cycle. MSX has no separate NMI
    // source. See
    // `knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md`.
    fn feed_interrupt_pins(&mut self) {
        self.cpu.irq = self.video.interrupt();
    }

    fn tick_cpu_and_bus(&mut self) {
        // Standard MSX hardware stretches every opcode-fetch (M1) bus
        // cycle by one T-state. The Z80 samples WAIT during T2; assert it
        // for one T-state, then release it so the held phase can advance.
        // Refresh is also marked M1, but is not a memory read, hence the
        // MREQ + RD qualification. Sources: `reference/by-system/msx/
        // msx-reference.md`; openMSX `src/cpu/Z80.hh::WAIT_CYCLES`.
        if !self.cpu.m1 {
            self.m1_wait_phase = 0;
        }
        if self.cpu.m1 && self.cpu.mreq && self.cpu.rd && self.m1_wait_phase == 0 {
            // `Z80::tick` is a half-cycle, so a one-T-state wait holds
            // T2Rise for two calls.
            self.m1_wait_phase = 2;
        }
        self.cpu.wait = matches!(self.m1_wait_phase, 1 | 2);
        self.cpu.tick();
        if self.m1_wait_phase == 2 {
            self.m1_wait_phase = 1;
        } else if self.m1_wait_phase == 1 {
            self.m1_wait_phase = 3;
        }
        self.handle_bus();
    }

    fn tick_chips_halfcycle(&mut self) {
        // Sample after the CPU's port-write edge. Two half-cycle charges
        // compose one T-state in the existing mixer; old samples retain
        // their original level when a later BSR/mode write changes C7.
        self.expansion
            .accumulate_click_halfcycle(self.key_click_output());
        // VDP dots at 3:2 against T-states, accumulated per
        // half-cycle so the frame interrupt lands at the correct
        // scanline relative to CPU execution.
        self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.native_frame_ready |= self.video.tick();
            self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }
    }

    fn tick_chips(&mut self) {
        // The cartridges' and MSX-MUSIC's chips run at the CPU clock.
        let mut level = self.cart1.sound.tick() + self.cart2.sound.tick();
        if let Some(music) = &mut self.msx_music {
            level += music.tick();
        }
        self.expansion.accumulate(level);

        // PSG runs at CPU ÷ 2.
        self.psg_phase ^= 1;
        if self.psg_phase == 0 {
            self.psg.tick();
            self.expansion.psg_tick();
        }

        if let Some(parts) = &mut self.msx2 {
            parts.rtc_phase += emu198x_ricoh_rp5c01::TICK_HZ;
            if parts.rtc_phase >= CPU_CLOCK_HZ {
                parts.rtc_phase -= CPU_CLOCK_HZ;
                parts.rtc.tick();
            }
        }

        self.cpu_tstates += 1;
    }
}

impl emu198x_zilog_z80::Z80Stepper for Msx {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
    }
}

#[cfg(test)]
mod note_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppi_output_controls_follow_direction_bsr_and_mode_reset() {
        let mut machine = Msx::new(trap_bios(), MsxRegion::Ntsc);
        assert!(!machine.caps_led_on() && !machine.cassette_motor_on());
        assert!(!machine.cassette_output() && !machine.key_click_output());
        machine.io_write(0xAB, 0x82);
        machine.io_write(0xAA, 0xAA);
        assert!(machine.caps_led_on() && machine.cassette_motor_on());
        assert!(machine.cassette_output() && machine.key_click_output());
        assert_eq!(machine.ppi.port_c() & 15, 10);
        machine.io_write(0xAB, 0x0E);
        assert!(!machine.key_click_output());
        assert!(machine.caps_led_on() && machine.cassette_output());
        machine.io_write(0xAB, 9); // set C4: motor off
        machine.io_write(0xAB, 13); // set C6: CAPS off
        machine.io_write(0xAB, 10); // clear C5
        assert!(!machine.cassette_motor_on() && !machine.caps_led_on());
        assert!(!machine.cassette_output());
        assert_eq!(
            machine.ppi.port_c() & 15,
            10,
            "upper BSR preserves keyboard row"
        );
        machine.io_write(0xAB, 0x8A); // upper nibble becomes input
        machine.io_write(0xAB, 0x0F); // latch high, still undriven
        assert!(!machine.key_click_output());
        assert!(!machine.caps_led_on() && !machine.cassette_motor_on());
        machine.io_write(0xAB, 0x82); // mode write clears the output latches
        assert!(!machine.key_click_output() && !machine.cassette_output());
        assert!(
            machine.caps_led_on() && machine.cassette_motor_on(),
            "active-low controls"
        );
        assert_eq!(machine.ppi.port_c() & 15, 0);
    }

    #[test]
    fn click_integrates_both_halfcycles_and_preserves_mid_sample_history() {
        let mut full = booted(MsxRegion::Ntsc);
        let mut half = booted(MsxRegion::Ntsc);
        for _ in 0..1000 {
            full.io_write(0xAB, 0x0F);
            half.io_write(0xAB, 0x0F);
            full.advance_halfcycles(1);
            half.advance_halfcycles(1);
            half.io_write(0xAB, 0x0E);
            full.advance_halfcycles(1);
            half.advance_halfcycles(1);
        }
        let mut full_audio = [0.0; 1024];
        let mut half_audio = [0.0; 1024];
        let count = full.end_audio_frame(&mut full_audio);
        assert_eq!(half.end_audio_frame(&mut half_audio), count);
        assert!(full_audio.iter().any(|s| s.abs() > 0.000_001));
        for (full, half) in full_audio.iter().zip(&half_audio) {
            assert!(
                (half - full * 0.5).abs() < 0.000_001,
                "one high half must contribute half the charge"
            );
        }
        // Save with a partial output sample at the supported T-state boundary.
        // The existing snapshot skips cadence_hc; arbitrary half-T-state
        // restores are outside that contract.
        half.io_write(0xAB, 0x0F);
        half.advance_halfcycles(2);
        let bytes = postcard::to_allocvec(&half).expect("mid-sample state");
        let mut restored: Msx = postcard::from_bytes(&bytes).expect("restore");
        restored.cpu_mut().rehydrate_walker_sequence();
        for machine in [&mut half, &mut restored] {
            machine.io_write(0xAB, 0x0E);
            machine.advance_halfcycles(2);
            for edge in 0..24 {
                machine.io_write(0xAB, if edge % 2 == 0 { 0x0F } else { 0x0E });
                machine.advance_tstates(29);
            }
        }
        let mut replay_audio = [0.0; 1024];
        let mut continued_audio = [0.0; 1024];
        assert_eq!(
            half.end_audio_frame(&mut continued_audio),
            restored.end_audio_frame(&mut replay_audio)
        );
        assert_eq!(continued_audio, replay_audio);
        assert_eq!(
            postcard::to_allocvec(&half).expect("continued"),
            postcard::to_allocvec(&restored).expect("replayed")
        );
    }

    #[test]
    fn ppi_click_reaches_generated_audio_and_is_not_applied_at_drain() {
        let render = |toggle: bool| {
            let mut machine = booted(MsxRegion::Ntsc);
            for edge in 0..16 {
                machine.io_write(0xAB, if toggle && edge % 2 == 0 { 0x0F } else { 0x0E });
                for _ in 0..500 {
                    machine.tick_tstate();
                }
            }
            machine.io_write(0xAB, 0x0E); // final low must not erase earlier sound
            let mut audio = [0.0; 1024];
            let count = machine.end_audio_frame(&mut audio);
            assert!(count > 0);
            audio
        };
        let silent = render(false);
        assert!(silent.iter().all(|s| *s == 0.0));
        let click = render(true);
        assert!(click.iter().all(|s| s.is_finite()));
        assert!(
            click.iter().any(|s| s.abs() > 0.000_001),
            "PPI SOUND must reach the mixer"
        );
    }

    #[test]
    fn native_frame_stops_at_the_chip_wrap_after_partial_stepping() {
        for region in [MsxRegion::Ntsc, MsxRegion::Pal] {
            let mut machine = booted(region);
            let line = |m: &Msx| match &m.video {
                Video::Tms9918(vdp) => vdp.scanline(),
                _ => panic!("MSX1 fixture"),
            };
            for _ in 0..37 {
                machine.tick_tstate();
            }
            for _ in 0..4 {
                let bytes = postcard::to_allocvec(&machine).expect("encode partial state");
                let mut direct: Msx = postcard::from_bytes(&bytes).expect("restore control");
                let start = direct.cpu_tstates;
                loop {
                    let before = line(&direct);
                    direct.tick_tstate();
                    assert!(direct.cpu_tstates - start < 200_000, "control must wrap");
                    if line(&direct) < before {
                        break;
                    }
                }
                let (ticks, completed) = machine.run_native_frame();
                assert!(completed);
                assert_eq!(
                    ticks,
                    direct.cpu_tstates - start,
                    "elapsed time must match the chip event"
                );
                assert_eq!(
                    line(&machine),
                    0,
                    "raster must wrap, not merely consume a nominal budget"
                );
                assert_eq!(machine.cpu_tstates, direct.cpu_tstates);
                // A debugger can leave us partway through the next field.
                for _ in 0..53 {
                    machine.tick_tstate();
                }
            }
        }
    }

    /// An MSX whose 8255 has the mode the BIOS sets at reset: port A
    /// (slot select) out, port B (keyboard) in, port C out. The trap BIOS
    /// never runs that code.
    fn booted(region: MsxRegion) -> Msx {
        let mut sys = Msx::new(trap_bios(), region);
        sys.ppi.write(3, 0x82);
        sys
    }

    fn trap_bios() -> Vec<u8> {
        // 32 KB BIOS-shaped ROM full of NOPs with a JR -2 trap at
        // $0008 — gives the Z80 something legal to run for boot-
        // structure tests.
        let mut rom = vec![0u8; 32768];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    /// The VG 8020-20's layout: RAM in subslot 2 of an expanded slot 3.
    fn expanded_slot_3() -> Msx {
        let layout = MsxLayout {
            ram: vec![RamRegion {
                slot: SlotAddress::expanded(3, 2),
                start: 0x0000,
                end: 0xFFFF,
            }],
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        };
        let mut sys = Msx::with_layout(trap_bios(), MsxRegion::Ntsc, layout);
        sys.ppi.write(3, 0x82);
        sys
    }

    #[test]
    fn the_subslot_register_reads_back_complemented() {
        let mut sys = expanded_slot_3();
        sys.ppi.write(0, 0b1100_0000); // page 3 -> slot 3
        sys.poke(0xFFFF, 0b1010_0000);
        assert_eq!(sys.peek(0xFFFF), 0b0101_1111);
    }

    #[test]
    fn ram_answers_only_in_its_subslot() {
        let mut sys = expanded_slot_3();
        sys.ppi.write(0, 0b1100_0000);
        sys.poke(0xFFFF, 0b1000_0000); // page 3 -> subslot 2
        sys.poke(0xC000, 0x5A);
        assert_eq!(sys.peek(0xC000), 0x5A);
        sys.poke(0xFFFF, 0b0100_0000); // page 3 -> subslot 1
        assert_eq!(sys.peek(0xC000), 0xFF, "subslot 1 is empty");
        sys.poke(0xC000, 0x11);
        sys.poke(0xFFFF, 0b1000_0000);
        assert_eq!(
            sys.peek(0xC000),
            0x5A,
            "the write to subslot 1 went nowhere"
        );
    }

    #[test]
    fn an_unexpanded_slot_has_no_subslot_register() {
        let mut sys = booted(MsxRegion::Ntsc);
        sys.ppi.write(0, 0b1100_0000);
        sys.poke(0xFFFF, 0x42);
        assert_eq!(sys.peek(0xFFFF), 0x42, "$FFFF is plain RAM");
    }

    #[test]
    fn ram_beside_the_bios_covers_only_its_pages() {
        let layout = MsxLayout {
            ram: vec![RamRegion {
                slot: SlotAddress::primary(0),
                start: 0x8000,
                end: 0xFFFF,
            }],
            ..MsxLayout::default()
        };
        let mut sys = Msx::with_layout(trap_bios(), MsxRegion::Ntsc, layout);
        sys.ppi.write(3, 0x82);
        sys.ppi.write(0, 0); // every page in slot 0
        sys.poke(0x0009, 0x00);
        assert_eq!(sys.peek(0x0009), 0xFE, "the BIOS is read-only");
        sys.poke(0x8000, 0x77);
        assert_eq!(sys.peek(0x8000), 0x77, "RAM fills pages 2 and 3");
    }

    /// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP +
    /// AY-3-8910 PSG + 8255 PPI + RAM + slot/bank state), not cold-boot from
    /// the ROM. Serialise, advance (so the state differs), then deserialise
    /// the first snapshot and confirm re-serialising it is byte-identical —
    /// every stateful field round-trips, including the VDP's 16 KB VRAM.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = booted(MsxRegion::Ntsc);
        sys.run_frame();
        // RAM lives in slot 3; at reset PPI port A = 0 selects slot 0 for
        // every page, so route page 3 ($C000-$FFFF) to slot 3 before poking.
        sys.ppi.write(0, 0b1100_0000); // page 3 → slot 3 (RAM)
        sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0xC100), 0xA5, "0xC100 is RAM and accepts the poke");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Msx = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = booted(MsxRegion::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ay_watch_captures_psg_data_writes() {
        let mut sys = booted(MsxRegion::Ntsc);
        // Disarmed by default: no records.
        assert!(sys.ay_write_watch_records().is_none());

        let cap = sys.start_ay_write_watch();
        assert!(cap > 0, "start reports a non-zero capacity");

        // Select R7 (mixer), write a value; select R8 (channel-A volume),
        // write another. Each data write ($A1) should capture the register
        // selected by the preceding $A0 write.
        sys.io_write(0xA0, 7);
        sys.io_write(0xA1, 0x38);
        sys.io_write(0xA0, 8);
        sys.io_write(0xA1, 0x0F);

        let records = sys.ay_write_watch_records().expect("watch is armed");
        assert_eq!(records.len(), 2, "two data writes captured");
        assert_eq!((records[0].register, records[0].value), (7, 0x38));
        assert_eq!((records[1].register, records[1].value), (8, 0x0F));

        // A bare register-select ($A0) is not a data write — still two.
        sys.io_write(0xA0, 1);
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 2);

        sys.clear_ay_write_watch_records();
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 0);

        sys.stop_ay_write_watch();
        assert!(sys.ay_write_watch_records().is_none());
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = booted(MsxRegion::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = booted(MsxRegion::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn default_slot_layout_reads_bios_then_ram() {
        // PPI port A defaults to 0 → slot 0 selected for all four
        // pages. Page 0 (BIOS NOP region) should read 0.
        let sys = booted(MsxRegion::Ntsc);
        assert_eq!(sys.mem_read(0x0000), 0);
        // BIOS-trap byte at $0008.
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Page 2+ reads slot 0 with addr >= 0x8000 — returns 0xFF.
        assert_eq!(sys.mem_read(0xC000), 0xFF);
    }

    #[test]
    fn slot_select_routes_pages() {
        let mut sys = booted(MsxRegion::Ntsc);
        // Select slot 3 (RAM) for page 3 only.
        sys.ppi.write(0, 0b1100_0000); // page 3 → slot 3
        sys.mem_write(0xC000, 0xAB);
        sys.mem_write(0xC001, 0xCD);
        assert_eq!(sys.mem_read(0xC000), 0xAB);
        assert_eq!(sys.mem_read(0xC001), 0xCD);
        // Page 0 still reads BIOS (slot 0).
        assert_eq!(sys.mem_read(0x0008), 0x18);
    }

    #[test]
    fn joystick_reads_through_psg_port_a() {
        let mut sys = booted(MsxRegion::Ntsc);

        // Port 1 is selected by default (R15 bit 6 = 0). Press up + fire.
        sys.set_joystick(1, true, false, false, false, true);
        sys.io_write(0xA0, 14); // select PSG register 14 (port A)
        let p1 = sys.io_read(0xA2);
        assert_eq!(p1 & 0x01, 0, "up pressed → bit 0 low");
        assert_eq!(p1 & 0x10, 0, "fire pressed → bit 4 (trigger A) low");
        assert_eq!(p1 & 0x0E, 0x0E, "down/left/right idle high");

        // Release → idle high across the board.
        sys.set_joystick(1, false, false, false, false, false);
        sys.io_write(0xA0, 14);
        assert_eq!(sys.io_read(0xA2), 0xFF, "released joystick reads idle 0xFF");

        // Selecting port 2 (R15 bit 6 = 1) reads the other stick independently.
        sys.set_joystick(2, false, true, false, false, false);
        sys.io_write(0xA0, 15); // select R15 (port B)
        sys.io_write(0xA1, 0x40); // bit 6 = 1 → joystick port 2
        sys.io_write(0xA0, 14);
        let p2 = sys.io_read(0xA2);
        assert_eq!(p2 & 0x02, 0, "port 2 down → bit 1 low");
        assert_eq!(p2 & 0x01, 0x01, "port 2 up idle high");
    }

    #[test]
    fn keyboard_press_and_release_round_trip() {
        let mut sys = booted(MsxRegion::Ntsc);
        sys.press_key(2, 3);
        assert_eq!(sys.keyboard[2] & 0b0000_1000, 0);
        sys.release_key(2, 3);
        assert_eq!(sys.keyboard[2] & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn keyboard_io_returns_selected_row() {
        let mut sys = booted(MsxRegion::Ntsc);
        sys.keyboard[5] = 0x42;
        // Select row 5 via PPI port C bits 0-3.
        sys.ppi.write(2, 0x05);
        assert_eq!(sys.io_read(0xA9), 0x42);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = booted(MsxRegion::Ntsc);
        let start = sys.vdp_scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp_scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn cart_plain_mapper_reads_at_0x4000() {
        let mut sys = booted(MsxRegion::Ntsc);
        let mut cart = vec![0u8; 0x4000];
        cart[0] = 0x42;
        cart[1] = 0xAA;
        sys.insert_cart1(cart, MapperType::Plain);
        // Select slot 1 for page 1 ($4000-$7FFF).
        sys.ppi.write(0, 0b0000_0100);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        assert_eq!(sys.mem_read(0x4001), 0xAA);
    }

    fn mapped_rom_with_writes(addresses: &[u16]) -> Vec<u8> {
        let mut rom = vec![0; 128 * 1024];
        for (index, address) in addresses.iter().copied().enumerate() {
            let offset = index * 3;
            rom[offset] = 0x32;
            rom[offset + 1..offset + 3].copy_from_slice(&address.to_le_bytes());
        }
        rom
    }

    #[test]
    fn mapper_detection_keeps_small_and_unrecognised_images_plain() {
        assert_eq!(detect_mapper(&vec![0; 32 * 1024]), MapperType::Plain);
        assert_eq!(detect_mapper(&vec![0; 128 * 1024]), MapperType::Plain);
    }

    #[test]
    fn mapper_detection_scores_each_supported_register_family() {
        assert_eq!(
            detect_mapper(&mapped_rom_with_writes(&[0x8000, 0xa000, 0x6000])),
            MapperType::Konami
        );
        assert_eq!(
            detect_mapper(&mapped_rom_with_writes(&[0x5000, 0x9000, 0xb000])),
            MapperType::KonamiScc
        );
        assert_eq!(
            detect_mapper(&mapped_rom_with_writes(&[0x6800, 0x7800, 0x6800])),
            MapperType::Ascii8
        );
        assert_eq!(
            detect_mapper(&mapped_rom_with_writes(&[0x6000, 0x7000, 0x77ff])),
            MapperType::Ascii16
        );
    }

    /// The C-BIOS MSX2 layout: the SUB-ROM in subslot 0 and a 128 KB mapper
    /// in subslot 2 of an expanded slot 3.
    fn msx2_layout() -> MsxLayout {
        MsxLayout {
            sub_rom: Some(SlotAddress::expanded(3, 0)),
            ram: Vec::new(),
            mapper: Some(MemoryMapper {
                slot: SlotAddress::expanded(3, 2),
                size_kb: 128,
            }),
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        }
    }

    fn msx2() -> Msx {
        let mut sub_rom = vec![0u8; 0x4000];
        sub_rom[0] = b'C';
        sub_rom[1] = b'D';
        let mut sys = Msx::msx2(trap_bios(), sub_rom, MsxRegion::Pal, msx2_layout());
        sys.ppi.write(3, 0x82);
        sys
    }

    #[test]
    fn an_msx1_has_no_msx2_parts() {
        let mut sys = booted(MsxRegion::Ntsc);
        assert!(!sys.is_msx2());
        assert!(sys.tms9918().is_some() && sys.v9938().is_none());
        assert!(sys.sub_rom().is_none() && sys.mapper_registers().is_none());
        assert!(sys.rtc().is_none() && sys.rtc_mut().is_none());
        sys.io_write(0xFE, 5);
        sys.io_write(0xB4, 13);
        sys.io_write(0xB5, 1);
        assert_eq!(sys.io_read(0xFE), 0xFF);
        assert_eq!(sys.io_read(0xB5), 0xFF);
    }

    #[test]
    fn an_msx2_has_a_v9938_and_a_sub_rom() {
        let mut sys = msx2();
        assert!(sys.is_msx2());
        assert!(sys.v9938().is_some() && sys.tms9918().is_none());
        sys.ppi.write(0, 0b0000_0011); // page 0 -> slot 3
        sys.poke(0xFFFF, 0); // every page of slot 3 -> subslot 0
        assert_eq!((sys.peek(0x0000), sys.peek(0x0001)), (b'C', b'D'));
        sys.poke(0x0000, 0x55);
        assert_eq!(sys.peek(0x0000), b'C', "the SUB-ROM is read-only");
        assert_eq!(sys.sub_rom().map(<[u8]>::len), Some(0x4000));
        assert_eq!(sys.framebuffer_width(), 556, "the V9938's PAL framebuffer");
    }

    #[test]
    fn the_mapper_selects_a_segment_a_page() {
        let mut sys = msx2();
        sys.ppi.write(0, 0xFF); // every page -> slot 3
        sys.poke(0xFFFF, 0b1010_1010); // every page -> subslot 2
        for (page, segment) in [3u8, 2, 1, 0].into_iter().enumerate() {
            sys.io_write(0xFC + page as u16, segment);
        }
        sys.poke(0x0000, 0x33);
        sys.poke(0xC000, 0x00);
        assert_eq!(sys.mapper_registers(), Some([3, 2, 1, 0]));
        // Segment 3 behind page 2 shows what page 0 wrote.
        sys.io_write(0xFE, 3);
        assert_eq!(sys.peek(0x8000), 0x33);
        assert_eq!(sys.io_read(0xFE), 3, "all eight bits read back");
        // 128 KB is eight segments, so segment 11 is segment 3.
        sys.io_write(0xFE, 11);
        assert_eq!(sys.peek(0x8000), 0x33);
        // Plain RAM elsewhere is untouched; the old 64 KB store is not used.
        assert_eq!(sys.ram[0], 0);
    }

    #[test]
    fn mapper_ports_read_ff_without_a_mapper() {
        let layout = MsxLayout {
            sub_rom: Some(SlotAddress::expanded(3, 0)),
            ram: vec![RamRegion {
                slot: SlotAddress::expanded(3, 3),
                start: 0x0000,
                end: 0xFFFF,
            }],
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        };
        let mut sys = Msx::msx2(trap_bios(), vec![0; 0x4000], MsxRegion::Ntsc, layout);
        sys.ppi.write(3, 0x82);
        sys.io_write(0xFC, 2);
        assert_eq!(sys.io_read(0xFC), 0xFF);
        sys.ppi.write(0, 0xC0);
        sys.poke(0xFFFF, 0xC0);
        sys.poke(0xC000, 0x99);
        assert_eq!(sys.peek(0xC000), 0x99, "plain RAM in subslot 3");
    }

    #[test]
    fn the_rtc_answers_on_b4_and_b5() {
        let mut sys = msx2();
        sys.io_write(0xB4, 13);
        sys.io_write(0xB5, 0x8 | 2); // timer on, block 2
        sys.io_write(0xB4, 0);
        sys.io_write(0xB5, 0xA);
        assert_eq!(sys.io_read(0xB5), 0xFA, "a nibble with the high bits set");
        assert_eq!(sys.rtc().map(|rtc| rtc.ram()[0]), Some(0xA));
        if let Some(rtc) = sys.rtc_mut() {
            rtc.write(13, 0x8);
        }
        assert_eq!(sys.rtc().map(|rtc| rtc.date_time().second), Some(0));
        // Two seconds of CPU time move the clock on two seconds.
        for _ in 0..2 * 50 {
            sys.run_frame();
        }
        let seconds = sys.rtc().map_or(0, |rtc| rtc.date_time().second);
        assert!((1..=2).contains(&seconds), "{seconds} seconds passed");
    }

    #[test]
    fn the_v9938_takes_palette_and_indirect_writes() {
        let mut sys = msx2();
        // R#16 = 2, then entry 2 = red 7, blue 3, green 5.
        sys.io_write(0x99, 2);
        sys.io_write(0x99, 0x80 | 16);
        sys.io_write(0x9A, 0x73);
        sys.io_write(0x9A, 0x05);
        assert_eq!(sys.v9938().map(|vdp| vdp.palette()[2]), Some(0x573));
        // R#17 = 7, then R#7 through port $9B.
        sys.io_write(0x99, 0x80 | 7);
        sys.io_write(0x99, 0x80 | 17);
        sys.io_write(0x9B, 0x4C);
        assert_eq!(sys.v9938().map(|vdp| vdp.register(7)), Some(0x4C));
        assert_eq!(sys.io_read(0x9A), 0xFF, "the palette port is write-only");
    }

    #[test]
    fn a_long_bios_maps_its_logo_rom_at_8000() {
        let mut bios = trap_bios();
        bios.resize(0xC000, 0x77);
        let mut sys = Msx::msx2(bios, vec![0; 0x4000], MsxRegion::Ntsc, msx2_layout());
        sys.ppi.write(3, 0x82);
        sys.ppi.write(0, 0);
        assert_eq!(sys.peek(0x8000), 0x77);
        assert_eq!(sys.peek(0xC000), 0xFF, "nothing above the logo");
    }

    #[test]
    fn an_msx2_snapshot_round_trips() {
        let mut sys = msx2();
        sys.io_write(0xFD, 6);
        sys.run_frame();
        let bytes = postcard::to_allocvec(&sys).expect("encode");
        let restored: Msx = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(restored.mapper_registers(), Some([0, 6, 0, 0]));
        assert_eq!(postcard::to_allocvec(&restored).expect("re-encode"), bytes);
        assert_eq!(restored.vdp_scanline(), sys.vdp_scanline());
    }
}
