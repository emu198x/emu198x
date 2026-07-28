//! GVP A530 accelerator-board configuration and local RAM.
//!
//! This crate supplies the board-local state used by an Amiga machine's A530
//! profile:
//!
//! - the documented 1, 2, 4, or 8 MiB local-RAM configurations;
//! - the factory cache-enable and autoboot jumper states;
//! - one Zorro-II Autoconfig memory function; and
//! - a full 32-bit local-RAM response path for an MC68EC030 bus.
//!
//! The A530 manual identifies a 40 MHz MC68EC030, a shipped minimum of 1 MiB
//! local RAM, and the four supported RAM capacities. CPU construction,
//! ownership, clocking, cache behaviour, and the synchronized motherboard
//! bridge belong to the integrating machine rather than this board-state
//! crate.
//!
//! The memory-function identity `2017/9` comes from WinUAE and is therefore a
//! secondary-oracle compatibility fact, not a claim sourced from the A530
//! manual. The SCSI/controller function is deliberately absent: this crate
//! does not invent controller registers, firmware, media, or autoboot
//! behaviour.

#![forbid(unsafe_code)]

use commodore_amiga_autoconfig::{AutoconfigBoard, AutoconfigState};
use motorola_68k_common::bus::{
    DataPortSize, TransferSize, dynamic_transfer_bytes, extract_dynamic_bus_data,
    place_dynamic_read_data,
};
use serde::{Deserialize, Serialize};

/// WinUAE's secondary-oracle manufacturer identity for the GVP memory
/// function.
///
/// The A530 manual does not establish this value.
pub const GVP_MANUFACTURER_ID: u16 = 2017;

/// WinUAE's secondary-oracle product identity for the A530 memory function.
///
/// This is not the unimplemented SCSI/controller function's product ID.
pub const A530_MEMORY_PRODUCT_ID: u8 = 9;

/// The A530 local RAM is connected to a full 32-bit accelerator-local port.
pub const LOCAL_RAM_PORT: DataPortSize = DataPortSize::Long;

/// Supported A530 local-RAM capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum A530RamSize {
    /// 1 MiB, the minimum shipped configuration documented by the manual.
    Mib1,
    /// 2 MiB.
    Mib2,
    /// 4 MiB.
    Mib4,
    /// 8 MiB.
    Mib8,
}

impl A530RamSize {
    /// Parse a supported capacity expressed in MiB.
    #[must_use]
    pub const fn from_mib(mib: u8) -> Option<Self> {
        match mib {
            1 => Some(Self::Mib1),
            2 => Some(Self::Mib2),
            4 => Some(Self::Mib4),
            8 => Some(Self::Mib8),
            _ => None,
        }
    }

    /// Capacity in MiB.
    #[must_use]
    pub const fn mib(self) -> u8 {
        match self {
            Self::Mib1 => 1,
            Self::Mib2 => 2,
            Self::Mib4 => 4,
            Self::Mib8 => 8,
        }
    }

    /// Capacity in KiB, as required by the Zorro-II size encoding.
    #[must_use]
    pub const fn kib(self) -> u32 {
        self.mib() as u32 * 1024
    }

    /// Capacity in bytes.
    #[must_use]
    pub const fn bytes(self) -> u32 {
        self.kib() * 1024
    }
}

/// Static A530 board configuration.
///
/// `serial` is caller-supplied because no primary source in the current
/// evidence set establishes one canonical board serial number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct A530Config {
    ram_size: A530RamSize,
    serial: u32,
    cache_enabled: bool,
    autoboot_enabled: bool,
}

impl A530Config {
    /// Construct the documented factory-jumper configuration.
    ///
    /// Factory J3 enables the processor cache and factory J9 enables
    /// autoboot. The flags record the board configuration only; this crate
    /// does not implement either cache or SCSI autoboot behaviour.
    #[must_use]
    pub const fn new(ram_size: A530RamSize, serial: u32) -> Self {
        Self {
            ram_size,
            serial,
            cache_enabled: true,
            autoboot_enabled: true,
        }
    }

    /// Select whether the cache-enable jumper is enabled.
    #[must_use]
    pub const fn with_cache_enabled(mut self, enabled: bool) -> Self {
        self.cache_enabled = enabled;
        self
    }

    /// Select whether the autoboot jumper is enabled.
    #[must_use]
    pub const fn with_autoboot_enabled(mut self, enabled: bool) -> Self {
        self.autoboot_enabled = enabled;
        self
    }

    /// Installed local-RAM capacity.
    #[must_use]
    pub const fn ram_size(self) -> A530RamSize {
        self.ram_size
    }

    /// Autoconfig serial number for the modelled board instance.
    #[must_use]
    pub const fn serial(self) -> u32 {
        self.serial
    }

    /// Whether the cache-enable jumper is enabled.
    #[must_use]
    pub const fn cache_enabled(self) -> bool {
        self.cache_enabled
    }

    /// Whether the autoboot jumper is enabled.
    #[must_use]
    pub const fn autoboot_enabled(self) -> bool {
        self.autoboot_enabled
    }
}

/// Serde-persistable GVP A530 board-local state.
///
/// The contained Autoconfig board is only the memory function. The integrating
/// machine owns the processor and decides whether an access uses this local
/// 32-bit path or the synchronized 16-bit motherboard bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GvpA530 {
    config: A530Config,
    memory_function: AutoconfigBoard,
}

impl GvpA530 {
    /// Construct an A530 board with zero-filled local RAM.
    #[must_use]
    pub fn new(config: A530Config) -> Self {
        let memory_function = AutoconfigBoard::fast_ram_with_identity(
            config.ram_size().kib(),
            GVP_MANUFACTURER_ID,
            A530_MEMORY_PRODUCT_ID,
            config.serial(),
        );
        Self {
            config,
            memory_function,
        }
    }

    /// Static board configuration.
    #[must_use]
    pub const fn config(&self) -> A530Config {
        self.config
    }

    /// Whether the cache-enable jumper is enabled.
    ///
    /// This is configuration state only; no cache datapath lives here.
    #[must_use]
    pub const fn cache_enabled(&self) -> bool {
        self.config.cache_enabled()
    }

    /// Whether the autoboot jumper is enabled.
    ///
    /// This is configuration state only; no SCSI or boot-ROM behaviour lives
    /// here.
    #[must_use]
    pub const fn autoboot_enabled(&self) -> bool {
        self.config.autoboot_enabled()
    }

    /// Current state of the Zorro-II memory function.
    #[must_use]
    pub fn autoconfig_state(&self) -> AutoconfigState {
        self.memory_function.state()
    }

    /// Host-assigned base address of the memory function, when configured.
    #[must_use]
    pub fn mapped_base(&self) -> Option<u32> {
        self.memory_function.base()
    }

    /// Read one word from the Autoconfig probe window.
    #[must_use]
    pub fn read_autoconfig_word(&self, offset: u16) -> u16 {
        self.memory_function.read_word(offset)
    }

    /// Write one word to the Autoconfig probe window.
    pub fn write_autoconfig_word(&mut self, offset: u16, value: u16) {
        self.memory_function.write_word(offset, value);
    }

    /// Return the memory function to its power-on probe state without
    /// clearing local RAM.
    pub fn reset(&mut self) {
        self.memory_function.reset();
    }

    /// Local-RAM capacity in bytes.
    #[must_use]
    pub fn ram_size(&self) -> u32 {
        self.memory_function.ram_size()
    }

    /// Whether persisted board configuration, Autoconfig identity, size
    /// encoding, and local-RAM backing agree.
    ///
    /// Constructors maintain these invariants. Snapshot validators should
    /// reject state for which this returns `false`.
    #[must_use]
    pub fn configuration_is_coherent(&self) -> bool {
        self.memory_function.configuration_is_coherent()
            && self.memory_function.ram_size() == self.config.ram_size().bytes()
            && self.memory_function.manufacturer_id() == GVP_MANUFACTURER_ID
            && self.memory_function.product_id() == A530_MEMORY_PRODUCT_ID
            && self.memory_function.serial_number() == self.config.serial()
    }

    /// Direct board-local storage view, independent of Autoconfig mapping.
    #[must_use]
    pub fn storage(&self) -> &[u8] {
        self.memory_function.ram_bytes()
    }

    /// Mutable board-local storage view, independent of Autoconfig mapping.
    pub fn storage_mut(&mut self) -> &mut [u8] {
        self.memory_function.ram_bytes_mut()
    }

    /// Return whether an address lands in the configured memory window.
    #[must_use]
    pub fn contains_mapped_address(&self, address: u32) -> bool {
        self.memory_function.contains_ram_address(address)
    }

    /// Whether every byte in one dynamic-sized CPU phase lands in local RAM.
    #[must_use]
    pub fn contains_sized_access(&self, address: u32, remaining: TransferSize) -> bool {
        let transferred = dynamic_transfer_bytes(remaining, address, LOCAL_RAM_PORT);
        (0..transferred)
            .all(|offset| self.contains_mapped_address(address.wrapping_add(u32::from(offset))))
    }

    /// Read one mapped byte.
    #[must_use]
    pub fn read_mapped_byte(&self, address: u32) -> Option<u8> {
        self.memory_function.read_ram_byte(address)
    }

    /// Write one mapped byte, returning whether the memory function absorbed
    /// the access.
    #[must_use]
    pub fn write_mapped_byte(&mut self, address: u32, value: u8) -> bool {
        if !self.contains_mapped_address(address) {
            return false;
        }
        self.memory_function.write_ram_byte(address, value);
        true
    }

    /// Read one physical phase from the 32-bit local-RAM port.
    ///
    /// `remaining` is the MC68020/MC68030 SIZ value for the logical
    /// transfer. The returned value is placed on D31-D0 according to the
    /// address lanes and can be returned with [`LOCAL_RAM_PORT`] in a sized
    /// bus response.
    #[must_use]
    pub fn read_sized(&self, address: u32, remaining: TransferSize) -> Option<u32> {
        if !self.contains_sized_access(address, remaining) {
            return None;
        }
        let transferred = dynamic_transfer_bytes(remaining, address, LOCAL_RAM_PORT);
        let mut value = 0u32;
        for offset in 0..transferred {
            value = (value << 8)
                | u32::from(self.read_mapped_byte(address.wrapping_add(u32::from(offset)))?);
        }
        Some(place_dynamic_read_data(
            value,
            transferred,
            address,
            LOCAL_RAM_PORT,
        ))
    }

    /// Write one physical phase to the 32-bit local-RAM port.
    ///
    /// `data` is the physical D31-D0 bus image. Returns `false` if any byte
    /// in this phase lies outside the configured memory function; in that
    /// case no bytes are written.
    #[must_use]
    pub fn write_sized(&mut self, address: u32, remaining: TransferSize, data: u32) -> bool {
        if !self.contains_sized_access(address, remaining) {
            return false;
        }
        let transferred = dynamic_transfer_bytes(remaining, address, LOCAL_RAM_PORT);
        let value = extract_dynamic_bus_data(data, transferred, address, LOCAL_RAM_PORT);
        for offset in 0..transferred {
            let shift = u32::from(transferred - offset - 1) * 8;
            let byte = ((value >> shift) & 0xFF) as u8;
            let absorbed = self.write_mapped_byte(address.wrapping_add(u32::from(offset)), byte);
            debug_assert!(absorbed, "validated local-RAM byte must remain mapped");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIAL: u32 = 0x1234_5678;
    const BASE: u32 = 0x0020_0000;

    fn configured_board(size: A530RamSize) -> GvpA530 {
        let mut board = GvpA530::new(A530Config::new(size, SERIAL));
        board.write_autoconfig_word(0x4A, 0x0000);
        board.write_autoconfig_word(0x48, 0x2000);
        assert_eq!(board.mapped_base(), Some(BASE));
        board
    }

    fn read_probe_byte(board: &GvpA530, high_nibble_offset: u16) -> u8 {
        let high = ((board.read_autoconfig_word(high_nibble_offset) >> 12) & 0x0F) as u8;
        let low = ((board.read_autoconfig_word(high_nibble_offset + 2) >> 12) & 0x0F) as u8;
        (high << 4) | low
    }

    #[test]
    fn supported_ram_sizes_are_exact() {
        let cases = [
            (1, A530RamSize::Mib1, 1_048_576),
            (2, A530RamSize::Mib2, 2_097_152),
            (4, A530RamSize::Mib4, 4_194_304),
            (8, A530RamSize::Mib8, 8_388_608),
        ];
        for (mib, size, bytes) in cases {
            assert_eq!(A530RamSize::from_mib(mib), Some(size));
            assert_eq!(size.bytes(), bytes);
            assert_eq!(
                GvpA530::new(A530Config::new(size, SERIAL)).ram_size(),
                bytes
            );
            assert!(GvpA530::new(A530Config::new(size, SERIAL)).configuration_is_coherent());
        }
        assert_eq!(A530RamSize::from_mib(0), None);
        assert_eq!(A530RamSize::from_mib(3), None);
        assert_eq!(A530RamSize::from_mib(16), None);
    }

    #[test]
    fn factory_and_compatibility_jumper_states_are_explicit() {
        let factory = A530Config::new(A530RamSize::Mib1, SERIAL);
        assert!(factory.cache_enabled());
        assert!(factory.autoboot_enabled());

        let compatibility = factory
            .with_cache_enabled(false)
            .with_autoboot_enabled(false);
        let board = GvpA530::new(compatibility);
        assert!(!board.cache_enabled());
        assert!(!board.autoboot_enabled());
    }

    #[test]
    fn probe_exposes_secondary_memory_function_identity() {
        let board = GvpA530::new(A530Config::new(A530RamSize::Mib1, SERIAL));

        // All identity fields are bitwise inverted on the probe bus.
        assert_eq!(!read_probe_byte(&board, 0x04), A530_MEMORY_PRODUCT_ID);
        assert_eq!(
            !read_probe_byte(&board, 0x10),
            (GVP_MANUFACTURER_ID >> 8) as u8
        );
        assert_eq!(!read_probe_byte(&board, 0x14), GVP_MANUFACTURER_ID as u8);
        assert_eq!(!read_probe_byte(&board, 0x18), (SERIAL >> 24) as u8);
        assert_eq!(!read_probe_byte(&board, 0x24), SERIAL as u8);
    }

    #[test]
    fn coherence_check_rejects_backing_or_identity_mismatches() {
        let config = A530Config::new(A530RamSize::Mib1, SERIAL);
        let mut wrong_backing = GvpA530::new(config);
        wrong_backing.memory_function = AutoconfigBoard::fast_ram_with_identity(
            A530RamSize::Mib2.kib(),
            GVP_MANUFACTURER_ID,
            A530_MEMORY_PRODUCT_ID,
            SERIAL,
        );
        assert!(!wrong_backing.configuration_is_coherent());

        let mut wrong_identity = GvpA530::new(config);
        wrong_identity.memory_function = AutoconfigBoard::fast_ram_with_identity(
            A530RamSize::Mib1.kib(),
            GVP_MANUFACTURER_ID,
            A530_MEMORY_PRODUCT_ID.wrapping_add(1),
            SERIAL,
        );
        assert!(!wrong_identity.configuration_is_coherent());
    }

    #[test]
    fn autoconfig_mapping_controls_mapped_access() {
        let mut board = GvpA530::new(A530Config::new(A530RamSize::Mib1, SERIAL));
        assert_eq!(board.autoconfig_state(), AutoconfigState::Unconfigured);
        assert_eq!(board.read_mapped_byte(BASE), None);

        board.write_autoconfig_word(0x4A, 0x0000);
        board.write_autoconfig_word(0x48, 0x2000);

        assert_eq!(
            board.autoconfig_state(),
            AutoconfigState::Configured { base: BASE }
        );
        assert!(board.write_mapped_byte(BASE, 0xA5));
        assert_eq!(board.read_mapped_byte(BASE), Some(0xA5));
        assert!(!board.write_mapped_byte(BASE - 1, 0xFF));
        assert!(!board.write_mapped_byte(BASE + board.ram_size(), 0xFF));
    }

    #[test]
    fn sized_access_is_big_endian_on_the_32_bit_local_port() {
        let mut board = configured_board(A530RamSize::Mib1);

        assert!(board.write_sized(BASE, TransferSize::Long, 0x1234_5678));
        assert_eq!(&board.storage()[..4], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(
            board.read_sized(BASE, TransferSize::Long),
            Some(0x1234_5678)
        );
        assert_eq!(
            board.read_sized(BASE + 1, TransferSize::ThreeBytes),
            Some(0x0034_5678)
        );
        assert_eq!(
            board.read_sized(BASE + 2, TransferSize::Word),
            Some(0x0000_5678)
        );
    }

    #[test]
    fn storage_view_and_reset_preserve_ram_but_drop_mapping() {
        let mut board = configured_board(A530RamSize::Mib1);
        board.storage_mut()[0x20] = 0xA5;
        assert_eq!(board.read_mapped_byte(BASE + 0x20), Some(0xA5));

        board.reset();

        assert_eq!(board.autoconfig_state(), AutoconfigState::Unconfigured);
        assert_eq!(board.read_mapped_byte(BASE + 0x20), None);
        assert_eq!(board.storage()[0x20], 0xA5);
    }

    #[test]
    fn serde_roundtrip_preserves_config_mapping_and_ram() {
        let config = A530Config::new(A530RamSize::Mib1, SERIAL)
            .with_cache_enabled(false)
            .with_autoboot_enabled(false);
        let mut board = GvpA530::new(config);
        board.write_autoconfig_word(0x4A, 0x0000);
        board.write_autoconfig_word(0x48, 0x2000);
        assert!(board.write_mapped_byte(BASE + 7, 0x5A));

        let bytes = postcard::to_allocvec(&board).expect("serialize A530");
        let restored: GvpA530 = postcard::from_bytes(&bytes).expect("deserialize A530");

        assert_eq!(restored.config(), config);
        assert_eq!(restored.mapped_base(), Some(BASE));
        assert_eq!(restored.read_mapped_byte(BASE + 7), Some(0x5A));
        assert_eq!(restored.ram_size(), A530RamSize::Mib1.bytes());
    }
}
