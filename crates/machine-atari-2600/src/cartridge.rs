//! Atari 2600 cartridge handling.
//!
//! Supports 2KB and 4KB (no banking) ROMs, plus F8 (8KB / 2 banks),
//! F6 (16KB / 4 banks), F4 (32KB / 8 banks) bank-switching via
//! hotspot detection. Reads or writes to specific addresses in the
//! `$1000-$1FFF` range trigger bank switches.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-2600/src/cartridge.rs`
//! (2026-06-01).

/// Cartridge banking scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankingScheme {
    /// 2KB or 4KB, no banking.
    None,
    /// F8: 8KB, 2 banks. Hotspots `$1FF8`/`$1FF9`.
    F8,
    /// F6: 16KB, 4 banks. Hotspots `$1FF6-$1FF9`.
    F6,
    /// F4: 32KB, 8 banks. Hotspots `$1FF4-$1FFB`.
    F4,
    /// Parker Brothers E0: 8KB as eight 1KB banks. The 4KB window is four 1KB
    /// slices — slices 0/1/2 are independently switchable (hotspots `$1FE0-$1FE7`,
    /// `$1FE8-$1FEF`, `$1FF0-$1FF7`), slice 3 is fixed to bank 7.
    E0,
    /// CBS RAM+ (FA): 12KB as three 4KB banks (hotspots `$1FF8`/`$1FF9`/`$1FFA`)
    /// plus 256 bytes of on-cart RAM — write port `$1000-$10FF`, read port
    /// `$1100-$11FF`.
    Fa,
    /// EF: 64KB as sixteen 4KB banks, selected by hotspots `$1FE0-$1FEF`.
    /// Address-decode only; the EFSC variant adds the Superchip overlay (the
    /// `superchip` flag), detected independently of the base scheme.
    Ef,
    /// UA Limited: 8KB, two 4KB banks. Unusually, the bank-select hotspots sit
    /// *outside* the cart window — accessing `$0220` selects bank 0 and `$0240`
    /// bank 1 (low TIA-mirror addresses the cart snoops off the bus). The
    /// swapped-hotspot Digivision variant isn't modelled yet.
    Ua,
    /// 0840 "EconoBank": 8KB, two 4KB banks, switched by out-of-window
    /// accesses — `$0800` selects bank 0, `$0840` bank 1 (snooped off the bus).
    EconoBank,
    /// 3E (Tigervision-style + RAM): the `$1000-$17FF` half-window holds either
    /// a 2KB ROM segment (selected by `STA $3F`) or a 1KB RAM bank (`STA $3E`);
    /// `$1800-$1FFF` is fixed to the last ROM segment. Up to 512KB ROM + 32KB
    /// RAM. Distinct from plain 3F, which has no RAM.
    ThreeE,
    /// M-Network E7: 16KB in 2KB segments + 2KB RAM. The `$1000-$17FF` window
    /// holds ROM bank 0-6 or a 1KB RAM bank (hotspots `$1FE0-$1FE7`, where
    /// `$1FE7` selects RAM). A 256-byte RAM strip sits at `$1800-$19FF` (one of
    /// four banks, hotspots `$1FE8-$1FEB`). `$1A00-$1FFF` is fixed to the last
    /// ROM bank. Each RAM region splits into a low write port + high read port.
    E7,
    /// 3F (Tigervision): like 3E but ROM-only — a 2KB ROM segment in the
    /// `$1000-$17FF` window (selected by storing the bank to any address
    /// `$00-$3F`), with `$1800-$1FFF` fixed to the last segment.
    ThreeF,
    /// Activision FE: 8KB, two 4KB banks with no conventional hotspots. The
    /// bank is chosen by snooping the stack — an access to `$01FE` arms a probe,
    /// and the *next* access's bus value selects the bank (`(value >> 5) ^ 7`).
    /// Used by Robot Tank and Decathlon.
    Fe,
    /// DPC (Pitfall II's coprocessor): F8-style 8KB program banking plus a 2KB
    /// graphics ROM streamed through eight data fetchers, an LFSR, and three
    /// music-mode fetchers. Registers at `$1000-$103F` (read) / `$1040-$107F`
    /// (write); reads have side effects (clock the RNG, advance counters). The
    /// music oscillator timing is added separately (#532).
    Dpc,
}

pub struct Cartridge {
    rom: Vec<u8>,
    scheme: BankingScheme,
    bank: usize,
    bank_size: usize,
    /// E0 only: the bank mapped into each of the three switchable 1KB slices.
    /// Slice 3 is always bank 7, so it isn't tracked here.
    e0_segments: [usize; 3],
    /// On-cart RAM. FA: 256 bytes; 3E: 32 KB (32 × 1 KB banks). Empty for
    /// schemes without RAM.
    ram: Vec<u8>,
    /// 3E only: the switchable window holds RAM (`true`) or ROM (`false`).
    three_e_ram_active: bool,
    /// 3E only: ROM 2 KB segment in the window when ROM is active.
    three_e_rom_seg: usize,
    /// 3E only: RAM 1 KB bank in the window when RAM is active.
    three_e_ram_bank: usize,
    /// E7 only: the 256-byte RAM bank (0-3) mapped at `$1800-$19FF`. The
    /// `$1000-$17FF` window bank reuses `bank` (0-7; 7 = the 1 KB RAM).
    e7_ram_bank: usize,
    /// Superchip (SARA) overlay: 128 bytes of RAM at the bottom of the window
    /// (write port `$1000-$107F`, read port `$1080-$10FF`), present in every
    /// bank. Layered over F8/F6/F4/EF when detected; uses the first 128 bytes
    /// of `ram`.
    superchip: bool,
    /// FE only: armed by an access to `$01FE`; the next access's bus value then
    /// selects the bank.
    fe_armed: bool,
    /// DPC data-fetcher state (8 fetchers): top/bottom comparators, 11-bit
    /// counters, and flag registers. Fetchers 5-7 can run in music mode.
    dpc_tops: [u8; 8],
    dpc_bottoms: [u8; 8],
    dpc_counters: [u16; 8],
    dpc_flags: [u8; 8],
    /// DPC music mode for fetchers 5/6/7.
    dpc_music_mode: [bool; 3],
    /// DPC LFSR random-number register (must stay non-zero).
    dpc_rng: u8,
    /// DPC music oscillator: CPU cycles elapsed (driven by [`Self::tick`]), the
    /// cycle of the last music update, the carried fractional OSC clock, and the
    /// CPU clock rate (region-dependent, set by the machine).
    dpc_cycle: u64,
    dpc_audio_cycle: u64,
    dpc_fractional: f64,
    dpc_clock_rate: f64,
}

/// E0 slice size: 1 KB.
const E0_SLICE: usize = 1024;

/// 3E ROM segment size: 2 KB (the switchable half-window).
const THREE_E_SEG: usize = 2048;
/// 3E RAM bank size: 1 KB.
const THREE_E_RAM_BANK: usize = 1024;
/// 3E RAM bank count (32 × 1 KB = 32 KB).
const THREE_E_RAM_BANKS: usize = 32;

/// E7 segment size: 2 KB. Segment 0 is `$1000-$17FF`, segment 1 `$1800-$1FFF`.
const E7_SEG: usize = 2048;
/// E7 segment-0 RAM bank size: 1 KB (write port low half, read port high half).
const E7_WINDOW_RAM: usize = 1024;
/// E7 fixed 256-byte RAM bank size (four banks at `$1800-$19FF`).
const E7_STRIP_RAM: usize = 256;

/// DPC: the 2KB graphics ROM follows the 8KB program in the image.
const DPC_DISPLAY_OFFSET: usize = 8192;
/// DPC graphics ROM size (2KB), also the data-fetcher counter span.
const DPC_DISPLAY_SIZE: usize = 2048;
/// DPC music oscillator frequency (Hz). Stella's default DPC "pitch"; the music
/// fetchers advance at this rate relative to the CPU clock.
const DPC_PITCH: f64 = 20_000.0;

impl Cartridge {
    /// Parse a ROM and detect the banking scheme from its size.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        // 3E and 3F are size-agnostic — detected by signature ahead of the
        // size-keyed schemes, and use 2 KB ROM segments. Priority is E0 → 3E →
        // 3F (3E's signature is a superset of 3F's), matching Stella; the E0
        // gate keeps an 8 KB E0 cart out of the 3E/3F pre-checks.
        // DPC (Pitfall II) is ~10 KB (8 KB program + 2 KB graphics, optional
        // padding); it's a distinct size with no other scheme, so detect it by
        // size ahead of the 2 KB-multiple 3E/3F pre-check.
        let is_dpc = (10240..=10496).contains(&data.len());
        let is_e0 = !is_dpc && data.len() == 8192 && is_probably_e0(data);
        let bankable_2k =
            !is_dpc && !is_e0 && data.len() >= 8192 && data.len().is_multiple_of(THREE_E_SEG);
        let is_3e = bankable_2k && is_probably_3e(data);
        let is_3f = bankable_2k && !is_3e && is_probably_3f(data);
        let (scheme, bank_size) = if is_dpc {
            (BankingScheme::Dpc, 4096)
        } else if is_3e {
            (BankingScheme::ThreeE, THREE_E_SEG)
        } else if is_3f {
            (BankingScheme::ThreeF, THREE_E_SEG)
        } else {
            match data.len() {
                0..=2048 => (BankingScheme::None, data.len()),
                2049..=4096 => (BankingScheme::None, data.len()),
                // 8 KB is ambiguous: plain F8 or Parker Brothers E0. Distinguish by
                // scanning for an E0 hotspot-access signature (Stella's heuristic),
                // since both are the same length (#412).
                8192 if is_probably_e0(data) => (BankingScheme::E0, 4096),
                // UA also shares the 8 KB size; detect it by its hotspot-access
                // signature, ahead of the plain-F8 fallback.
                8192 if is_probably_ua(data) => (BankingScheme::Ua, 4096),
                // Activision FE, gated off any F8-style signature (matching
                // Stella's `isProbablyFE(image) && !f8`).
                8192 if is_probably_fe(data) && !is_probably_f8(data) => (BankingScheme::Fe, 4096),
                8192 if is_probably_0840(data) => (BankingScheme::EconoBank, 4096),
                8192 => (BankingScheme::F8, 4096),
                // 12 KB is unique to CBS RAM+ (FA) — three 4 KB banks + 256 B RAM.
                12288 => (BankingScheme::Fa, 4096),
                // 16 KB is shared with M-Network E7 (2 KB segments + RAM);
                // detect it by signature ahead of the plain-F6 fallback.
                16384 if is_probably_e7(data) => (BankingScheme::E7, E7_SEG),
                16384 => (BankingScheme::F6, 4096),
                32768 => (BankingScheme::F4, 4096),
                // 64 KB is EF (sixteen 4 KB banks); the EFSC variant's Superchip
                // RAM is added by the overlay detection below.
                65536 => (BankingScheme::Ef, 4096),
                other => return Err(format!("Unsupported ROM size: {other} bytes")),
            }
        };
        let num_banks = data.len().checked_div(bank_size).unwrap_or(1);
        // Power-on bank, per Stella's per-scheme `getStartBank`. Most multi-bank
        // schemes (F8/F6/F4/FA) boot from the last bank, but EF explicitly
        // resets to bank 1 — its reset vector isn't replicated across all 16
        // banks, so the last-bank default would misboot a real EF cart.
        let bank = match scheme {
            BankingScheme::Ef => 1,
            // 3E uses the dedicated three_e_* state, not `bank`; UA/0840/E7
            // power on at bank 0 (E7 = ROM bank 0 in its $1000 window).
            BankingScheme::Ua
            | BankingScheme::EconoBank
            | BankingScheme::ThreeE
            | BankingScheme::ThreeF
            | BankingScheme::E7
            | BankingScheme::Fe => 0,
            _ => num_banks.saturating_sub(1),
        };
        // Superchip (SARA) is a 128-byte RAM overlay on the 4 KB-bank schemes,
        // detected by the repeated-first-128-bytes padding in each bank.
        let superchip = matches!(
            scheme,
            BankingScheme::F8 | BankingScheme::F6 | BankingScheme::F4 | BankingScheme::Ef
        ) && is_probably_sc(data);
        let ram = match scheme {
            BankingScheme::Fa => vec![0u8; 256],
            BankingScheme::ThreeE => vec![0u8; THREE_E_RAM_BANKS * THREE_E_RAM_BANK],
            // E7: 1 KB window RAM + 4 × 256 B strip = 2 KB.
            BankingScheme::E7 => vec![0u8; E7_WINDOW_RAM + 4 * E7_STRIP_RAM],
            _ if superchip => vec![0u8; 128],
            _ => Vec::new(),
        };
        Ok(Self {
            rom: data.to_vec(),
            scheme,
            bank,
            bank_size,
            // E0 power-on slice mapping (Stella's default: 4/5/6); the cart's
            // own startup code reprograms the slices before drawing.
            e0_segments: [4, 5, 6],
            ram,
            // 3E powers on with ROM segment 0 in the window (the reset vector
            // lives in the fixed last segment, so this is just the default).
            three_e_ram_active: false,
            three_e_rom_seg: 0,
            three_e_ram_bank: 0,
            e7_ram_bank: 0,
            superchip,
            fe_armed: false,
            dpc_tops: [0; 8],
            dpc_bottoms: [0; 8],
            dpc_counters: [0; 8],
            dpc_flags: [0; 8],
            dpc_music_mode: [false; 3],
            dpc_rng: 1,
            dpc_cycle: 0,
            dpc_audio_cycle: 0,
            dpc_fractional: 0.0,
            // NTSC CPU clock by default; the machine overrides per region.
            dpc_clock_rate: 1_193_182.0,
        })
    }

    /// Read a byte from the cart at `$1000-$1FFF` (also fires hotspot
    /// detection for bank switching).
    pub fn read(&mut self, addr: u16) -> u8 {
        // DPC register reads have side effects (clock the RNG, advance fetcher
        // counters), so they take a dedicated mutating path.
        if self.scheme == BankingScheme::Dpc {
            return self.dpc_read(addr);
        }
        self.check_hotspot(addr);
        self.byte_at(addr)
    }

    /// DPC read: registers at `$1000-$103F` stream the graphics ROM through the
    /// data fetchers / RNG (with side effects); `$1040-$1FFF` is program ROM.
    fn dpc_read(&mut self, addr: u16) -> u8 {
        self.check_hotspot(addr); // F8-style $1FF8/$1FF9 program banking
        self.dpc_clock_rng();
        let address = (addr & 0x0FFF) as usize;
        if address >= 0x40 {
            return self
                .rom
                .get(self.bank * 4096 + address)
                .copied()
                .unwrap_or(0);
        }
        let index = address & 0x07;
        let function = (address >> 3) & 0x07;
        // Refresh the fetcher's flag from its top/bottom comparators.
        let low = (self.dpc_counters[index] & 0x00FF) as u8;
        if low == self.dpc_tops[index] {
            self.dpc_flags[index] = 0xFF;
        } else if low == self.dpc_bottoms[index] {
            self.dpc_flags[index] = 0x00;
        }
        let result = match function {
            0x00 if index < 4 => self.dpc_rng,
            0x00 => {
                // Music amplitude from fetchers 5-7, after advancing the
                // oscillator by the cycles elapsed since the last music read.
                self.dpc_update_music();
                const AMPLITUDES: [u8; 8] = [0x00, 0x04, 0x05, 0x09, 0x06, 0x0A, 0x0B, 0x0F];
                let mut i = 0usize;
                if self.dpc_music_mode[0] && self.dpc_flags[5] != 0 {
                    i |= 0x01;
                }
                if self.dpc_music_mode[1] && self.dpc_flags[6] != 0 {
                    i |= 0x02;
                }
                if self.dpc_music_mode[2] && self.dpc_flags[7] != 0 {
                    i |= 0x04;
                }
                AMPLITUDES[i]
            }
            0x01 => self.dpc_display(index),
            0x02 => self.dpc_display(index) & self.dpc_flags[index],
            0x07 => self.dpc_flags[index],
            _ => 0,
        };
        // Advance the counter unless this is a music-mode fetcher (5-7).
        if index < 5 || !self.dpc_music_mode[index - 5] {
            self.dpc_counters[index] = self.dpc_counters[index].wrapping_sub(1) & 0x07FF;
        }
        result
    }

    /// The graphics-ROM byte the data fetcher at `index` currently points to.
    /// The 11-bit counter indexes the 2KB display ROM in reverse.
    fn dpc_display(&self, index: usize) -> u8 {
        let counter = (self.dpc_counters[index] & 0x07FF) as usize;
        self.rom
            .get(DPC_DISPLAY_OFFSET + (DPC_DISPLAY_SIZE - 1 - counter))
            .copied()
            .unwrap_or(0)
    }

    /// Clock the DPC's LFSR one step (the input bit is the NOT-XOR of bits
    /// 7/5/4/3, via a lookup table; matches Stella).
    fn dpc_clock_rng(&mut self) {
        const F: [u8; 16] = [1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1];
        let idx = usize::from((self.dpc_rng >> 3) & 0x07)
            | if self.dpc_rng & 0x80 != 0 { 0x08 } else { 0x00 };
        self.dpc_rng = (self.dpc_rng << 1) | F[idx];
    }

    /// The cart byte mapped at `addr`, with no bank-switch side effect.
    fn byte_at(&self, addr: u16) -> u8 {
        let offset = (addr & 0x0FFF) as usize;
        // Superchip read port ($1080-$10FF) overlays every bank; reads of the
        // write port ($1000-$107F) fall through to the (padding) ROM below.
        if self.superchip && (0x80..0x100).contains(&offset) {
            return self.ram.get(offset - 0x80).copied().unwrap_or(0);
        }
        // DPC: side-effect-free peek (debugger view). Registers ($1000-$103F)
        // can't be read without mutating, so report 0; the rest is program ROM.
        if self.scheme == BankingScheme::Dpc {
            if offset < 0x40 {
                return 0;
            }
            return self
                .rom
                .get(self.bank * 4096 + offset)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::E0 {
            // Four 1KB slices: 0/1/2 follow their segment banks, slice 3 ($1C00-
            // $1FFF) is fixed to bank 7.
            let (seg_bank, slice_off) = match offset {
                0x000..=0x3FF => (self.e0_segments[0], offset),
                0x400..=0x7FF => (self.e0_segments[1], offset - 0x400),
                0x800..=0xBFF => (self.e0_segments[2], offset - 0x800),
                _ => (7, offset - 0xC00),
            };
            return self
                .rom
                .get(seg_bank * E0_SLICE + slice_off)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::Fa {
            // The RAM read port ($1100-$11FF) overlays the bank window; the
            // write port ($1000-$10FF) reads back ROM (undefined on hardware).
            if (0x100..0x200).contains(&offset) {
                return self.ram.get(offset - 0x100).copied().unwrap_or(0);
            }
            return self
                .rom
                .get(self.bank * self.bank_size + offset)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::ThreeE {
            let seg_count = self.rom.len() / THREE_E_SEG;
            // $1800-$1FFF is fixed to the last 2 KB ROM segment.
            if offset >= THREE_E_SEG {
                let last = seg_count.saturating_sub(1);
                return self
                    .rom
                    .get(last * THREE_E_SEG + (offset - THREE_E_SEG))
                    .copied()
                    .unwrap_or(0);
            }
            // $1000-$17FF: either a RAM bank or a ROM segment. A RAM bank is
            // 1 KB read through the low half; the high half mirrors it (its
            // write port is handled in `write`).
            if self.three_e_ram_active {
                let cell =
                    self.three_e_ram_bank * THREE_E_RAM_BANK + (offset & (THREE_E_RAM_BANK - 1));
                return self.ram.get(cell).copied().unwrap_or(0);
            }
            return self
                .rom
                .get(self.three_e_rom_seg * THREE_E_SEG + offset)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::ThreeF {
            // ROM-only: window ($1000-$17FF) = selected segment, $1800-$1FFF
            // fixed to the last 2 KB segment.
            if offset >= THREE_E_SEG {
                let last = (self.rom.len() / THREE_E_SEG).saturating_sub(1);
                return self
                    .rom
                    .get(last * THREE_E_SEG + (offset - THREE_E_SEG))
                    .copied()
                    .unwrap_or(0);
            }
            return self
                .rom
                .get(self.bank * THREE_E_SEG + offset)
                .copied()
                .unwrap_or(0);
        }
        if self.scheme == BankingScheme::E7 {
            let ram_bank_id = self.rom.len() / E7_SEG - 1; // 7 for 16 KB
            // Segment 0 ($1000-$17FF): a ROM bank, or the 1 KB RAM when the
            // window bank is the RAM id. Read and write ports both alias the
            // same 1 KB (read port $1400-$17FF, write port $1000-$13FF).
            if offset < E7_SEG {
                if self.bank == ram_bank_id {
                    return self
                        .ram
                        .get(offset & (E7_WINDOW_RAM - 1))
                        .copied()
                        .unwrap_or(0);
                }
                return self
                    .rom
                    .get(self.bank * E7_SEG + offset)
                    .copied()
                    .unwrap_or(0);
            }
            // The 256-byte RAM strip at $1800-$19FF (write $1800-$18FF, read
            // $1900-$19FF — both alias the selected 256 B bank).
            if (0x800..0xA00).contains(&offset) {
                let cell = E7_WINDOW_RAM + self.e7_ram_bank * E7_STRIP_RAM + (offset & 0xFF);
                return self.ram.get(cell).copied().unwrap_or(0);
            }
            // $1A00-$1FFF: fixed to the last ROM bank.
            return self
                .rom
                .get(ram_bank_id * E7_SEG + (offset & (E7_SEG - 1)))
                .copied()
                .unwrap_or(0);
        }
        if self.bank_size <= 2048 {
            self.rom[offset % self.rom.len()]
        } else {
            let idx = self.bank * self.bank_size + offset;
            self.rom.get(idx).copied().unwrap_or(0)
        }
    }

    /// Write to cart space — fires hotspot detection and, on FA, stores to the
    /// on-cart RAM through its write port (`$1000-$10FF`).
    pub fn write(&mut self, addr: u16, value: u8) {
        self.check_hotspot(addr);
        if self.superchip {
            // Superchip write port: $1000-$107F.
            let offset = (addr & 0x0FFF) as usize;
            if offset < 0x80
                && let Some(c) = self.ram.get_mut(offset)
            {
                *c = value;
            }
        }
        if self.scheme == BankingScheme::Fa {
            let offset = (addr & 0x0FFF) as usize;
            if offset < 0x100
                && let Some(cell) = self.ram.get_mut(offset)
            {
                *cell = value;
            }
        }
        if self.scheme == BankingScheme::ThreeE && self.three_e_ram_active {
            // The RAM write port is the high 1 KB of the window ($1400-$17FF).
            let offset = (addr & 0x0FFF) as usize;
            if (THREE_E_RAM_BANK..THREE_E_SEG).contains(&offset) {
                let cell = self.three_e_ram_bank * THREE_E_RAM_BANK + (offset - THREE_E_RAM_BANK);
                if let Some(c) = self.ram.get_mut(cell) {
                    *c = value;
                }
            }
        }
        if self.scheme == BankingScheme::E7 {
            let offset = (addr & 0x0FFF) as usize;
            let ram_bank_id = self.rom.len() / E7_SEG - 1;
            if self.bank == ram_bank_id && offset < E7_WINDOW_RAM {
                // 1 KB window RAM write port: $1000-$13FF.
                if let Some(c) = self.ram.get_mut(offset) {
                    *c = value;
                }
            } else if (0x800..0x900).contains(&offset) {
                // 256-byte strip write port: $1800-$18FF.
                let cell = E7_WINDOW_RAM + self.e7_ram_bank * E7_STRIP_RAM + (offset & 0xFF);
                if let Some(c) = self.ram.get_mut(cell) {
                    *c = value;
                }
            }
        }
        // DPC: program the data fetchers via the write registers ($1040-$107F).
        if self.scheme == BankingScheme::Dpc {
            self.dpc_clock_rng();
            let address = (addr & 0x0FFF) as usize;
            if (0x40..0x80).contains(&address) {
                let index = address & 0x07;
                match (address >> 3) & 0x07 {
                    // DFx top count (also clears the flag).
                    0x00 => {
                        self.dpc_tops[index] = value;
                        self.dpc_flags[index] = 0x00;
                    }
                    // DFx bottom count.
                    0x01 => self.dpc_bottoms[index] = value,
                    // DFx counter low — a music-mode fetcher reloads from `top`.
                    0x02 => {
                        let lo = if index >= 5 && self.dpc_music_mode[index - 5] {
                            u16::from(self.dpc_tops[index])
                        } else {
                            u16::from(value)
                        };
                        self.dpc_counters[index] = (self.dpc_counters[index] & 0x0700) | lo;
                    }
                    // DFx counter high (+ music-mode enable for fetchers 5-7).
                    0x03 => {
                        self.dpc_counters[index] =
                            ((u16::from(value) & 0x07) << 8) | (self.dpc_counters[index] & 0x00FF);
                        if index >= 5 {
                            self.dpc_music_mode[index - 5] = value & 0x10 != 0;
                        }
                    }
                    // RNG reset.
                    0x06 => self.dpc_rng = 1,
                    _ => {}
                }
            }
        }
    }

    /// Snoop any bus access for schemes whose bank-select hotspots fall
    /// *outside* the `$1000-$1FFF` cart window. UA watches low TIA-mirror
    /// addresses (incomplete address decoding lets the cart see them), so the
    /// machine forwards every access here. Window-hotspot schemes ignore it,
    /// and their own switching stays in [`Self::read`]/[`Self::write`].
    pub fn snoop(&mut self, addr: u16) {
        match self.scheme {
            // UA: `$0220` → bank 0, `$0240` → bank 1. The mask folds the
            // address mirrors the real titles use (e.g. `$02C0`) onto these.
            BankingScheme::Ua => match addr & 0x1260 {
                0x0220 => self.bank = 0,
                0x0240 => self.bank = 1,
                _ => {}
            },
            // 0840 EconoBank: `$0800` → bank 0, `$0840` → bank 1.
            BankingScheme::EconoBank => match addr & 0x1840 {
                0x0800 => self.bank = 0,
                0x0840 => self.bank = 1,
                _ => {}
            },
            _ => {}
        }
    }

    /// Snoop a bus *write*. Covers the access-triggered schemes (UA/0840 also
    /// switch on writes to their hotspots) plus 3E, whose bank-select uses the
    /// written *value*: `STA $3F` maps ROM segment `value` into the window,
    /// `STA $3E` maps RAM bank `value`. Hotspots are matched in the cart window
    /// mask (`$3E`/`$3F`), per Stella's `Cartridge3E`.
    pub fn snoop_write(&mut self, addr: u16, value: u8) {
        self.snoop(addr);
        if self.scheme == BankingScheme::ThreeE {
            match addr & 0x0FFF {
                0x3F => {
                    let seg_count = (self.rom.len() / THREE_E_SEG).max(1);
                    self.three_e_ram_active = false;
                    self.three_e_rom_seg = usize::from(value) % seg_count;
                }
                0x3E => {
                    self.three_e_ram_active = true;
                    self.three_e_ram_bank = usize::from(value) % THREE_E_RAM_BANKS;
                }
                _ => {}
            }
        }
        // 3F (Tigervision): any write to $00-$3F stores the window ROM segment.
        if self.scheme == BankingScheme::ThreeF && addr <= 0x003F {
            let seg_count = (self.rom.len() / THREE_E_SEG).max(1);
            self.bank = usize::from(value) % seg_count;
        }
    }

    /// Observe a bus access (post-value) for the Activision FE scheme: an
    /// access to `$01FE` arms a probe, and the next access's bus `value`
    /// selects the bank (`(value >> 5) ^ 0b111`). Called for every CPU access,
    /// read or write; no-op for other schemes.
    pub fn snoop_fe(&mut self, addr: u16, value: u8) {
        if self.scheme != BankingScheme::Fe {
            return;
        }
        if self.fe_armed {
            let banks = (self.rom.len() / self.bank_size).max(1);
            self.bank = usize::from((value >> 5) ^ 0b111) % banks;
            self.fe_armed = false;
        } else {
            self.fe_armed = addr == 0x01FE;
        }
    }

    /// Advance the cart's CPU-cycle clock by one (driven by the machine each
    /// CPU cycle). Only the DPC music oscillator uses it.
    pub fn tick(&mut self) {
        if self.scheme == BankingScheme::Dpc {
            self.dpc_cycle = self.dpc_cycle.wrapping_add(1);
        }
    }

    /// Set the CPU clock rate (Hz) the DPC music oscillator times against.
    /// Region-dependent; the machine sets it after construction.
    pub fn set_dpc_clock_rate(&mut self, hz: f64) {
        self.dpc_clock_rate = hz;
    }

    /// Advance the DPC music-mode fetchers (5-7) by the OSC clocks elapsed
    /// since the last update, deriving the count from the CPU cycles run and
    /// the pitch/clock ratio. Called on each music-amplitude read.
    fn dpc_update_music(&mut self) {
        let cycles = self.dpc_cycle.wrapping_sub(self.dpc_audio_cycle);
        self.dpc_audio_cycle = self.dpc_cycle;
        let clocks = (DPC_PITCH * cycles as f64) / self.dpc_clock_rate + self.dpc_fractional;
        let whole = clocks.floor();
        self.dpc_fractional = clocks - whole;
        let whole = whole as u32;
        if whole == 0 {
            return;
        }
        for x in 5..8 {
            if !self.dpc_music_mode[x - 5] {
                continue;
            }
            let top = u32::from(self.dpc_tops[x]) + 1;
            let mut new_low = i32::from((self.dpc_counters[x] & 0x00FF) as u8);
            if self.dpc_tops[x] != 0 {
                new_low -= (whole % top) as i32;
                if new_low < 0 {
                    new_low += top as i32;
                }
            } else {
                new_low = 0;
            }
            if new_low <= i32::from(self.dpc_bottoms[x]) {
                self.dpc_flags[x] = 0x00;
            } else if new_low <= i32::from(self.dpc_tops[x]) {
                self.dpc_flags[x] = 0xFF;
            }
            self.dpc_counters[x] = (self.dpc_counters[x] & 0x0700) | (new_low as u16 & 0x00FF);
        }
    }

    /// Current bank.
    #[must_use]
    pub fn bank(&self) -> usize {
        self.bank
    }

    /// Banking scheme.
    #[must_use]
    pub fn scheme(&self) -> BankingScheme {
        self.scheme
    }

    /// Whether a Superchip (SARA) 128-byte RAM overlay is present.
    #[must_use]
    pub fn has_superchip(&self) -> bool {
        self.superchip
    }

    fn check_hotspot(&mut self, addr: u16) {
        match self.scheme {
            BankingScheme::None => {}
            BankingScheme::F8 => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                _ => {}
            },
            BankingScheme::F6 => match addr {
                0x1FF6 => self.bank = 0,
                0x1FF7 => self.bank = 1,
                0x1FF8 => self.bank = 2,
                0x1FF9 => self.bank = 3,
                _ => {}
            },
            BankingScheme::F4 => match addr {
                0x1FF4 => self.bank = 0,
                0x1FF5 => self.bank = 1,
                0x1FF6 => self.bank = 2,
                0x1FF7 => self.bank = 3,
                0x1FF8 => self.bank = 4,
                0x1FF9 => self.bank = 5,
                0x1FFA => self.bank = 6,
                0x1FFB => self.bank = 7,
                _ => {}
            },
            // E0: each switchable slice picks one of the eight 1KB banks from
            // the low 3 bits of the hotspot address.
            BankingScheme::E0 => match addr {
                0x1FE0..=0x1FE7 => self.e0_segments[0] = usize::from(addr & 0x07),
                0x1FE8..=0x1FEF => self.e0_segments[1] = usize::from(addr & 0x07),
                0x1FF0..=0x1FF7 => self.e0_segments[2] = usize::from(addr & 0x07),
                _ => {}
            },
            BankingScheme::Fa => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                0x1FFA => self.bank = 2,
                _ => {}
            },
            // EF: sixteen banks across the $1FE0-$1FEF hotspot window.
            BankingScheme::Ef => {
                if (0x1FE0..=0x1FEF).contains(&addr) {
                    self.bank = usize::from(addr - 0x1FE0);
                }
            }
            // UA / 0840 switch on out-of-window addresses, handled in `snoop`;
            // 3E/3F switch on writes to TIA-space addresses, in `snoop_write`.
            BankingScheme::Ua
            | BankingScheme::EconoBank
            | BankingScheme::ThreeE
            | BankingScheme::ThreeF => {}
            // FE switches via the stack-snoop probe in `snoop_fe`.
            BankingScheme::Fe => {}
            // E7 (16K): $1FE0-$1FE7 select the $1000 window bank (7 = RAM),
            // $1FE8-$1FEB select the 256-byte RAM strip bank.
            BankingScheme::E7 => match addr {
                0x1FE0..=0x1FE7 => self.bank = usize::from(addr & 0x0007),
                0x1FE8..=0x1FEB => self.e7_ram_bank = usize::from(addr & 0x0003),
                _ => {}
            },
            // DPC banks its 8 KB program F8-style ($1FF8/$1FF9).
            BankingScheme::Dpc => match addr {
                0x1FF8 => self.bank = 0,
                0x1FF9 => self.bank = 1,
                _ => {}
            },
        }
    }
}

impl Cartridge {
    /// Read ROM at the current bank/slice mapping with no bank-switch side
    /// effect (the debugger's view; `read` checks hotspots and may switch).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.byte_at(addr)
    }
}

/// Whether an 8 KB image is a Parker Brothers E0 cart rather than plain F8.
///
/// Both are 8 KB, so size alone can't tell them apart. E0 carts switch banks by
/// accessing `$1FE0-$1FF9` with absolute addressing; scan for the known
/// instruction signatures (ported from Stella's `isProbablyE0`, attributed to
/// MESS) that catch the real E0 titles without false-positiving on F8.
fn is_probably_e0(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 8] = [
        [0x8D, 0xE0, 0x1F], // STA $1FE0
        [0x8D, 0xE0, 0x5F], // STA $5FE0
        [0x8D, 0xE9, 0xFF], // STA $FFE9
        [0x0C, 0xE0, 0x1F], // NOP $1FE0
        [0xAD, 0xE0, 0x1F], // LDA $1FE0
        [0xAD, 0xE9, 0xFF], // LDA $FFE9
        [0xAD, 0xED, 0xFF], // LDA $FFED
        [0xAD, 0xF3, 0xBF], // LDA $BFF3
    ];
    SIGNATURES
        .iter()
        .any(|sig| rom.windows(sig.len()).any(|w| w == sig))
}

/// How many times `sig` occurs in `rom`.
fn count_bytes(rom: &[u8], sig: &[u8]) -> usize {
    rom.windows(sig.len()).filter(|w| *w == sig).count()
}

/// Whether an 8 KB image is a UA Limited cart. Like E0, it shares 8 KB with
/// plain F8, so detection scans for the instruction signatures that access the
/// `$0220`/`$0240` (and mirror) bankswitch hotspots — ported from Stella's
/// `isProbablyUA`.
fn is_probably_ua(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 7] = [
        [0x8D, 0x40, 0x02], // STA $240 (Funky Fish, Pleiades)
        [0xAD, 0x40, 0x02], // LDA $240
        [0xBD, 0x1F, 0x02], // LDA $21F,X (Gingerbread Man)
        [0x2C, 0xC0, 0x02], // BIT $2C0 (Time Pilot)
        [0x8D, 0xC0, 0x02], // STA $2C0 (Fathom, Vanguard)
        [0xAD, 0xC0, 0x02], // LDA $2C0 (Mickey)
        [0x2C, 0xB0, 0x0F], // BIT $FB0 (Digivision Beamrider)
    ];
    SIGNATURES.iter().any(|sig| count_bytes(rom, sig) >= 1)
}

/// Whether an 8 KB image is a 0840 "EconoBank" cart. It shares 8 KB with F8,
/// so detection scans for the `$0800`/`$0840` hotspot-access signatures —
/// which must appear *at least twice* to avoid false positives (Stella's
/// `isProbably0840`).
fn is_probably_0840(rom: &[u8]) -> bool {
    const SIG3: [[u8; 3]; 3] = [
        [0xAD, 0x00, 0x08], // LDA $0800
        [0xAD, 0x40, 0x08], // LDA $0840
        [0x2C, 0x00, 0x08], // BIT $0800
    ];
    if SIG3.iter().any(|sig| count_bytes(rom, sig) >= 2) {
        return true;
    }
    const SIG4: [[u8; 4]; 2] = [
        [0x0C, 0x00, 0x08, 0x4C], // NOP $0800; JMP
        [0x0C, 0xFF, 0x0F, 0x4C], // NOP $0FFF; JMP
    ];
    SIG4.iter().any(|sig| count_bytes(rom, sig) >= 2)
}

/// Whether an image is a 3E (RAM+ROM) cart. Bank-select is by storing the bank
/// number to `$3E` (RAM) / `$3F` (ROM); we expect `STA $3F` at least twice
/// (there are at least two ROM segments) and at least one `STA $3E`, matching
/// Stella's `isProbably3E`.
fn is_probably_3e(rom: &[u8]) -> bool {
    count_bytes(rom, &[0x85, 0x3E]) >= 1 && count_bytes(rom, &[0x85, 0x3F]) >= 2
}

/// Whether an image is a 3F (Tigervision) cart — ROM-only banking by `STA $3F`,
/// expected at least twice (≥ 2 banks). Per Stella's `isProbably3F`. Check this
/// *after* 3E, whose signature is a superset.
fn is_probably_3f(rom: &[u8]) -> bool {
    count_bytes(rom, &[0x85, 0x3F]) >= 2
}

/// Whether an 8 KB image is an Activision FE cart. FE bankswitching always
/// rides a `JSR $xxxx`; detection scans for the known per-game signatures,
/// ported from Stella's `isProbablyFE`.
fn is_probably_fe(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 5]; 5] = [
        [0x20, 0x00, 0xD0, 0xC6, 0xC5], // JSR $D000; DEC $C5   Decathlon
        [0x20, 0xC3, 0xF8, 0xA5, 0x82], // JSR $F8C3; LDA $82   Robot Tank
        [0xD0, 0xFB, 0x20, 0x73, 0xFE], // BNE -5; JSR $FE73    Space Shuttle
        [0xD0, 0xFB, 0x20, 0x68, 0xFE], // BNE -5; JSR $FE68    Space Shuttle (SECAM)
        [0x20, 0x00, 0xF0, 0x84, 0xD6], // JSR $F000; STY $D6   Thwocker
    ];
    SIGNATURES.iter().any(|sig| count_bytes(rom, sig) >= 1)
}

/// Whether an image carries an F8-style hotspot signature (`STA $1FF9` /
/// `STA $FFF9`). Used to keep an F8 cart out of the FE detection path.
fn is_probably_f8(rom: &[u8]) -> bool {
    count_bytes(rom, &[0x8D, 0xF9, 0x1F]) >= 1 || count_bytes(rom, &[0x8D, 0xF9, 0xFF]) >= 1
}

/// Whether a 4 KB-bank image carries a Superchip (SARA) overlay. The 128-byte
/// RAM occupies the first 256 bytes of each 4 KB bank, so authoring tools leave
/// that area as the first 128 bytes repeated into the second 128 — Stella keys
/// detection off exactly that (`isProbablySC`).
fn is_probably_sc(rom: &[u8]) -> bool {
    !rom.is_empty()
        && rom.len().is_multiple_of(4096)
        && rom
            .chunks_exact(4096)
            .all(|bank| bank[0..128] == bank[128..256])
}

/// Whether a 16 KB image is an M-Network E7 cart. Like the other ambiguous
/// sizes, 16 KB is shared (with F6), so detection scans for the `$1FE0-$1FE7`
/// hotspot-access signatures — ported from Stella's `isProbablyE7`.
fn is_probably_e7(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 7] = [
        [0xAD, 0xE2, 0xFF], // LDA $FFE2
        [0xAD, 0xE5, 0xFF], // LDA $FFE5
        [0xAD, 0xE5, 0x1F], // LDA $1FE5
        [0xAD, 0xE7, 0x1F], // LDA $1FE7
        [0x0C, 0xE7, 0x1F], // NOP $1FE7
        [0x8D, 0xE7, 0xFF], // STA $FFE7
        [0x8D, 0xE7, 0x1F], // STA $1FE7
    ];
    SIGNATURES.iter().any(|sig| count_bytes(rom, sig) >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an 8 KB image whose eight 1 KB banks are filled with the bank
    /// index, optionally carrying an E0 signature so detection fires.
    fn banked_8k(with_e0_sig: bool) -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        for bank in 0..8 {
            rom[bank * 1024..(bank + 1) * 1024].fill(bank as u8);
        }
        if with_e0_sig {
            // STA $FFE9 somewhere in the image (one of Stella's E0 signatures).
            rom[0x10..0x13].copy_from_slice(&[0x8D, 0xE9, 0xFF]);
        }
        rom
    }

    #[test]
    fn detect_e0_vs_f8_for_8k() {
        // Plain 8K with no E0 signature → F8.
        assert_eq!(
            Cartridge::from_rom(&banked_8k(false)).expect("8K").scheme(),
            BankingScheme::F8
        );
        // 8K carrying an E0 access signature → E0.
        assert_eq!(
            Cartridge::from_rom(&banked_8k(true)).expect("8K").scheme(),
            BankingScheme::E0
        );
    }

    #[test]
    fn e0_slices_switch_independently_and_slice3_is_fixed() {
        let mut cart = Cartridge::from_rom(&banked_8k(true)).expect("E0");

        // Slice 3 ($1C00-$1FFF) is always bank 7.
        assert_eq!(cart.read(0x1C00), 7);

        // Each switchable slice selects its bank via its hotspot group.
        cart.read(0x1FE3); // slice 0 → bank 3
        cart.read(0x1FEA); // slice 1 → bank 2 ($1FEA & 7)
        cart.read(0x1FF5); // slice 2 → bank 5
        assert_eq!(cart.read(0x1000), 3, "slice 0 → bank 3");
        assert_eq!(cart.read(0x1400), 2, "slice 1 → bank 2");
        assert_eq!(cart.read(0x1800), 5, "slice 2 → bank 5");
        assert_eq!(cart.read(0x1C00), 7, "slice 3 stays bank 7");

        // Re-pointing one slice doesn't disturb the others.
        cart.read(0x1FE0); // slice 0 → bank 0
        assert_eq!(cart.read(0x1000), 0, "slice 0 → bank 0");
        assert_eq!(cart.read(0x1400), 2, "slice 1 unchanged");
    }

    /// Build a 12 KB CBS RAM+ image whose three 4 KB banks are each filled
    /// with the bank index.
    fn banked_12k() -> Vec<u8> {
        let mut rom = vec![0u8; 12288];
        for bank in 0..3 {
            rom[bank * 4096..(bank + 1) * 4096].fill(bank as u8);
        }
        rom
    }

    #[test]
    fn detect_fa_rom() {
        let cart = Cartridge::from_rom(&banked_12k()).expect("12K");
        assert_eq!(cart.scheme(), BankingScheme::Fa);
        assert_eq!(cart.bank(), 2, "power-on bank is the last (2)");
    }

    #[test]
    fn fa_banks_switch_on_their_hotspots() {
        let mut cart = Cartridge::from_rom(&banked_12k()).expect("FA");

        // A plain ROM read (not a hotspot, not the RAM window) reflects the
        // current bank's fill byte.
        assert_eq!(cart.read(0x1F00), 2, "starts in bank 2");
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1F00), 0, "$1FF8 → bank 0");
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1F00), 1, "$1FF9 → bank 1");
        cart.read(0x1FFA);
        assert_eq!(cart.read(0x1F00), 2, "$1FFA → bank 2");
    }

    #[test]
    fn fa_ram_round_trips_through_its_ports() {
        let mut cart = Cartridge::from_rom(&banked_12k()).expect("FA");

        // Write port $1000-$10FF in, read port $1100-$11FF out (same offset).
        cart.write(0x1005, 0xAB);
        cart.write(0x10FF, 0x42);
        assert_eq!(cart.read(0x1105), 0xAB, "RAM offset 5 reads back");
        assert_eq!(cart.read(0x11FF), 0x42, "RAM offset 255 reads back");

        // RAM survives a bank switch (it's separate from the ROM banks).
        cart.read(0x1FF8); // → bank 0
        assert_eq!(cart.read(0x1105), 0xAB, "RAM persists across banking");
    }

    /// Build a 64 KB EF image whose sixteen 4 KB banks are each filled with
    /// the bank index.
    fn banked_64k() -> Vec<u8> {
        let mut rom = vec![0u8; 65536];
        for bank in 0..16 {
            rom[bank * 4096..(bank + 1) * 4096].fill(bank as u8);
        }
        rom
    }

    #[test]
    fn detect_ef_rom() {
        let cart = Cartridge::from_rom(&banked_64k()).expect("64K");
        assert_eq!(cart.scheme(), BankingScheme::Ef);
        assert_eq!(cart.bank(), 1, "EF resets to bank 1 (Stella getStartBank)");
    }

    #[test]
    fn ef_banks_switch_across_the_full_hotspot_window() {
        let mut cart = Cartridge::from_rom(&banked_64k()).expect("EF");
        assert_eq!(cart.read(0x1F00), 1, "EF resets to bank 1");
        // Every hotspot $1FE0-$1FEF selects its bank 0-15.
        for bank in 0..16u16 {
            cart.read(0x1FE0 + bank);
            assert_eq!(
                cart.read(0x1F00),
                bank as u8,
                "$1F{:02X} → bank {bank}",
                0xE0 + bank
            );
        }
        // An address just outside the window leaves the bank alone.
        cart.read(0x1FE5);
        cart.read(0x1FDF);
        assert_eq!(cart.read(0x1F00), 5, "$1FDF is not a hotspot");
    }

    /// Build an 8 KB UA image: bank 0 filled `0xA0`, bank 1 `0xA1`, carrying a
    /// `STA $240` UA hotspot signature.
    fn ua_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xA0);
        rom[4096..8192].fill(0xA1);
        rom[0x20..0x23].copy_from_slice(&[0x8D, 0x40, 0x02]); // STA $240
        rom
    }

    #[test]
    fn detect_ua_vs_f8() {
        // Plain 8K (no UA/E0 signature) stays F8.
        assert_eq!(
            Cartridge::from_rom(&vec![0xEA; 8192]).expect("F8").scheme(),
            BankingScheme::F8
        );
        // 8K with a UA hotspot signature → UA, power-on bank 0.
        let cart = Cartridge::from_rom(&ua_rom()).expect("UA");
        assert_eq!(cart.scheme(), BankingScheme::Ua);
        assert_eq!(cart.bank(), 0, "UA resets to bank 0 (Stella default)");
    }

    #[test]
    fn ua_snoops_its_out_of_window_hotspots() {
        let mut cart = Cartridge::from_rom(&ua_rom()).expect("UA");
        assert_eq!(cart.read(0x1F00), 0xA0, "starts in bank 0");

        cart.snoop(0x0240); // → bank 1
        assert_eq!(cart.read(0x1F00), 0xA1, "$0240 → bank 1");
        cart.snoop(0x0220); // → bank 0
        assert_eq!(cart.read(0x1F00), 0xA0, "$0220 → bank 0");

        // The address mirror real titles use ($02C0) folds onto the bank-1 case.
        cart.snoop(0x02C0);
        assert_eq!(cart.read(0x1F00), 0xA1, "$02C0 mirror → bank 1");

        // An unrelated access leaves the bank alone.
        cart.snoop(0x1F00);
        assert_eq!(cart.read(0x1F00), 0xA1, "non-hotspot access is inert");
    }

    /// Build an 8 KB 0840 image: bank 0 `0xB0`, bank 1 `0xB1`, carrying two
    /// `LDA $0800` hotspot signatures (the scheme needs the signature twice).
    fn econobank_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xB0);
        rom[4096..8192].fill(0xB1);
        rom[0x20..0x23].copy_from_slice(&[0xAD, 0x00, 0x08]); // LDA $0800
        rom[0x40..0x43].copy_from_slice(&[0xAD, 0x00, 0x08]); // ...again
        rom
    }

    #[test]
    fn detect_0840_vs_f8() {
        // A single signature copy is not enough — stays F8.
        let mut once = vec![0xEA; 8192];
        once[0x20..0x23].copy_from_slice(&[0xAD, 0x00, 0x08]);
        assert_eq!(
            Cartridge::from_rom(&once).expect("F8").scheme(),
            BankingScheme::F8
        );
        // Two copies → 0840 EconoBank, power-on bank 0.
        let cart = Cartridge::from_rom(&econobank_rom()).expect("0840");
        assert_eq!(cart.scheme(), BankingScheme::EconoBank);
        assert_eq!(cart.bank(), 0, "0840 resets to bank 0");
    }

    #[test]
    fn econobank_snoops_its_out_of_window_hotspots() {
        let mut cart = Cartridge::from_rom(&econobank_rom()).expect("0840");
        assert_eq!(cart.read(0x1F00), 0xB0, "starts in bank 0");
        cart.snoop(0x0840); // → bank 1
        assert_eq!(cart.read(0x1F00), 0xB1, "$0840 → bank 1");
        cart.snoop(0x0800); // → bank 0
        assert_eq!(cart.read(0x1F00), 0xB0, "$0800 → bank 0");
    }

    /// Build a 32 KB 3E image: sixteen 2 KB ROM segments each filled with the
    /// segment index, carrying the `STA $3E` + 2× `STA $3F` signature.
    fn three_e_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 32768];
        for seg in 0..16 {
            rom[seg * 2048..(seg + 1) * 2048].fill(seg as u8);
        }
        // Signature: STA $3E once, STA $3F twice (placed in segment 0).
        rom[0x10..0x12].copy_from_slice(&[0x85, 0x3E]);
        rom[0x12..0x14].copy_from_slice(&[0x85, 0x3F]);
        rom[0x14..0x16].copy_from_slice(&[0x85, 0x3F]);
        rom
    }

    #[test]
    fn detect_3e_by_signature() {
        let cart = Cartridge::from_rom(&three_e_rom()).expect("3E");
        assert_eq!(cart.scheme(), BankingScheme::ThreeE);
        // A plain 32K cart with no 3E signature stays F4.
        assert_eq!(
            Cartridge::from_rom(&vec![0xEA; 32768])
                .expect("F4")
                .scheme(),
            BankingScheme::F4
        );
    }

    #[test]
    fn three_e_rom_segment_window_and_fixed_tail() {
        let mut cart = Cartridge::from_rom(&three_e_rom()).expect("3E");
        // Power-on: segment 0 in the window, last segment (15) fixed at $1800.
        assert_eq!(cart.read(0x1000), 0, "window holds ROM segment 0");
        assert_eq!(cart.read(0x1800), 15, "$1800-$1FFF fixed to last segment");

        // STA $3F with value 5 maps ROM segment 5 into the window.
        cart.snoop_write(0x003F, 5);
        assert_eq!(cart.read(0x1000), 5, "$3F → ROM segment 5 in window");
        assert_eq!(cart.read(0x1800), 15, "fixed tail unchanged");

        // The bank number wraps modulo the segment count (16 segments).
        cart.snoop_write(0x003F, 16 + 3);
        assert_eq!(cart.read(0x1000), 3, "segment select wraps mod 16");
    }

    #[test]
    fn three_e_ram_bank_reads_and_writes() {
        let mut cart = Cartridge::from_rom(&three_e_rom()).expect("3E");
        // STA $3E with value 2 maps RAM bank 2 into the window.
        cart.snoop_write(0x003E, 2);
        // Write port is the high 1 KB ($1400-$17FF), read port the low ($1000-$13FF).
        cart.write(0x1400, 0xC3);
        cart.write(0x17FF, 0x5A);
        assert_eq!(cart.read(0x1000), 0xC3, "RAM read port cell 0");
        assert_eq!(cart.read(0x13FF), 0x5A, "RAM read port cell 1023");

        // A different RAM bank is independent.
        cart.snoop_write(0x003E, 3);
        assert_eq!(cart.read(0x1000), 0, "RAM bank 3 starts clear");
        // Switching back to ROM restores the segment view.
        cart.snoop_write(0x003F, 1);
        assert_eq!(cart.read(0x1000), 1, "back to ROM segment 1");
    }

    /// Build a 16 KB E7 image: eight 2 KB ROM banks each filled with the bank
    /// index, carrying an `STA $1FE7` E7 hotspot signature.
    fn e7_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 16384];
        for b in 0..8 {
            rom[b * 2048..(b + 1) * 2048].fill(b as u8);
        }
        rom[0x30..0x33].copy_from_slice(&[0x8D, 0xE7, 0x1F]); // STA $1FE7
        rom
    }

    #[test]
    fn detect_e7_vs_f6() {
        let cart = Cartridge::from_rom(&e7_rom()).expect("E7");
        assert_eq!(cart.scheme(), BankingScheme::E7);
        // Plain 16K with no E7 signature stays F6.
        assert_eq!(
            Cartridge::from_rom(&vec![0xEA; 16384])
                .expect("F6")
                .scheme(),
            BankingScheme::F6
        );
    }

    #[test]
    fn e7_window_selects_rom_banks_and_fixes_the_tail() {
        let mut cart = Cartridge::from_rom(&e7_rom()).expect("E7");
        // Power-on: ROM bank 0 in the $1000 window; bank 7 fixed at $1A00-$1FFF.
        assert_eq!(cart.read(0x1000), 0, "window starts at ROM bank 0");
        assert_eq!(cart.read(0x1A00), 7, "$1A00 fixed to last ROM bank");

        // $1FE3 selects ROM bank 3 into the window; the tail stays put.
        cart.read(0x1FE3);
        assert_eq!(cart.read(0x1000), 3, "$1FE3 → ROM bank 3 in window");
        assert_eq!(cart.read(0x1A00), 7, "tail unchanged");
    }

    #[test]
    fn e7_window_ram_uses_split_write_read_ports() {
        let mut cart = Cartridge::from_rom(&e7_rom()).expect("E7");
        // $1FE7 maps the 1 KB RAM into the window.
        cart.read(0x1FE7);
        // Write port is the low 1 KB ($1000-$13FF); read port the high ($1400-$17FF).
        cart.write(0x1000, 0x11);
        cart.write(0x13FF, 0x22);
        assert_eq!(cart.read(0x1400), 0x11, "RAM cell 0 via read port $1400");
        assert_eq!(cart.read(0x17FF), 0x22, "RAM cell 1023 via read port $17FF");

        // Switching the window back to a ROM bank hides the RAM.
        cart.read(0x1FE0);
        assert_eq!(cart.read(0x1000), 0, "window back to ROM bank 0");
    }

    #[test]
    fn e7_strip_ram_banks_switch_independently() {
        let mut cart = Cartridge::from_rom(&e7_rom()).expect("E7");
        // 256-byte strip at $1800-$19FF: write $1800-$18FF, read $1900-$19FF.
        cart.read(0x1FE8); // strip bank 0
        cart.write(0x1800, 0xAB);
        assert_eq!(cart.read(0x1900), 0xAB, "strip bank 0 round-trips");

        // A different strip bank is independent and survives the switch back.
        cart.read(0x1FE9); // strip bank 1
        cart.write(0x1800, 0xCD);
        assert_eq!(cart.read(0x1900), 0xCD, "strip bank 1 is separate");
        cart.read(0x1FE8);
        assert_eq!(cart.read(0x1900), 0xAB, "strip bank 0 retained its byte");
    }

    /// Build an 8 KB 3F image: four 2 KB ROM segments each filled with the
    /// segment index, carrying `STA $3F` twice (and no `STA $3E`, so it's 3F
    /// not 3E).
    fn three_f_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        for seg in 0..4 {
            rom[seg * 2048..(seg + 1) * 2048].fill(seg as u8);
        }
        rom[0x20..0x22].copy_from_slice(&[0x85, 0x3F]); // STA $3F
        rom[0x22..0x24].copy_from_slice(&[0x85, 0x3F]); // ...again
        rom
    }

    #[test]
    fn detect_3f_after_3e_and_e0() {
        let cart = Cartridge::from_rom(&three_f_rom()).expect("3F");
        assert_eq!(cart.scheme(), BankingScheme::ThreeF);

        // Adding an STA $3E makes it 3E (the superset signature wins).
        let mut as_3e = three_f_rom();
        as_3e[0x30..0x32].copy_from_slice(&[0x85, 0x3E]);
        assert_eq!(
            Cartridge::from_rom(&as_3e).expect("3E").scheme(),
            BankingScheme::ThreeE
        );
    }

    #[test]
    fn three_f_window_switches_on_low_writes_with_fixed_tail() {
        let mut cart = Cartridge::from_rom(&three_f_rom()).expect("3F");
        assert_eq!(cart.read(0x1000), 0, "power-on window = segment 0");
        assert_eq!(
            cart.read(0x1800),
            3,
            "$1800-$1FFF fixed to last segment (3)"
        );

        // Any write to $00-$3F stores the window segment from the value.
        cart.snoop_write(0x003F, 2);
        assert_eq!(cart.read(0x1000), 2, "STA $3F,2 → segment 2 in window");
        // Tigervision quirk: a write to *any* $00-$3F address switches too.
        cart.snoop_write(0x0009, 1);
        assert_eq!(cart.read(0x1000), 1, "write to $09 also switches (value 1)");
        // The select wraps modulo the segment count, and the tail stays fixed.
        cart.snoop_write(0x003F, 4 + 3);
        assert_eq!(cart.read(0x1000), 3, "segment select wraps mod 4");
        assert_eq!(cart.read(0x1800), 3, "fixed tail unchanged");
    }

    /// Build a 16 KB F6 image with the Superchip padding (first 128 bytes
    /// repeated into the second 128) in every 4 KB bank, and the bank's fill
    /// byte from $0100 up so bank reads are identifiable.
    fn f6sc_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 16384];
        for (b, bank) in rom.chunks_exact_mut(4096).enumerate() {
            // First 256 bytes stay zero, so the first-128 == next-128 Superchip
            // signature holds; the rest carries the bank index.
            bank[256..].fill(b as u8);
        }
        rom
    }

    #[test]
    fn detect_f6_superchip_overlay() {
        let cart = Cartridge::from_rom(&f6sc_rom()).expect("F6SC");
        assert_eq!(cart.scheme(), BankingScheme::F6, "base scheme is still F6");
        assert!(cart.has_superchip(), "Superchip overlay detected");

        // A plain F6 cart whose banks don't have the repeated padding: no SC.
        let mut plain = vec![0xEA; 16384];
        plain[0] = 0x01; // break the first-128 == second-128 equality
        let cart = Cartridge::from_rom(&plain).expect("F6");
        assert_eq!(cart.scheme(), BankingScheme::F6);
        assert!(!cart.has_superchip(), "no overlay without the padding");
    }

    #[test]
    fn superchip_ram_uses_split_write_read_ports() {
        let mut cart = Cartridge::from_rom(&f6sc_rom()).expect("F6SC");
        // Write port $1000-$107F (128 bytes), read port $1080-$10FF.
        cart.write(0x1000, 0x7E);
        cart.write(0x107F, 0x81);
        assert_eq!(cart.read(0x1080), 0x7E, "RAM cell 0 via read port");
        assert_eq!(cart.read(0x10FF), 0x81, "RAM cell 127 via read port");

        // The RAM overlays every bank — survives a bank switch ($1FF7 → bank 1).
        cart.read(0x1FF7);
        assert_eq!(
            cart.read(0x1080),
            0x7E,
            "Superchip RAM persists across banks"
        );
        // Above the RAM window, normal banked ROM shows through.
        assert_eq!(
            cart.read(0x1200),
            1,
            "ROM bank 1 visible past the RAM window"
        );
    }

    /// Build an 8 KB Activision FE image: bank 0 = 0xE0, bank 1 = 0xE1, with a
    /// JSR-based FE signature and no F8 signature.
    fn fe_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xE0);
        rom[4096..8192].fill(0xE1);
        rom[0x40..0x45].copy_from_slice(&[0x20, 0xC3, 0xF8, 0xA5, 0x82]); // Robot Tank
        rom
    }

    #[test]
    fn detect_fe_not_f8() {
        let cart = Cartridge::from_rom(&fe_rom()).expect("FE");
        assert_eq!(cart.scheme(), BankingScheme::Fe);
        assert_eq!(cart.bank(), 0, "FE powers on at bank 0");

        // An 8K image with both an FE and an F8 signature stays F8 (the !f8 gate).
        let mut both = fe_rom();
        both[0x80..0x83].copy_from_slice(&[0x8D, 0xF9, 0x1F]); // STA $1FF9
        assert_eq!(
            Cartridge::from_rom(&both).expect("F8").scheme(),
            BankingScheme::F8
        );
    }

    #[test]
    fn fe_selects_bank_from_the_access_after_01fe() {
        let mut cart = Cartridge::from_rom(&fe_rom()).expect("FE");
        assert_eq!(cart.read(0x1000), 0xE0, "starts in bank 0");

        // Arm with a $01FE access, then a $D0-valued access → bank 1.
        cart.snoop_fe(0x01FE, 0x00);
        cart.snoop_fe(0x1234, 0xD0);
        assert_eq!(cart.read(0x1000), 0xE1, "value $D0 after $01FE → bank 1");

        // Arm again, a $F0-valued access → bank 0.
        cart.snoop_fe(0x01FE, 0x00);
        cart.snoop_fe(0x1234, 0xF0);
        assert_eq!(cart.read(0x1000), 0xE0, "value $F0 after $01FE → bank 0");

        // A value access *not* preceded by $01FE leaves the bank alone.
        cart.snoop_fe(0x1234, 0xD0);
        assert_eq!(cart.read(0x1000), 0xE0, "lone access doesn't switch");
    }

    /// Build a 10 KB DPC image (Pitfall II shape): 8 KB program with bank
    /// markers at `$1080`, then a 2 KB graphics ROM filled `display[j] = j`.
    fn dpc_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 10240];
        rom[0x80] = 0xB0; // program bank 0, read at $1080
        rom[4096 + 0x80] = 0xB1; // program bank 1, read at $1080
        for (j, byte) in rom[8192..10240].iter_mut().enumerate() {
            *byte = j as u8;
        }
        rom
    }

    #[test]
    fn detect_dpc_by_size() {
        let cart = Cartridge::from_rom(&dpc_rom()).expect("DPC");
        assert_eq!(cart.scheme(), BankingScheme::Dpc);
        assert_eq!(cart.bank(), 1, "DPC powers on at program bank 1 (F8-style)");
    }

    #[test]
    fn dpc_program_banks_switch_f8_style() {
        let mut cart = Cartridge::from_rom(&dpc_rom()).expect("DPC");
        assert_eq!(cart.read(0x1080), 0xB1, "starts in bank 1");
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1080), 0xB0, "$1FF8 → bank 0");
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1080), 0xB1, "$1FF9 → bank 1");
    }

    #[test]
    fn dpc_data_fetcher_streams_the_graphics_rom() {
        let mut cart = Cartridge::from_rom(&dpc_rom()).expect("DPC");
        // Point fetcher 0 at counter 2047 → display index 0. Write registers:
        // counter-low = function 2 ($1050), counter-high = function 3 ($1058).
        cart.write(0x1050, 0xFF);
        cart.write(0x1058, 0x07); // counter = 0x7FF = 2047

        // Reads via function 1 ($1008) return display[2047 - counter] and then
        // decrement the counter, so the fetcher streams display[0], [1], [2]…
        assert_eq!(cart.read(0x1008), 0, "display[0]");
        assert_eq!(cart.read(0x1008), 1, "display[1] after decrement");
        assert_eq!(cart.read(0x1008), 2, "display[2]");
    }

    #[test]
    fn dpc_random_number_generator_advances_and_resets() {
        let mut cart = Cartridge::from_rom(&dpc_rom()).expect("DPC");
        // Function 0, index < 4 ($1000) reads the RNG (clocked on each access).
        // From the reset state (1) the LFSR yields 3, then 7.
        assert_eq!(cart.read(0x1000), 3, "first RNG step");
        assert_eq!(cart.read(0x1000), 7, "second RNG step");

        // RNG-reset register (function 6 → $1070) returns it to the seed.
        cart.write(0x1070, 0x00);
        assert_eq!(cart.read(0x1000), 3, "reset restarts the sequence");
    }

    #[test]
    fn dpc_music_fetcher_advances_with_elapsed_cycles() {
        let mut cart = Cartridge::from_rom(&dpc_rom()).expect("DPC");
        // One OSC clock per CPU cycle makes the timing deterministic.
        cart.set_dpc_clock_rate(DPC_PITCH);

        // Fetcher 5 in music mode: top 0x0A, counter low 0x08, bottom 0 — set
        // the counter *before* enabling music mode (a music-mode counter-low
        // write reloads from top instead).
        cart.write(0x1045, 0x0A); // top (fetcher 5)
        cart.write(0x1055, 0x08); // counter low
        cart.write(0x105D, 0x10); // counter high 0 + music-mode enable

        // No cycles elapsed yet: the flag is clear (top write cleared it), so
        // the music amplitude read ($1005, function 0 / index 5) is 0.
        assert_eq!(cart.read(0x1005), 0x00, "no elapsed cycles → amplitude 0");

        // Advance 3 OSC clocks: counter 0x08 → 0x05, which sits between bottom
        // and top, so fetcher 5's flag sets and voice 0 contributes (0x04).
        for _ in 0..3 {
            cart.tick();
        }
        assert_eq!(
            cart.read(0x1005),
            0x04,
            "3 clocks → fetcher 5 flag set → 0x04"
        );
    }

    #[test]
    fn detect_2k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 2048]).expect("2K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_4k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 4096]).expect("4K");
        assert_eq!(cart.scheme(), BankingScheme::None);
    }

    #[test]
    fn detect_f8_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("F8");
        assert_eq!(cart.scheme(), BankingScheme::F8);
        assert_eq!(cart.bank(), 1);
    }

    #[test]
    fn detect_f6_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("F6");
        assert_eq!(cart.scheme(), BankingScheme::F6);
        assert_eq!(cart.bank(), 3);
    }

    #[test]
    fn detect_f4_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("F4");
        assert_eq!(cart.scheme(), BankingScheme::F4);
        assert_eq!(cart.bank(), 7);
    }

    #[test]
    fn reject_invalid_size() {
        assert!(Cartridge::from_rom(&vec![0u8; 5000]).is_err());
    }

    #[test]
    fn f8_bank_switching() {
        let mut rom = vec![0u8; 8192];
        rom[..4096].fill(0xAA);
        rom[4096..].fill(0xBB);
        let mut cart = Cartridge::from_rom(&rom).expect("F8");
        assert_eq!(cart.read(0x1000), 0xBB);
        cart.read(0x1FF8);
        assert_eq!(cart.read(0x1000), 0xAA);
        cart.read(0x1FF9);
        assert_eq!(cart.read(0x1000), 0xBB);
    }

    #[test]
    fn two_kb_rom_mirrors() {
        let mut rom = vec![0u8; 2048];
        rom[0] = 0x42;
        let mut cart = Cartridge::from_rom(&rom).expect("2K");
        assert_eq!(cart.read(0x1000), 0x42);
        assert_eq!(cart.read(0x1800), 0x42);
    }
}
