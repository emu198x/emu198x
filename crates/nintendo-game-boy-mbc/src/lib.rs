//! Game Boy Memory Bank Controllers.
//!
//! A [`Cartridge`] owns the full ROM image + external RAM buffer and
//! dispatches reads / writes to the selected [`Mbc`] variant. Every
//! bank-switch command arrives as a write to the CPU-visible ROM
//! region ($0000–$7FFF); external RAM access flows through the
//! $A000–$BFFF window and is gated by each MBC's "RAM enable" state.
//!
//! Supported today: None (ROM only), MBC1, MBC2, MBC3, MBC5. MBC2's
//! on-chip 512×4-bit "RAM" and address-bit-8 enable / bank split are
//! the main deviations from MBC1.
//!
//! The cart-type byte lives at ROM offset `$0147`; the header parser
//! in [`format-nintendo-game-boy-cartridge`] (landing next) decodes
//! it to a [`CartType`] value and passes it plus the RAM-size byte
//! to [`Cartridge::new`].

mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use mbc1::Mbc1;
pub use mbc2::Mbc2;
pub use mbc3::{Mbc3, RtcRegisters};
pub use mbc5::Mbc5;

/// Cartridge-type byte at ROM `$0147`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CartType {
    /// `$00` (no RAM), `$08` (ROM+RAM), `$09` (ROM+RAM+BATTERY). No MBC;
    /// any RAM is wired directly. `$09` carries a battery so its RAM is
    /// persisted to the `.sav` sidecar.
    RomOnly { battery: bool },
    /// `$01..=$03`. Flags carry RAM / battery presence.
    Mbc1 { ram: bool, battery: bool },
    /// `$05..=$06`. MBC2 has internal 512×4-bit RAM.
    Mbc2 { battery: bool },
    /// `$0F..=$13`. RTC and battery are independent flags.
    Mbc3 { ram: bool, battery: bool, rtc: bool },
    /// `$19..=$1E`.
    Mbc5 {
        ram: bool,
        battery: bool,
        rumble: bool,
    },
}

impl CartType {
    /// Human-readable name used in logs / error messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RomOnly { .. } => "ROM only",
            Self::Mbc1 { .. } => "MBC1",
            Self::Mbc2 { .. } => "MBC2",
            Self::Mbc3 { .. } => "MBC3",
            Self::Mbc5 { .. } => "MBC5",
        }
    }

    /// Returns whether this cartridge type includes a battery-backed
    /// persistent component.
    #[must_use]
    pub const fn has_battery(self) -> bool {
        match self {
            Self::RomOnly { battery }
            | Self::Mbc1 { battery, .. }
            | Self::Mbc2 { battery }
            | Self::Mbc3 { battery, .. }
            | Self::Mbc5 { battery, .. } => battery,
        }
    }
}

/// Active MBC state. Each variant holds its own bank / enable /
/// mode registers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Mbc {
    None,
    Mbc1(Mbc1),
    Mbc2(Mbc2),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
}

/// A loaded cartridge: ROM image, external RAM, and the MBC that
/// decodes them. Exposes the same four operations every bus-side
/// consumer needs:
///
/// - [`read_rom`](Self::read_rom) for `$0000..=$7FFF`
/// - [`write_rom`](Self::write_rom) for `$0000..=$7FFF` (bank-switch
///   commands; no ROM byte ever changes)
/// - [`read_ram`](Self::read_ram) for `$A000..=$BFFF`
/// - [`write_ram`](Self::write_ram) for `$A000..=$BFFF`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    mbc: Mbc,
    cart_type: CartType,
}

impl Cartridge {
    /// Build a cartridge from a ROM image, the chosen MBC kind, and
    /// the target external-RAM size (bytes). `ram_size` is
    /// whatever the header's RAM-size byte decoded to; pass `0` for
    /// ROM-only / no-RAM carts.
    #[must_use]
    pub fn new(rom: Vec<u8>, cart_type: CartType, ram_size: usize) -> Self {
        let ram_size = if matches!(cart_type, CartType::Mbc2 { .. }) {
            Mbc2::RAM_SIZE
        } else {
            ram_size
        };
        let ram = vec![0xFF; ram_size];
        let mbc = match cart_type {
            CartType::RomOnly { .. } => Mbc::None,
            CartType::Mbc1 { .. } => Mbc::Mbc1(Mbc1::new_for_rom(&rom)),
            CartType::Mbc2 { .. } => Mbc::Mbc2(Mbc2::new()),
            CartType::Mbc3 { rtc, .. } => Mbc::Mbc3(Mbc3::new(rtc)),
            CartType::Mbc5 { .. } => Mbc::Mbc5(Mbc5::new()),
        };
        Self {
            rom,
            ram,
            mbc,
            cart_type,
        }
    }

    /// Cartridge type as decoded from the header.
    #[must_use]
    pub const fn cart_type(&self) -> CartType {
        self.cart_type
    }

    /// Returns whether this cartridge has battery-backed external RAM.
    #[must_use]
    pub fn has_battery_backed_ram(&self) -> bool {
        self.cart_type.has_battery() && !self.ram.is_empty()
    }

    /// ROM image (for the machine's direct reads, e.g. the header
    /// during boot).
    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// External RAM image (for save-state loading / persistence).
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Mutable external RAM (for save-state restore).
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// Read from the CPU's `$0000..=$7FFF` ROM window.
    pub fn read_rom(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::None => self.rom.get(usize::from(addr)).copied().unwrap_or(0xFF),
            Mbc::Mbc1(m) => m.read_rom(&self.rom, addr),
            Mbc::Mbc2(m) => m.read_rom(&self.rom, addr),
            Mbc::Mbc3(m) => m.read_rom(&self.rom, addr),
            Mbc::Mbc5(m) => m.read_rom(&self.rom, addr),
        }
    }

    /// Writes to the CPU's `$0000..=$7FFF` range are bank-switch
    /// commands — no actual ROM byte ever changes.
    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::None => {}
            Mbc::Mbc1(m) => m.write_rom(addr, value),
            Mbc::Mbc2(m) => m.write_rom(addr, value),
            Mbc::Mbc3(m) => m.write_rom(addr, value),
            Mbc::Mbc5(m) => m.write_rom(addr, value),
        }
    }

    /// Read from the CPU's `$A000..=$BFFF` external RAM window.
    /// `$FF` is returned when RAM isn't enabled or the cartridge
    /// carries no RAM at all.
    pub fn read_ram(&self, addr: u16) -> u8 {
        match &self.mbc {
            Mbc::None => self.ram_linear_read(addr),
            Mbc::Mbc1(m) => m.read_ram(&self.ram, addr),
            Mbc::Mbc2(m) => m.read_ram(&self.ram, addr),
            Mbc::Mbc3(m) => m.read_ram(&self.ram, addr),
            Mbc::Mbc5(m) => m.read_ram(&self.ram, addr),
        }
    }

    /// Write to the CPU's `$A000..=$BFFF` external RAM window.
    pub fn write_ram(&mut self, addr: u16, value: u8) {
        match &mut self.mbc {
            Mbc::None => self.ram_linear_write(addr, value),
            Mbc::Mbc1(m) => m.write_ram(&mut self.ram, addr, value),
            Mbc::Mbc2(m) => m.write_ram(&mut self.ram, addr, value),
            Mbc::Mbc3(m) => m.write_ram(&mut self.ram, addr, value),
            Mbc::Mbc5(m) => m.write_ram(&mut self.ram, addr, value),
        }
    }

    /// Advance the cartridge's real-time clock by `elapsed` real seconds.
    /// Only MBC3-with-RTC carts have one; every other cartridge ignores it.
    pub fn advance_rtc(&mut self, elapsed: u64) {
        if let Mbc::Mbc3(m) = &mut self.mbc {
            m.advance_seconds(elapsed);
        }
    }

    /// Whether this cartridge carries a real-time clock (MBC3 + RTC).
    #[must_use]
    pub fn has_rtc(&self) -> bool {
        matches!(self.cart_type, CartType::Mbc3 { rtc: true, .. })
    }

    fn ram_linear_read(&self, addr: u16) -> u8 {
        let offset = usize::from(addr.wrapping_sub(0xA000));
        self.ram.get(offset).copied().unwrap_or(0xFF)
    }

    fn ram_linear_write(&mut self, addr: u16, value: u8) {
        let offset = usize::from(addr.wrapping_sub(0xA000));
        if let Some(slot) = self.ram.get_mut(offset) {
            *slot = value;
        }
    }
}
