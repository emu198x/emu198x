//! Commodore Gayle gate array — IDE interface, PCMCIA slot, and
//! address decoding for the Amiga 600 and Amiga 1200.
//!
//! Gayle sits between the CPU and the $D80000-$DFFFFF address range,
//! providing:
//!
//! - IDE task-file registers at `$DA0000-$DA3FFF`.
//! - Four Gayle control registers at `$DA8000-$DABFFF` (Card Status,
//!   Interrupt Request, Interrupt Enable, Configuration).
//! - PCMCIA slot routing for common memory (`$600000-$9FFFFF`),
//!   attribute/IO (`$A00000-$A5FFFF`), and card reset
//!   (`$A40000-$A5FFFF`).
//!
//! Stage A scope (per `knowledge/decisions/amiga-machine-rollout-plan.md`):
//! enough to let Kickstart 3.x probe Gayle without crashing. With no
//! IDE drive and no PCMCIA card, STATUS reads `$7F` ("no drive"),
//! other IDE registers read `$FF`, and the control registers are
//! zero-initialised and writeable. PCMCIA reads return `$FF` (no
//! card detected).
//!
//! IDE drive emulation, PCMCIA SRAM / CompactFlash / NE2000 support,
//! and the full A600/A1200 hard-disk boot path land in later stages
//! alongside the catalogue entries that require them. The full donor
//! implementation lives at `Emu198x-Oldest/crates/commodore-gayle/`
//! (2334 lines including 841 lines of NE2000 PCMCIA — dropped here).

// ── Gayle Card Status register bit definitions ────────────────────

/// IDE interrupt active.
pub const GAYLE_CS_IDE: u8 = 0x80;
/// Card detect — card is inserted.
pub const GAYLE_CS_CCDET: u8 = 0x40;
/// Battery voltage detect 1.
pub const GAYLE_CS_BVD1: u8 = 0x20;
/// Battery voltage detect 2.
pub const GAYLE_CS_BVD2: u8 = 0x10;
/// Write protect — card is writable when set.
pub const GAYLE_CS_WR: u8 = 0x08;
/// Busy / IRQ — PCMCIA card interrupt pending.
pub const GAYLE_CS_BSY: u8 = 0x04;
/// PCMCIA digital-audio path enable.
pub const GAYLE_CS_DAEN: u8 = 0x02;
/// Disable — when set, PCMCIA slot is disabled.
pub const GAYLE_CS_DIS: u8 = 0x01;

/// Open-bus return value for IDE registers with no drive attached.
const IDE_NO_DRIVE_OPEN_BUS: u8 = 0xFF;

/// STATUS register value when no drive is attached. Reads as $7F
/// across WinUAE and fs-uae's IDE controllers: BSY clear, DRDY set,
/// DRQ clear, ERR clear, plus the unused bits high.
const IDE_NO_DRIVE_STATUS: u8 = 0x7F;

/// IDE STATUS register offset within the IDE task-file window.
const IDE_REG_STATUS: u32 = 0x1C;

/// Bits shared by Gayle's interrupt-request and interrupt-enable source fields.
const GAYLE_INTERRUPT_SOURCE_MASK: u8 = 0xFC;
/// RESET control bit in the Gayle interrupt-request register.
const GAYLE_IRQ_RESET: u8 = 0x02;
/// BERR control bit in the Gayle interrupt-request register.
const GAYLE_IRQ_BERR: u8 = 0x01;

/// Side-effect-free view of Gayle's four implemented control registers.
///
/// These are the exact values retained by the component. The configuration
/// register has already been masked to its implemented low nibble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleRegisterDiagnosticSnapshot {
    /// Raw Card Status register at `$DA8000`.
    pub card_status: u8,
    /// Raw Interrupt Request register at `$DA9000`.
    pub interrupt_request: u8,
    /// Raw Interrupt Enable register at `$DAA000`.
    pub interrupt_enable: u8,
    /// Effective low nibble of the Configuration register at `$DAB000`.
    pub configuration: u8,
}

/// Decoded Card Status register signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleCardStatusDiagnosticSnapshot {
    /// Raw Card Status register.
    pub raw: u8,
    /// IDE interrupt-active signal (`IDE`, bit 7).
    pub ide_interrupt_active: bool,
    /// PCMCIA card-detect signal (`CCDET`, bit 6).
    pub card_detected: bool,
    /// First PCMCIA battery-voltage signal (`BVD1`, bit 5).
    pub battery_voltage_detect_1: bool,
    /// Second PCMCIA battery-voltage signal (`BVD2`, bit 4).
    pub battery_voltage_detect_2: bool,
    /// PCMCIA write-status signal (`WR`, bit 3).
    pub card_writable: bool,
    /// PCMCIA busy/interrupt signal (`BSY`, bit 2).
    pub card_interrupt_pending: bool,
    /// PCMCIA digital-audio enable signal (`DAEN`, bit 1).
    pub digital_audio_enabled: bool,
    /// PCMCIA disable signal (`DIS`, bit 0).
    pub pcmcia_disabled: bool,
}

/// Decoded values for the six Gayle interrupt-source bits.
///
/// The type is used for the request, enable, and enabled-request views so
/// diagnostic consumers can compare the same signals without decoding masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleInterruptSourceDiagnosticSnapshot {
    /// IDE signal (`IDE`, bit 7).
    pub ide: bool,
    /// PCMCIA card-detect signal (`CCDET`, bit 6).
    pub ccdet: bool,
    /// First PCMCIA battery-voltage signal (`BVD1`, bit 5).
    pub bvd1: bool,
    /// Second PCMCIA battery-voltage signal (`BVD2`, bit 4).
    pub bvd2: bool,
    /// PCMCIA write-status signal (`WR`, bit 3).
    pub wr: bool,
    /// PCMCIA busy/interrupt signal (`BSY`, bit 2).
    pub bsy: bool,
}

impl GayleInterruptSourceDiagnosticSnapshot {
    const fn from_register(value: u8) -> Self {
        Self {
            ide: value & GAYLE_CS_IDE != 0,
            ccdet: value & GAYLE_CS_CCDET != 0,
            bvd1: value & GAYLE_CS_BVD1 != 0,
            bvd2: value & GAYLE_CS_BVD2 != 0,
            wr: value & GAYLE_CS_WR != 0,
            bsy: value & GAYLE_CS_BSY != 0,
        }
    }
}

/// Side-effect-free view of Gayle's interrupt registers and decoded signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleInterruptDiagnosticSnapshot {
    /// Raw Interrupt Request register.
    pub request_register: u8,
    /// Raw Interrupt Enable register.
    pub enable_register: u8,
    /// Interrupt-source bits currently present in the request register.
    pub requested_sources: GayleInterruptSourceDiagnosticSnapshot,
    /// Interrupt-source bits currently present in the enable register.
    pub enabled_sources: GayleInterruptSourceDiagnosticSnapshot,
    /// Raw source mask produced by `request_register & enable_register`.
    pub enabled_request_mask: u8,
    /// Decoded source mask produced by `request_register & enable_register`.
    pub enabled_requested_sources: GayleInterruptSourceDiagnosticSnapshot,
    /// Directly stored RESET control bit from the request register.
    pub reset: bool,
    /// Directly stored BERR control bit from the request register.
    pub bus_error: bool,
    /// Low two enable-register bits, for which this implementation assigns no
    /// interrupt-source behavior.
    pub non_source_enable_bits: u8,
    /// Physical IDE IRQ input as implemented by the current no-drive model.
    pub ide_irq_line_pending: bool,
}

/// Side-effect-free decoded view of Gayle's four-bit configuration register.
///
/// The component does not yet assign behavior to individual configuration
/// bits, so their bit positions are exposed without speculative names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleConfigurationDiagnosticSnapshot {
    /// Effective low-nibble register value.
    pub raw: u8,
    /// Configuration bit 0.
    pub bit_0: bool,
    /// Configuration bit 1.
    pub bit_1: bool,
    /// Configuration bit 2.
    pub bit_2: bool,
    /// Configuration bit 3.
    pub bit_3: bool,
}

/// Side-effect-free view of the currently implemented IDE interface.
///
/// Gayle presently has no IDE device or task-file state. The snapshot makes
/// the fixed no-drive bus behavior explicit and reserves optional scalar
/// fields for media metadata without copying any backing media payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleIdeDiagnosticSnapshot {
    /// Whether an IDE drive backend is attached.
    pub drive_attached: bool,
    /// Physical IDE interrupt input.
    pub irq_line_pending: bool,
    /// Value returned by the IDE STATUS register.
    pub status_register: u8,
    /// Value returned by byte reads from other IDE task-file registers.
    pub task_file_open_bus: u8,
    /// Value returned by word reads from the IDE DATA register.
    pub data_register_open_bus: u16,
    /// Attached-media size, absent while no drive backend exists.
    pub media_size_bytes: Option<u64>,
    /// Attached-media byte position, absent while no drive backend exists.
    pub media_position_bytes: Option<u64>,
    /// Whether task-file and DATA writes are discarded.
    pub writes_discarded: bool,
}

/// Side-effect-free view of the currently implemented PCMCIA interface.
///
/// The Card Status fields remain software-writeable in the Stage A model, so
/// `reported_card_detected` is kept distinct from `backend_attached`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GaylePcmciaDiagnosticSnapshot {
    /// Whether a PCMCIA card backend is attached.
    pub backend_attached: bool,
    /// Card-detect level reported by the Card Status register.
    pub reported_card_detected: bool,
    /// Whether the Card Status register disables the slot.
    pub slot_disabled: bool,
    /// Busy/interrupt level reported by the Card Status register.
    pub interrupt_pending: bool,
    /// Write-status level reported by the Card Status register.
    pub card_writable: bool,
    /// Whether the Card Status register enables the PCMCIA digital-audio path.
    pub digital_audio_enabled: bool,
    /// First battery-voltage level reported by the Card Status register.
    pub battery_voltage_detect_1: bool,
    /// Second battery-voltage level reported by the Card Status register.
    pub battery_voltage_detect_2: bool,
    /// Common-memory backend size, absent while no card backend exists.
    pub common_memory_size_bytes: Option<u64>,
    /// Attribute-memory backend size, absent while no card backend exists.
    pub attribute_memory_size_bytes: Option<u64>,
    /// I/O-space backend size, absent while no card backend exists.
    pub io_space_size_bytes: Option<u64>,
}

/// Side-effect-free description of Gayle's implemented address decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleAddressDecodeDiagnosticSnapshot {
    /// Mask applied to obtain the local address within Gayle's 1 MiB region.
    pub local_address_mask: u32,
    /// Address bits which must all be set for the A1200 Gayle filter to match.
    pub required_address_bits: u32,
    /// Bit selecting the control-register half of the decoded window.
    pub control_register_select_bit: u32,
    /// Mask selecting one of the four control registers.
    pub control_register_index_mask: u32,
    /// Mask used to obtain an IDE task-file offset.
    pub ide_register_offset_mask: u32,
    /// IDE STATUS register offset within the task-file window.
    pub ide_status_offset: u32,
}

/// Complete side-effect-free diagnostic view of the implemented Gayle state.
///
/// The snapshot contains every private mutable field retained by [`Gayle`],
/// decoded register signals, and the fixed behavior of the current absent IDE
/// and PCMCIA backends. It never reads or copies backing media payload bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GayleDiagnosticSnapshot {
    /// Exact values retained by Gayle's four control registers.
    pub registers: GayleRegisterDiagnosticSnapshot,
    /// Decoded Card Status register.
    pub card_status: GayleCardStatusDiagnosticSnapshot,
    /// Interrupt request, enable, and physical IDE-line state.
    pub interrupts: GayleInterruptDiagnosticSnapshot,
    /// Decoded low-nibble Configuration register.
    pub configuration: GayleConfigurationDiagnosticSnapshot,
    /// Implemented IDE backend and fixed no-drive register behavior.
    pub ide: GayleIdeDiagnosticSnapshot,
    /// Implemented PCMCIA backend and reported Card Status signals.
    pub pcmcia: GaylePcmciaDiagnosticSnapshot,
    /// Implemented CPU-address decoder.
    pub address_decode: GayleAddressDecodeDiagnosticSnapshot,
}

/// Gayle gate array state. Stage A: no IDE drive, no PCMCIA card.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Gayle {
    /// Card Status register at `$DA8000`.
    gayle_cs: u8,
    /// Interrupt Request register at `$DA9000`. Bits 2..7 are
    /// write-to-clear; bits 0..1 (RESET/BERR) are written directly.
    gayle_irq: u8,
    /// Interrupt Enable register at `$DAA000`.
    gayle_int: u8,
    /// Configuration register at `$DAB000`. Only the low nibble is
    /// significant.
    gayle_cfg: u8,
}

impl Gayle {
    /// Construct a fresh Gayle with no IDE drive and no PCMCIA card.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gayle_cs: 0,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0,
        }
    }

    /// Read a byte from a Gayle-decoded address. Callers should only
    /// route addresses in `$D80000-$DFFFFF` here; addresses outside
    /// the A1200 Gayle filter return `0`.
    #[must_use]
    pub const fn read(&self, addr: u32) -> u8 {
        let local = addr & 0x000F_FFFF;

        // A1200 address filter: only respond when bits 17 and 19 are
        // both set. Bit-19 is the `$80000` half of `$D80000`; bit-17
        // is the `$20000` half of `$DA0000`.
        if local & 0x000A_0000 != 0x000A_0000 {
            return 0;
        }

        // Gayle control registers at `$DA8000-$DABFFF`: bit 15 set.
        if local & 0x8000 != 0 {
            return match (local >> 12) & 0x03 {
                0 => self.gayle_cs,
                1 => self.gayle_irq,
                2 => self.gayle_int,
                _ => self.gayle_cfg & 0x0F,
            };
        }

        // IDE task-file registers at `$DA0000-$DA3FFF`. With no drive
        // attached, STATUS reads $7F and everything else reads $FF.
        if (local & 0x3FFF) == IDE_REG_STATUS {
            IDE_NO_DRIVE_STATUS
        } else {
            IDE_NO_DRIVE_OPEN_BUS
        }
    }

    /// Read a 16-bit word from a Gayle-decoded address. The IDE DATA
    /// register transfers 16 bits at a time; with no drive, the read
    /// returns `0xFFFF` (open bus).
    #[must_use]
    pub const fn read_word(&self, addr: u32) -> u16 {
        let local = addr & 0x000F_FFFF;
        if local & 0x000A_0000 != 0x000A_0000 {
            return 0;
        }
        if local & 0x8000 != 0 {
            return self.read(addr) as u16;
        }
        // IDE no-drive: open bus across both lanes.
        0xFFFF
    }

    /// Write a byte to a Gayle-decoded address.
    pub fn write(&mut self, addr: u32, val: u8) {
        let local = addr & 0x000F_FFFF;

        if local & 0x000A_0000 != 0x000A_0000 {
            return;
        }

        if local & 0x8000 != 0 {
            match (local >> 12) & 0x03 {
                0 => self.gayle_cs = val,
                1 => {
                    // Bits 2..7: writing 0 clears the corresponding
                    // flag. Bits 0..1 (RESET / BERR) are written
                    // directly.
                    self.gayle_irq = (self.gayle_irq & val) | (val & 0x03);
                }
                2 => self.gayle_int = val,
                _ => self.gayle_cfg = val & 0x0F,
            }
            return;
        }

        // IDE task-file write with no drive: silently dropped.
        let _ = val;
    }

    /// Write a 16-bit word to a Gayle-decoded address.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        let local = addr & 0x000F_FFFF;
        if local & 0x000A_0000 != 0x000A_0000 {
            return;
        }
        if local & 0x8000 != 0 {
            self.write(addr, val as u8);
            return;
        }
        // IDE no-drive: drop the write.
        let _ = val;
    }

    /// IDE IRQ line. Always low with no drive attached.
    #[must_use]
    pub const fn ide_irq_pending(&self) -> bool {
        false
    }

    /// Borrow the current Card Status register value (debug / query).
    #[must_use]
    pub const fn card_status(&self) -> u8 {
        self.gayle_cs
    }

    /// Borrow the current IRQ register value (debug / query).
    #[must_use]
    pub const fn irq_register(&self) -> u8 {
        self.gayle_irq
    }

    /// Borrow the current Interrupt Enable register value (debug / query).
    #[must_use]
    pub const fn int_register(&self) -> u8 {
        self.gayle_int
    }

    /// Borrow the current Configuration register value (debug / query).
    #[must_use]
    pub const fn cfg_register(&self) -> u8 {
        self.gayle_cfg & 0x0F
    }

    /// Return a complete side-effect-free view of all implemented Gayle state.
    ///
    /// This reports the exact retained register values alongside decoded
    /// signals and the fixed no-drive/no-card interface behavior. No mutable
    /// latch is advanced and no backing media payload is read.
    #[must_use]
    pub const fn diagnostic_snapshot(&self) -> GayleDiagnosticSnapshot {
        let card_status = GayleCardStatusDiagnosticSnapshot {
            raw: self.gayle_cs,
            ide_interrupt_active: self.gayle_cs & GAYLE_CS_IDE != 0,
            card_detected: self.gayle_cs & GAYLE_CS_CCDET != 0,
            battery_voltage_detect_1: self.gayle_cs & GAYLE_CS_BVD1 != 0,
            battery_voltage_detect_2: self.gayle_cs & GAYLE_CS_BVD2 != 0,
            card_writable: self.gayle_cs & GAYLE_CS_WR != 0,
            card_interrupt_pending: self.gayle_cs & GAYLE_CS_BSY != 0,
            digital_audio_enabled: self.gayle_cs & GAYLE_CS_DAEN != 0,
            pcmcia_disabled: self.gayle_cs & GAYLE_CS_DIS != 0,
        };
        let enabled_request_mask = self.gayle_irq & self.gayle_int & GAYLE_INTERRUPT_SOURCE_MASK;
        let configuration = self.gayle_cfg & 0x0F;

        GayleDiagnosticSnapshot {
            registers: GayleRegisterDiagnosticSnapshot {
                card_status: self.gayle_cs,
                interrupt_request: self.gayle_irq,
                interrupt_enable: self.gayle_int,
                configuration,
            },
            card_status,
            interrupts: GayleInterruptDiagnosticSnapshot {
                request_register: self.gayle_irq,
                enable_register: self.gayle_int,
                requested_sources: GayleInterruptSourceDiagnosticSnapshot::from_register(
                    self.gayle_irq,
                ),
                enabled_sources: GayleInterruptSourceDiagnosticSnapshot::from_register(
                    self.gayle_int,
                ),
                enabled_request_mask,
                enabled_requested_sources: GayleInterruptSourceDiagnosticSnapshot::from_register(
                    enabled_request_mask,
                ),
                reset: self.gayle_irq & GAYLE_IRQ_RESET != 0,
                bus_error: self.gayle_irq & GAYLE_IRQ_BERR != 0,
                non_source_enable_bits: self.gayle_int & !GAYLE_INTERRUPT_SOURCE_MASK,
                ide_irq_line_pending: self.ide_irq_pending(),
            },
            configuration: GayleConfigurationDiagnosticSnapshot {
                raw: configuration,
                bit_0: configuration & 0x01 != 0,
                bit_1: configuration & 0x02 != 0,
                bit_2: configuration & 0x04 != 0,
                bit_3: configuration & 0x08 != 0,
            },
            ide: GayleIdeDiagnosticSnapshot {
                drive_attached: false,
                irq_line_pending: self.ide_irq_pending(),
                status_register: IDE_NO_DRIVE_STATUS,
                task_file_open_bus: IDE_NO_DRIVE_OPEN_BUS,
                data_register_open_bus: u16::MAX,
                media_size_bytes: None,
                media_position_bytes: None,
                writes_discarded: true,
            },
            pcmcia: GaylePcmciaDiagnosticSnapshot {
                backend_attached: false,
                reported_card_detected: card_status.card_detected,
                slot_disabled: card_status.pcmcia_disabled,
                interrupt_pending: card_status.card_interrupt_pending,
                card_writable: card_status.card_writable,
                digital_audio_enabled: card_status.digital_audio_enabled,
                battery_voltage_detect_1: card_status.battery_voltage_detect_1,
                battery_voltage_detect_2: card_status.battery_voltage_detect_2,
                common_memory_size_bytes: None,
                attribute_memory_size_bytes: None,
                io_space_size_bytes: None,
            },
            address_decode: GayleAddressDecodeDiagnosticSnapshot {
                local_address_mask: 0x000F_FFFF,
                required_address_bits: 0x000A_0000,
                control_register_select_bit: 0x8000,
                control_register_index_mask: 0x3000,
                ide_register_offset_mask: 0x3FFF,
                ide_status_offset: IDE_REG_STATUS,
            },
        }
    }
}

impl Default for Gayle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GAYLE_CS_BSY, GAYLE_CS_BVD1, GAYLE_CS_BVD2, GAYLE_CS_CCDET, GAYLE_CS_DAEN, GAYLE_CS_DIS,
        GAYLE_CS_IDE, GAYLE_CS_WR, Gayle, IDE_NO_DRIVE_OPEN_BUS, IDE_NO_DRIVE_STATUS,
    };

    #[test]
    fn new_starts_with_all_control_registers_zero() {
        let g = Gayle::new();
        assert_eq!(g.card_status(), 0);
        assert_eq!(g.irq_register(), 0);
        assert_eq!(g.int_register(), 0);
        assert_eq!(g.cfg_register(), 0);
    }

    #[test]
    fn ide_status_reads_seven_f_with_no_drive() {
        let g = Gayle::new();
        assert_eq!(g.read(0x00DA_001C), IDE_NO_DRIVE_STATUS);
    }

    #[test]
    fn other_ide_registers_read_open_bus_with_no_drive() {
        let g = Gayle::new();
        for reg in [0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18] {
            assert_eq!(g.read(0x00DA_0000 | reg), IDE_NO_DRIVE_OPEN_BUS);
        }
    }

    #[test]
    fn ide_word_reads_return_open_bus_with_no_drive() {
        let g = Gayle::new();
        assert_eq!(g.read_word(0x00DA_0000), 0xFFFF);
    }

    #[test]
    fn control_registers_round_trip_writes() {
        let mut g = Gayle::new();
        g.write(0x00DA_8000, 0xA5); // gayle_cs
        assert_eq!(g.read(0x00DA_8000), 0xA5);

        g.write(0x00DA_9000, 0x03); // gayle_irq RESET+BERR bits
        assert_eq!(g.read(0x00DA_9000), 0x03);

        g.write(0x00DA_A000, 0x80); // gayle_int (enable IDE IRQ)
        assert_eq!(g.read(0x00DA_A000), 0x80);

        g.write(0x00DA_B000, 0xFF); // gayle_cfg masks to nibble
        assert_eq!(g.read(0x00DA_B000), 0x0F);
    }

    #[test]
    fn addresses_outside_gayle_filter_read_zero() {
        let g = Gayle::new();
        // bits 17 + 19 must both be set; $D90000 has bit 19 but not bit 17.
        assert_eq!(g.read(0x00D9_0000), 0);
        // $D40000 has neither.
        assert_eq!(g.read(0x00D4_0000), 0);
    }

    #[test]
    fn writes_outside_gayle_filter_drop_silently() {
        let mut g = Gayle::new();
        g.write(0x00D9_0000, 0xAA);
        assert_eq!(g.card_status(), 0);
    }

    #[test]
    fn ide_irq_pending_is_always_low_with_no_drive() {
        let mut g = Gayle::new();
        g.write(0x00DA_A000, 0xFF); // enable everything
        assert!(!g.ide_irq_pending());
    }

    #[test]
    fn diagnostic_snapshot_is_complete_and_non_destructive() {
        let mut g = Gayle {
            gayle_cs: 0xFF,
            gayle_irq: 0xFF,
            gayle_int: 0xAD,
            gayle_cfg: 0x0B,
        };
        let before = [
            g.read(0x00DA_8000),
            g.read(0x00DA_9000),
            g.read(0x00DA_A000),
            g.read(0x00DA_B000),
            g.read(0x00DA_001C),
        ];

        let first = g.diagnostic_snapshot();
        let second = g.diagnostic_snapshot();

        assert_eq!(first, second);
        assert_eq!(
            first.registers.card_status,
            GAYLE_CS_IDE
                | GAYLE_CS_CCDET
                | GAYLE_CS_BVD1
                | GAYLE_CS_BVD2
                | GAYLE_CS_WR
                | GAYLE_CS_BSY
                | GAYLE_CS_DAEN
                | GAYLE_CS_DIS
        );
        assert_eq!(first.registers.interrupt_request, 0xFF);
        assert_eq!(first.registers.interrupt_enable, 0xAD);
        assert_eq!(first.registers.configuration, 0x0B);
        assert_eq!(
            before,
            [
                g.read(0x00DA_8000),
                g.read(0x00DA_9000),
                g.read(0x00DA_A000),
                g.read(0x00DA_B000),
                g.read(0x00DA_001C),
            ]
        );

        // Prove that calling the diagnostic API did not alter later writes.
        g.write(0x00DA_B000, 0x04);
        assert_eq!(g.cfg_register(), 0x04);
    }

    #[test]
    fn diagnostic_snapshot_decodes_all_stored_register_bits() {
        let g = Gayle {
            gayle_cs: 0xFF,
            gayle_irq: 0xFF,
            gayle_int: 0xAD,
            gayle_cfg: 0x0B,
        };

        let snapshot = g.diagnostic_snapshot();
        let status = snapshot.card_status;
        assert!(status.ide_interrupt_active);
        assert!(status.card_detected);
        assert!(status.battery_voltage_detect_1);
        assert!(status.battery_voltage_detect_2);
        assert!(status.card_writable);
        assert!(status.card_interrupt_pending);
        assert!(status.digital_audio_enabled);
        assert!(status.pcmcia_disabled);

        let interrupts = snapshot.interrupts;
        assert!(interrupts.requested_sources.ide);
        assert!(interrupts.requested_sources.ccdet);
        assert!(interrupts.requested_sources.bvd1);
        assert!(interrupts.requested_sources.bvd2);
        assert!(interrupts.requested_sources.wr);
        assert!(interrupts.requested_sources.bsy);
        assert!(interrupts.enabled_sources.ide);
        assert!(!interrupts.enabled_sources.ccdet);
        assert!(interrupts.enabled_sources.bvd1);
        assert!(!interrupts.enabled_sources.bvd2);
        assert!(interrupts.enabled_sources.wr);
        assert!(interrupts.enabled_sources.bsy);
        assert_eq!(interrupts.enabled_request_mask, 0xAC);
        assert!(interrupts.reset);
        assert!(interrupts.bus_error);
        assert_eq!(interrupts.non_source_enable_bits, 0x01);
        assert!(!interrupts.ide_irq_line_pending);

        let configuration = snapshot.configuration;
        assert!(configuration.bit_0);
        assert!(configuration.bit_1);
        assert!(!configuration.bit_2);
        assert!(configuration.bit_3);
    }

    #[test]
    fn diagnostic_snapshot_reports_fixed_absent_backend_behavior() {
        let mut g = Gayle::new();
        g.write(0x00DA_8000, GAYLE_CS_CCDET | GAYLE_CS_DIS);

        let snapshot = g.diagnostic_snapshot();
        assert!(!snapshot.ide.drive_attached);
        assert!(!snapshot.ide.irq_line_pending);
        assert_eq!(snapshot.ide.status_register, IDE_NO_DRIVE_STATUS);
        assert_eq!(snapshot.ide.task_file_open_bus, IDE_NO_DRIVE_OPEN_BUS);
        assert_eq!(snapshot.ide.data_register_open_bus, 0xFFFF);
        assert_eq!(snapshot.ide.media_size_bytes, None);
        assert_eq!(snapshot.ide.media_position_bytes, None);
        assert!(snapshot.ide.writes_discarded);

        assert!(!snapshot.pcmcia.backend_attached);
        assert!(snapshot.pcmcia.reported_card_detected);
        assert!(snapshot.pcmcia.slot_disabled);
        assert!(!snapshot.pcmcia.digital_audio_enabled);
        assert_eq!(snapshot.pcmcia.common_memory_size_bytes, None);
        assert_eq!(snapshot.pcmcia.attribute_memory_size_bytes, None);
        assert_eq!(snapshot.pcmcia.io_space_size_bytes, None);

        assert_eq!(snapshot.address_decode.local_address_mask, 0x000F_FFFF);
        assert_eq!(snapshot.address_decode.required_address_bits, 0x000A_0000);
        assert_eq!(snapshot.address_decode.ide_status_offset, 0x1C);
    }
}
