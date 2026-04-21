//! Zorro-II autoconfig state machine.
//!
//! Implements the Commodore Amiga Zorro-II autoconfig protocol used
//! by `expansion.library` during boot to discover and map expansion
//! boards. This crate ships a single board variant — fast RAM —
//! which is enough to surface Zorro-II RAM to Exec's `AvailMem`.
//! Other board types (SCSI, network, video) share the probe state
//! machine and will layer on top of it.
//!
//! # Protocol summary
//!
//! On reset the first unconfigured board answers at the probe window
//! `$E80000-$E8007F`. Its 64-byte config ROM exposes:
//!
//! | Offset | Meaning                                     |
//! | ------ | ------------------------------------------- |
//! | `$00`  | `ER_TYPE`   — board class + size            |
//! | `$04`  | `ER_PRODUCT` — per-manufacturer product ID  |
//! | `$08`  | `ER_FLAGS`   — board flags                  |
//! | `$10`  | `ER_MANUFACTURER` (2 bytes)                 |
//! | `$18`  | `ER_SERIALNUMBER` (4 bytes)                 |
//! | `$20`  | `ER_INITDIAG`   (2 bytes)                   |
//!
//! Host writes `$E80048` (base high nibble) and `$E8004A` (base low
//! nibble) to map the board; `$E8004C` is the shut-up escape.
//!
//! # Bit-level weirdness we must honour
//!
//! Each config-ROM byte is **split across two word-aligned offsets**
//! (`base + 2n` high nibble, `base + 2n + 2` low nibble), delivered
//! in the high 4 bits of the returned 16-bit word. Every data bit
//! is **inverted** except `ER_TYPE` bits 6-7 (the board-class bits
//! host software uses to tell "board present" apart from floating
//! bus).
//!
//! ## `ER_TYPE` layout
//!
//! ```text
//!   bit 7-6  board class      11 = Zorro-II
//!   bit 5    memory board     1 = yes (unused by us — we set it)
//!   bit 4    read-from-ROM    1 = yes
//!   bit 3    reserved
//!   bit 2-0  size code        000 = 8M, 001 = 64K, 010 = 128K,
//!                             011 = 256K, 100 = 512K, 101 = 1M,
//!                             110 = 2M, 111 = 4M
//! ```
//!
//! # References
//!
//! - Amiga Hardware Reference Manual 3rd ed., Chapter 7 "Expansion
//!   Devices" — table 7-3 "Autoconfig Register Summary"
//! - Amiga Expansion Series ZORRO II specification (November 1987)
//! - `libraries/configregs.i` — AmigaOS autodocs struct offsets

#![forbid(unsafe_code)]

/// `ER_TYPE` bit 7: board class MSB. `11` = Zorro-II, `10` =
/// Zorro-III (not modelled here), `01` = reserved, `00` = no board
/// (floating bus).
pub const ER_TYPE_ZORRO_II: u8 = 0b1100_0000;

/// `ER_TYPE` bit 5 — memory-containing board.
pub const ER_TYPE_MEMORY: u8 = 0b0010_0000;

/// `ER_TYPE` bit 4 — "read from ROM" linkable-driver flag. Unused by
/// plain RAM boards but set on common boards, so we follow suit.
pub const ER_TYPE_EXTENDED: u8 = 0b0001_0000;

/// Commodore's assigned Zorro-II manufacturer ID. Set on CBM-branded
/// boards (A501, A590, ...). Tests use it as a known value; picked
/// here as the default for the fast-RAM board.
pub const MANUFACTURER_COMMODORE: u16 = 0x0202;

/// Size of the autoconfig probe window at `$E80000-$E8007F`.
pub const AUTOCONFIG_WINDOW_BYTES: u32 = 0x80;

/// Board-size code for `ER_TYPE` bits 2-0. Maps RAM size to the
/// 3-bit encoding the Zorro-II ROM header uses.
#[must_use]
pub const fn size_code_for_kib(size_kib: u32) -> Option<u8> {
    Some(match size_kib {
        8192 => 0b000,  // 8M ambiguity: zero means 8M
        64 => 0b001,
        128 => 0b010,
        256 => 0b011,
        512 => 0b100,
        1024 => 0b101,
        2048 => 0b110,
        4096 => 0b111,
        _ => return None,
    })
}

/// Configuration state of a Zorro-II board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoconfigState {
    /// Board is unconfigured — reads answer at `$E80000` and the
    /// host hasn't assigned a base address yet.
    Unconfigured,
    /// Host has written the high nibble of the base address via
    /// `$E80048` but not yet the low nibble.
    WaitingLowBase { hi: u8 },
    /// Host has written `$E8004C` shut-up — board is silent forever
    /// (until reset).
    ShutUp,
    /// Host has written both halves of the base address. Board is
    /// now mapped into memory starting at `base` and the probe
    /// window reverts to floating bus.
    Configured { base: u32 },
}

/// Board-specific payload held by an `AutoconfigBoard`.
#[derive(Debug, Clone)]
enum Payload {
    /// Plain Zorro-II fast RAM with the given byte-size backing.
    FastRam { bytes: Vec<u8> },
}

/// A single Zorro-II autoconfig board.
///
/// Today this is always a fast-RAM board; the shape generalises to
/// other Zorro-II peripherals by extending `Payload` and the
/// `ER_TYPE` computation. Chained multi-board configs are a later
/// milestone.
#[derive(Debug, Clone)]
pub struct AutoconfigBoard {
    manufacturer: u16,
    product: u8,
    serial: u32,
    size_code: u8,
    state: AutoconfigState,
    payload: Payload,
}

impl AutoconfigBoard {
    /// Create a Zorro-II fast-RAM board of the given size. Size must
    /// be one of {64, 128, 256, 512, 1024, 2048, 4096, 8192} KiB
    /// (validated by the `size_code_for_kib` helper).
    ///
    /// # Panics
    ///
    /// Panics if `size_kib` is outside the supported set.
    #[must_use]
    pub fn fast_ram(size_kib: u32) -> Self {
        let size_code = size_code_for_kib(size_kib).unwrap_or_else(|| {
            panic!("fast_ram: {size_kib} KiB is not a Zorro-II board size")
        });
        Self {
            manufacturer: MANUFACTURER_COMMODORE,
            product: 0x09, // arbitrary — not currently checked by ROM
            serial: 0x0000_0001,
            size_code,
            state: AutoconfigState::Unconfigured,
            payload: Payload::FastRam {
                bytes: vec![0; (size_kib as usize) * 1024],
            },
        }
    }

    /// Current configuration state.
    #[must_use]
    pub fn state(&self) -> AutoconfigState {
        self.state
    }

    /// `true` while the board is answering at `$E80000`.
    #[must_use]
    pub fn visible_in_probe_window(&self) -> bool {
        matches!(
            self.state,
            AutoconfigState::Unconfigured | AutoconfigState::WaitingLowBase { .. }
        )
    }

    /// Base address assigned by the host, if any.
    #[must_use]
    pub fn base(&self) -> Option<u32> {
        match self.state {
            AutoconfigState::Configured { base } => Some(base),
            _ => None,
        }
    }

    /// Size of the board's RAM backing in bytes, regardless of
    /// configuration state.
    #[must_use]
    pub fn ram_size(&self) -> u32 {
        match &self.payload {
            Payload::FastRam { bytes } => bytes.len() as u32,
        }
    }

    /// Read one byte from the board's mapped RAM, if the address
    /// lands inside the configured region.
    #[must_use]
    pub fn read_ram_byte(&self, addr: u32) -> Option<u8> {
        let base = self.base()?;
        let size = self.ram_size();
        if addr < base || addr >= base + size {
            return None;
        }
        match &self.payload {
            Payload::FastRam { bytes } => Some(bytes[(addr - base) as usize]),
        }
    }

    /// Write one byte into the board's mapped RAM, if the address
    /// lands inside the configured region. No-op otherwise.
    pub fn write_ram_byte(&mut self, addr: u32, val: u8) {
        let Some(base) = self.base() else { return };
        let size = self.ram_size();
        if addr < base || addr >= base + size {
            return;
        }
        match &mut self.payload {
            Payload::FastRam { bytes } => bytes[(addr - base) as usize] = val,
        }
    }

    /// Read one word from the autoconfig probe window
    /// `$E80000..$E80080`. The passed `offset` is relative to the
    /// window base (0..=`$7E`). Returns floating-bus (`$FFFF`) once
    /// the board is configured or shut up.
    ///
    /// Offset layout: each config byte occupies four bytes of
    /// address space — the "high nibble" slot at `offset & !3`
    /// delivers the high 4 bits; the "low nibble" slot at `(offset
    /// & !3) + 2` delivers the low 4 bits. Both land in the top 4
    /// bits of the returned 16-bit word; the bottom 12 bits are
    /// don't-cares (real boards float them; we return zero).
    #[must_use]
    pub fn read_word(&self, offset: u16) -> u16 {
        if !self.visible_in_probe_window() {
            return 0xFFFF;
        }
        if offset >= AUTOCONFIG_WINDOW_BYTES as u16 {
            return 0xFFFF;
        }
        let hi_nibble_offset = offset & !0x0003;
        let rom_byte = self.config_rom_byte(hi_nibble_offset);
        let is_lo_nibble = (offset & 0x0002) != 0;
        let nibble = if is_lo_nibble {
            rom_byte & 0x0F
        } else {
            (rom_byte >> 4) & 0x0F
        };
        u16::from(nibble) << 12
    }

    /// Accept one word written to the probe window. Handles base-
    /// address configuration (`$48`/`$4A`) and the shut-up command
    /// (`$4C`). Other offsets are no-ops.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        if !self.visible_in_probe_window() {
            return;
        }
        match offset {
            // ec_BaseAddress high nibble (top 4 bits of upper byte of
            // base address) goes to $48. The data appears in the high
            // 4 bits of the write value, same as the read protocol.
            0x48 => {
                let hi = ((val >> 12) & 0x0F) as u8;
                self.state = AutoconfigState::WaitingLowBase { hi };
            }
            // Low nibble goes to $4A — completes the base address.
            //
            // Base address byte layout (per Amiga Expansion Series
            // ZORRO II spec, "Configuration Address" section):
            //   top 4 bits of the base address's upper byte come
            //   from the $48 write; next 4 bits come from the $4A
            //   write. On Zorro-II the base address is always
            //   a multiple of $10000, so the lower 16 bits of the
            //   assigned base are always zero.
            0x4A => {
                if let AutoconfigState::WaitingLowBase { hi } = self.state {
                    let lo = ((val >> 12) & 0x0F) as u8;
                    let upper_byte = (hi << 4) | lo;
                    let base = (u32::from(upper_byte)) << 16;
                    self.state = AutoconfigState::Configured { base };
                }
            }
            // ec_Shutup: host is refusing the board. We go silent.
            0x4C => {
                self.state = AutoconfigState::ShutUp;
            }
            _ => {}
        }
    }

    /// Compute the post-inversion byte delivered at the given high-
    /// nibble window offset. `hi_nibble_offset` must be a multiple
    /// of 4 and less than `AUTOCONFIG_WINDOW_BYTES`.
    ///
    /// See the module doc: every bit is inverted except `ER_TYPE`
    /// bits 6-7. Inversion is applied per-byte here so `read_word`
    /// can focus on nibble dispatch.
    fn config_rom_byte(&self, hi_nibble_offset: u16) -> u8 {
        let nominal = self.nominal_rom_byte(hi_nibble_offset);
        if hi_nibble_offset == 0 {
            // ER_TYPE: bits 6-7 are NOT inverted (they carry the
            // board-present signature the host uses to distinguish
            // a live board from floating bus).
            (nominal & 0b1100_0000) | (!nominal & 0b0011_1111)
        } else {
            !nominal
        }
    }

    /// Return the ROM byte at the given high-nibble window offset
    /// in "nominal" form — pre-inversion, matching the AmigaOS
    /// autodoc struct field at that position.
    ///
    /// Each config byte occupies four window bytes (high nibble at
    /// `4n`, low nibble at `4n + 2`), so valid high-nibble offsets
    /// are `$00, $04, $08, ..., $7C`. The AmigaOS `ExpansionRom`
    /// struct field layout lands on these offsets as:
    ///
    /// ```text
    ///   $00  er_Type
    ///   $04  er_Product
    ///   $08  er_Flags
    ///   $0C  er_Reserved03
    ///   $10  er_Manufacturer (high byte)
    ///   $14  er_Manufacturer (low byte)
    ///   $18  er_SerialNumber byte 3 (MSB)
    ///   $1C  er_SerialNumber byte 2
    ///   $20  er_SerialNumber byte 1
    ///   $24  er_SerialNumber byte 0 (LSB)
    ///   $28  er_InitDiagVec (high byte)
    ///   $2C  er_InitDiagVec (low byte)
    ///   $30..$7C  reserved (zero)
    /// ```
    fn nominal_rom_byte(&self, hi_nibble_offset: u16) -> u8 {
        match hi_nibble_offset {
            0x00 => ER_TYPE_ZORRO_II | ER_TYPE_MEMORY | self.size_code,
            0x04 => self.product,
            0x08 => 0x40, // ER_FLAGS: memlist flag
            0x10 => (self.manufacturer >> 8) as u8,
            0x14 => (self.manufacturer & 0xFF) as u8,
            0x18 => (self.serial >> 24) as u8,
            0x1C => ((self.serial >> 16) & 0xFF) as u8,
            0x20 => ((self.serial >> 8) & 0xFF) as u8,
            0x24 => (self.serial & 0xFF) as u8,
            _ => 0x00,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reconstruct a byte from the pair of word reads at
    /// `hi_offset` (the high-nibble slot) and `hi_offset + 2` (the
    /// low-nibble slot). Helper for the protocol-level tests — the
    /// host does the same reconstruction during autoconfig scanning.
    fn read_byte_from_probe(board: &AutoconfigBoard, hi_offset: u16) -> u8 {
        assert!(hi_offset & 0x3 == 0,
            "hi_offset must land on a high-nibble slot (multiple of 4)");
        let hi = ((board.read_word(hi_offset) >> 12) & 0x0F) as u8;
        let lo = ((board.read_word(hi_offset + 2) >> 12) & 0x0F) as u8;
        (hi << 4) | lo
    }

    #[test]
    fn fast_ram_board_starts_unconfigured_and_visible() {
        let board = AutoconfigBoard::fast_ram(2048);
        assert_eq!(board.state(), AutoconfigState::Unconfigured);
        assert!(board.visible_in_probe_window());
        assert_eq!(board.ram_size(), 2048 * 1024);
        assert_eq!(board.base(), None);
    }

    #[test]
    fn er_type_reads_zorro_ii_memory_board_with_correct_size_code() {
        // ER_TYPE is the only byte where bits 6-7 are NOT inverted,
        // so the returned value encodes the Zorro-II marker cleanly
        // at the top while the lower 6 bits are inverted.
        let board = AutoconfigBoard::fast_ram(2048);
        let byte = read_byte_from_probe(&board, 0x00);
        // Nominal ER_TYPE: ER_TYPE_ZORRO_II | ER_TYPE_MEMORY | 0b110
        //                = 1100_0000 | 0010_0000 | 0000_0110
        //                = 1110_0110 = $E6
        // Selective invert: bits 7-6 unchanged; bits 5-0 invert.
        // -> 1100_0000 | (!1110_0110 & 0011_1111)
        //    = 1100_0000 | (0001_1001)
        //    = 1101_1001 = $D9
        assert_eq!(byte, 0xD9);
    }

    #[test]
    fn manufacturer_reads_commodore_id_post_inversion() {
        let board = AutoconfigBoard::fast_ram(512);
        // Manufacturer $0202:
        //   high byte at high-nibble offset $10 (low nibble $12)
        //   low  byte at high-nibble offset $14 (low nibble $16)
        // Both arrive inverted — host un-inverts.
        let hi = read_byte_from_probe(&board, 0x10);
        let lo = read_byte_from_probe(&board, 0x14);
        assert_eq!(hi, !(0x02u8)); // inverted high byte of $0202
        assert_eq!(lo, !(0x02u8)); // inverted low byte of $0202
    }

    #[test]
    fn base_address_assignment_is_two_step() {
        let mut board = AutoconfigBoard::fast_ram(2048);
        // Host sends high nibble = $2 (upper nibble of upper base
        // byte — maps to $2x_xxxx).
        board.write_word(0x48, 0x2000);
        assert_eq!(board.state(), AutoconfigState::WaitingLowBase { hi: 2 });
        assert!(board.visible_in_probe_window());

        // Now low nibble = $0 — complete base $20_0000.
        board.write_word(0x4A, 0x0000);
        assert_eq!(board.state(), AutoconfigState::Configured { base: 0x0020_0000 });
        assert!(!board.visible_in_probe_window());
        assert_eq!(board.base(), Some(0x0020_0000));
    }

    #[test]
    fn read_after_configuration_returns_floating_bus() {
        let mut board = AutoconfigBoard::fast_ram(512);
        board.write_word(0x48, 0x2000);
        board.write_word(0x4A, 0x0000);
        // Probe window reverts to floating bus.
        assert_eq!(board.read_word(0x00), 0xFFFF);
        assert_eq!(board.read_word(0x10), 0xFFFF);
    }

    #[test]
    fn shut_up_silences_board_permanently() {
        let mut board = AutoconfigBoard::fast_ram(512);
        board.write_word(0x4C, 0x0000);
        assert_eq!(board.state(), AutoconfigState::ShutUp);
        assert_eq!(board.read_word(0x00), 0xFFFF);
        // Further base-writes do nothing after shut-up.
        board.write_word(0x48, 0x2000);
        assert_eq!(board.state(), AutoconfigState::ShutUp);
    }

    #[test]
    fn configured_board_serves_reads_from_its_base() {
        let mut board = AutoconfigBoard::fast_ram(256);
        board.write_word(0x48, 0x2000);
        board.write_word(0x4A, 0x0000);
        // Write a byte at base + 0x100 through the board's write
        // helper, then read it back.
        board.write_ram_byte(0x0020_0100, 0xAB);
        assert_eq!(board.read_ram_byte(0x0020_0100), Some(0xAB));
    }

    #[test]
    fn configured_board_rejects_out_of_range_access() {
        let mut board = AutoconfigBoard::fast_ram(256);
        board.write_word(0x48, 0x2000);
        board.write_word(0x4A, 0x0000);
        // 256 KiB = $40000; $20_0000 + $40000 = $24_0000 is one past
        // the top.
        assert_eq!(board.read_ram_byte(0x0024_0000), None);
        assert_eq!(board.read_ram_byte(0x0019_FFFF), None);
    }

    #[test]
    fn size_code_round_trips_for_all_supported_sizes() {
        let cases = [
            (8192, 0b000), (64, 0b001), (128, 0b010), (256, 0b011),
            (512, 0b100), (1024, 0b101), (2048, 0b110), (4096, 0b111),
        ];
        for (kib, code) in cases {
            assert_eq!(size_code_for_kib(kib), Some(code),
                "size code mismatch for {kib} KiB");
        }
        assert_eq!(size_code_for_kib(100), None);
        assert_eq!(size_code_for_kib(3072), None);
    }

    #[test]
    fn out_of_window_reads_return_floating_bus() {
        let board = AutoconfigBoard::fast_ram(512);
        // $80 is one past the probe window.
        assert_eq!(board.read_word(0x80), 0xFFFF);
        assert_eq!(board.read_word(0xFE), 0xFFFF);
    }
}
