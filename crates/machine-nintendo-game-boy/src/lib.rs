//! Nintendo Game Boy machine: the DMG and its relatives, and the Game
//! Boy Color.
//!
//! Composes the SM83 CPU, PPU, APU, timer, and cartridge into a
//! single tickable machine. The bus dispatch covers the full memory
//! map; per-m-cycle orchestration drives the timer / APU /
//! PPU at T-cycle rate, the CPU at m-cycle rate, and OR's all IRQ
//! sources into `IF` for the CPU's interrupt-dispatch path.
//!
//! A Game Boy Color ([`BootProfile::Cgb`], [`BootProfile::Agb`]) adds
//! the [`cgb`] registers: VRAM and WRAM banks, colour palettes, HDMA
//! and double speed. At double speed the CPU and timer run twice as
//! fast as the PPU and APU, so an m-cycle is 2 dots instead of 4.
//!
//! Optional monochrome/CGB firmware runs through [`GameBoy::try_new_with_boot_rom`].
//! Without firmware the documented skipped-boot profile remains the default.
//!
//! Deferred behaviour (will land alongside the relevant test ROMs):
//!
//! - Full OAM DMA bus blocking. Real hardware blocks all CPU access to
//!   non-HRAM memory for the 160 m-cycles a DMA takes; the DMA transfer
//!   itself is paced and OAM access is blocked, but full CPU bus gating
//!   is still deferred.

/// The memory bank controller a header's cartridge type names.
fn board(kind: CartridgeType) -> CartType {
    match kind {
        CartridgeType::RomOnly { battery } => CartType::RomOnly { battery },
        CartridgeType::Mbc1 { ram, battery } => CartType::Mbc1 { ram, battery },
        CartridgeType::Mbc2 { battery } => CartType::Mbc2 { battery },
        CartridgeType::Mbc3 { ram, battery, rtc } => CartType::Mbc3 { ram, battery, rtc },
        CartridgeType::Mbc5 {
            ram,
            battery,
            rumble,
        } => CartType::Mbc5 {
            ram,
            battery,
            rumble,
        },
    }
}

pub mod cgb;
mod snapshot_v3;
#[doc(hidden)]
pub use snapshot_v3::GameBoyV3;

/// Size of a DMG0, DMG or MGB boot ROM.
pub const MONO_BOOT_ROM_SIZE: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
struct MonoBootRom {
    bytes: Vec<u8>,
    mapped: bool,
}
#[cfg(test)]
mod cgb_tests;
#[cfg(test)]
mod mono_boot_tests;
#[cfg(test)]
mod tests;

use common_nintendo_game_boy::{JoypadButton, JoypadMatrix, MemoryBus};
use emu198x_nintendo_game_boy_apu::Apu;
use emu198x_nintendo_game_boy_mbc::{CartType, Cartridge};
use emu198x_nintendo_game_boy_ppu::Ppu;
use emu198x_nintendo_game_boy_timer::Timer;
use emu198x_sharp_lr35902::{PostBootCpuState, Sm83};
use format_nintendo_game_boy_cartridge::{CartridgeHeader, CartridgeType, CgbSupport, HeaderError};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::cgb::{COMPATIBILITY_BG_PALETTE, COMPATIBILITY_OBJ_PALETTE, CgbState};

pub use emu198x_nintendo_game_boy_apu::{ApuChannel, AudioControls};

/// Eight 4 KiB WRAM banks; a DMG uses the first two.
const WRAM_SIZE: usize = 0x8000;
/// Two 8 KiB VRAM banks; a DMG uses the first.
const VRAM_SIZE: usize = 0x4000;
const OAM_SIZE: usize = 0xA0;
const HRAM_SIZE: usize = 0x7F;

const IF_VBLANK: u8 = 0x01;
const IF_STAT: u8 = 0x02;
const IF_TIMER: u8 = 0x04;
const IF_SERIAL: u8 = 0x08;
const IF_JOYPAD: u8 = 0x10;

/// Boot-ROM exit profile used when starting a cartridge directly at
/// `$0100`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootProfile {
    /// Original DMG0 boot ROM.
    Dmg0,
    /// DMG boot ROM v1 and later.
    DmgAbc,
    /// Game Boy Pocket.
    Mgb,
    /// Super Game Boy.
    Sgb,
    /// Super Game Boy 2.
    Sgb2,
    /// Game Boy Color.
    Cgb,
    /// Game Boy Advance running Game Boy Color software.
    Agb,
}

impl BootProfile {
    /// Whether this is colour hardware.
    #[must_use]
    pub const fn is_cgb(self) -> bool {
        matches!(self, Self::Cgb | Self::Agb)
    }

    /// The registers the boot ROM leaves. A CGB's depend on whether the
    /// cartridge put it in CGB mode.
    const fn cpu_state(self, cgb_mode: bool) -> PostBootCpuState {
        match self {
            Self::Dmg0 => PostBootCpuState::DMG0,
            Self::DmgAbc => PostBootCpuState::DMG_ABC,
            Self::Mgb => PostBootCpuState::MGB,
            Self::Sgb => PostBootCpuState::SGB,
            Self::Sgb2 => PostBootCpuState::SGB2,
            Self::Cgb if cgb_mode => PostBootCpuState::CGB,
            Self::Cgb => PostBootCpuState::CGB_DMG,
            Self::Agb if cgb_mode => PostBootCpuState::AGB,
            Self::Agb => PostBootCpuState::AGB_DMG,
        }
    }

    /// The DIV counter the boot ROM leaves. The CGB's is not documented,
    /// so a CGB starts from 0.
    const fn div_counter(self) -> u16 {
        match self {
            Self::Dmg0 => 0x182C,
            Self::DmgAbc | Self::Mgb => 0xABC9,
            Self::Sgb => 0xD85C,
            Self::Sgb2 => 0xD84C,
            Self::Cgb | Self::Agb => 0,
        }
    }

    const fn ch1_envelope(self) -> u8 {
        0xF3
    }

    const fn ch1_sweep(self) -> u8 {
        0x80
    }

    const fn ch1_enabled(self) -> bool {
        match self {
            Self::Sgb | Self::Sgb2 => false,
            Self::Dmg0 | Self::DmgAbc | Self::Mgb | Self::Cgb | Self::Agb => true,
        }
    }

    const fn joypad_select(self) -> u8 {
        match self {
            Self::Sgb | Self::Sgb2 => 0xF0,
            Self::Dmg0 | Self::DmgAbc | Self::Mgb | Self::Cgb | Self::Agb => 0xC0,
        }
    }

    const fn ppu_phase(self) -> (u8, u16) {
        match self {
            Self::Dmg0 => (145, 84),
            Self::DmgAbc | Self::Mgb | Self::Sgb | Self::Sgb2 | Self::Cgb | Self::Agb => (0, 0),
        }
    }
}

/// A loaded DMG.
#[derive(Clone, Serialize, Deserialize)]
pub struct GameBoy {
    cpu: Sm83,
    timer: Timer,
    ppu: Ppu,
    apu: Apu,
    cartridge: Cartridge,

    /// `WRAM_SIZE` bytes, on the heap: a CGB's 48 KiB of RAM would
    /// otherwise make the machine too large to move on a thread's stack.
    wram: Vec<u8>,
    /// `VRAM_SIZE` bytes.
    vram: Vec<u8>,
    #[serde(with = "BigArray")]
    oam: [u8; OAM_SIZE],
    #[serde(with = "BigArray")]
    hram: [u8; HRAM_SIZE],

    if_reg: u8,
    ie_reg: u8,

    joypad: JoypadMatrix,
    joypad_line_prev: bool,

    serial_data: u8,
    serial_control: u8,
    #[serde(default)]
    serial_irq_active: bool,
    #[serde(default)]
    serial_irq_bits_remaining: u8,
    /// Bytes "transmitted" via `SC = $81`. Drained by tests / the
    /// runtime layer to surface Blargg-style reporting.
    #[serde(skip)]
    serial_output: Vec<u8>,

    #[serde(default = "default_oam_dma_reg")]
    oam_dma_reg: u8,
    #[serde(default = "default_oam_dma_reg")]
    oam_dma_source_high: u8,
    #[serde(default = "default_oam_dma_index")]
    oam_dma_index: u8,
    #[serde(default)]
    oam_dma_start_delay: u8,
    #[serde(default = "default_oam_dma_reg")]
    oam_dma_pending_source_high: u8,
    #[serde(default)]
    oam_dma_pending_start_delay: u8,
    /// The last byte the OAM DMA transferred — the value the CPU reads from a
    /// bus-conflicting address while the DMA is running.
    #[serde(default = "default_oam_dma_reg")]
    oam_dma_last_byte: u8,

    /// The Game Boy Color's own state; `None` on monochrome models.
    #[serde(default)]
    cgb: Option<Box<CgbState>>,
    /// A monochrome model in STOP mode.
    #[serde(default)]
    dmg_stop_mode: bool,
    // Runtime v4 owns this new field; v3 reads use the explicit legacy layout.
    mono_boot: Option<MonoBootRom>,
}

/// Which of the DMG's two memory buses an address sits on. OAM DMA blocks
/// CPU access only to the bus it is currently reading from; the other bus,
/// HRAM and the IO/IE registers stay accessible.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemBus {
    /// ROM, cartridge RAM, WRAM (+ echo).
    External,
    /// VRAM.
    Video,
    /// OAM, IO, HRAM, IE — never conflicts with OAM DMA.
    None,
}

const fn mem_bus(addr: u16) -> MemBus {
    match addr {
        0x0000..=0x7FFF | 0xA000..=0xFDFF => MemBus::External,
        0x8000..=0x9FFF => MemBus::Video,
        _ => MemBus::None,
    }
}

/// The ® glyph the DMG, MGB and SGB boot ROMs place in tile `$19`,
/// one bitplane, top row first. The DMG0 boot ROM has no ® (Pan Docs,
/// "Power Up Sequence"). Glyph as in SameBoy's MIT-licensed boot ROM
/// (`emulators/gameboy/SameBoy/BootROMs/dmg_boot.asm`,
/// `TrademarkSymbol`).
const TRADEMARK_TILE: [u8; 8] = [0x3C, 0x42, 0xB9, 0xA5, 0xB9, 0xA5, 0x42, 0x3C];

/// VRAM as the boot ROM leaves it. The monochrome boot ROMs unpack the
/// cartridge header logo (`$0104-$0133`) into tiles 1-24, each nibble
/// doubled to a byte and written to two rows of bitplane 0; add the ®
/// in tile 25; and lay the logo out in the `$9800` map at rows 8-9
/// (Pan Docs, "Power Up Sequence"; SameBoy `dmg_boot.asm`,
/// `.loadLogoLoop` / `.tilemapLoop`). Games and test ROMs that skip
/// clearing VRAM rely on this — the Mealybug Tearoom PPU tests draw
/// tile `$19` as their sprite. The DMG0 boot ROM's exact VRAM is not
/// documented, so that profile keeps VRAM clear, and so do the colour
/// models, whose boot ROM draws its own logo.
fn post_boot_vram(cartridge: &Cartridge, boot_profile: BootProfile) -> Vec<u8> {
    let mut vram = vec![0; VRAM_SIZE];
    if boot_profile == BootProfile::Dmg0 || boot_profile.is_cgb() {
        return vram;
    }
    let mut row = 0x0010;
    for addr in 0x0104..0x0134u16 {
        let byte = cartridge.read_rom(addr);
        for nibble in [byte >> 4, byte & 0x0F] {
            let mut doubled = 0u8;
            for bit in (0..4).rev() {
                let set = (nibble >> bit) & 1;
                doubled = (doubled << 2) | (set << 1) | set;
            }
            vram[row] = doubled;
            vram[row + 2] = doubled;
            row += 4;
        }
    }
    for (i, &glyph_row) in TRADEMARK_TILE.iter().enumerate() {
        vram[row + i * 2] = glyph_row;
    }
    // Map rows 8 and 9, columns 4-15: tiles 1-12 and 13-24; the ® at
    // row 8, column 16.
    for column in 0..12u8 {
        vram[0x1904 + usize::from(column)] = column + 1;
        vram[0x1924 + usize::from(column)] = column + 13;
    }
    vram[0x1910] = 0x19;
    vram
}

const fn default_oam_dma_reg() -> u8 {
    0xFF
}

const fn default_oam_dma_index() -> u8 {
    OAM_SIZE as u8
}

impl GameBoy {
    /// Build a Game Boy around the given parsed cartridge.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self::new_with_boot_profile(cartridge, BootProfile::DmgAbc)
    }

    /// Build a Game Boy around the given parsed cartridge with a
    /// specific skipped-boot ROM exit profile.
    #[must_use]
    pub fn new_with_boot_profile(cartridge: Cartridge, boot_profile: BootProfile) -> Self {
        let cgb_mode = boot_profile.is_cgb()
            && CgbSupport::from_flag(cartridge.read_rom(0x0143)).uses_cgb_mode();
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom_with_state(boot_profile.cpu_state(cgb_mode));

        let (ppu_ly, ppu_dot) = boot_profile.ppu_phase();
        let vram = post_boot_vram(&cartridge, boot_profile);
        let mut ppu = Ppu::new_post_bootrom_with_phase(ppu_ly, ppu_dot);
        let cgb = boot_profile.is_cgb().then(|| {
            ppu.set_cgb(cgb_mode);
            if !cgb_mode {
                ppu.load_compatibility_palettes(
                    COMPATIBILITY_BG_PALETTE,
                    COMPATIBILITY_OBJ_PALETTE,
                    COMPATIBILITY_OBJ_PALETTE,
                );
            }
            Box::new(CgbState::new(cgb_mode, None))
        });

        Self {
            cpu,
            timer: Timer::new_post_bootrom_with_counter(boot_profile.div_counter()),
            ppu,
            apu: Apu::new_post_bootrom_with_ch1_regs(
                boot_profile.ch1_sweep(),
                boot_profile.ch1_envelope(),
                boot_profile.ch1_enabled(),
            ),
            cartridge,
            wram: vec![0; WRAM_SIZE],
            vram,
            oam: [0; OAM_SIZE],
            hram: [0; HRAM_SIZE],
            if_reg: IF_VBLANK,
            ie_reg: 0,
            joypad: JoypadMatrix::new_post_bootrom_with_select(boot_profile.joypad_select()),
            joypad_line_prev: false,
            serial_data: 0,
            serial_control: 0,
            serial_irq_active: false,
            serial_irq_bits_remaining: 0,
            serial_output: Vec::new(),
            oam_dma_reg: 0xFF,
            oam_dma_source_high: 0xFF,
            oam_dma_index: OAM_SIZE as u8,
            oam_dma_start_delay: 0,
            oam_dma_pending_source_high: 0xFF,
            oam_dma_pending_start_delay: 0,
            oam_dma_last_byte: 0xFF,
            cgb,
            dmg_stop_mode: false,
            mono_boot: None,
        }
    }

    /// Build a Game Boy Color that runs its boot ROM from `$0000`. The
    /// boot ROM decides CGB or compatibility mode, sets the palettes,
    /// and hands over at `$0100`.
    ///
    /// `boot_profile` should be [`BootProfile::Cgb`] or
    /// [`BootProfile::Agb`]; a monochrome profile is treated as `Cgb`.
    /// The boot ROM is user-supplied firmware, never bundled.
    #[must_use]
    pub fn new_cgb_with_boot_rom(
        cartridge: Cartridge,
        boot_profile: BootProfile,
        boot_rom: Vec<u8>,
    ) -> Self {
        let boot_profile = if boot_profile.is_cgb() {
            boot_profile
        } else {
            BootProfile::Cgb
        };
        let mut machine = Self::new_with_boot_profile(cartridge, boot_profile);
        machine.cpu.reset();
        machine.timer = Timer::new();
        machine.apu = Apu::new();
        machine.joypad = JoypadMatrix::new();
        machine.if_reg = 0;
        machine.ppu = Ppu::new();
        machine.ppu.write_lcdc(0);
        // The boot ROM runs in CGB mode until it writes KEY0.
        machine.ppu.set_cgb(true);
        machine.cgb = Some(Box::new(CgbState::new(true, Some(boot_rom))));
        machine
    }

    /// Convenience: parse a ROM image and build the Game Boy in one
    /// step. Returns the decoded header alongside the machine for
    /// the runtime layer's metadata.
    ///
    /// # Errors
    ///
    /// Forwards any [`HeaderError`] from the cartridge format crate.
    pub fn from_rom(rom: Vec<u8>) -> Result<(CartridgeHeader, Self), HeaderError> {
        Self::from_rom_with_boot_profile(rom, BootProfile::DmgAbc)
    }

    /// Convenience: parse a ROM image and build the Game Boy with a
    /// specific skipped-boot ROM exit profile.
    ///
    /// # Errors
    ///
    /// Forwards any [`HeaderError`] from the cartridge format crate.
    pub fn from_rom_with_boot_profile(
        rom: Vec<u8>,
        boot_profile: BootProfile,
    ) -> Result<(CartridgeHeader, Self), HeaderError> {
        let header = CartridgeHeader::parse(&rom)?;
        let cart = Cartridge::new(rom, board(header.cart_type), header.ram_size);
        Ok((header, Self::new_with_boot_profile(cart, boot_profile)))
    }

    /// Build a machine that runs `boot_rom` when it is given and the
    /// profile is a colour one; otherwise as
    /// [`GameBoy::new_with_boot_profile`].
    #[must_use]
    pub fn new_with_boot_rom(
        cartridge: Cartridge,
        boot_profile: BootProfile,
        boot_rom: Option<Vec<u8>>,
    ) -> Self {
        match boot_rom {
            Some(boot_rom) if boot_profile.is_cgb() => {
                Self::new_cgb_with_boot_rom(cartridge, boot_profile, boot_rom)
            }
            _ => Self::new_with_boot_profile(cartridge, boot_profile),
        }
    }

    /// Convenience: parse a ROM image and build the machine as
    /// [`GameBoy::new_with_boot_rom`] does.
    ///
    /// # Errors
    ///
    /// Forwards any [`HeaderError`] from the cartridge format crate.
    pub fn from_rom_with_boot_rom(
        rom: Vec<u8>,
        boot_profile: BootProfile,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<(CartridgeHeader, Self), HeaderError> {
        let header = CartridgeHeader::parse(&rom)?;
        let cart = Cartridge::new(rom, board(header.cart_type), header.ram_size);
        Ok((
            header,
            Self::new_with_boot_rom(cart, boot_profile, boot_rom),
        ))
    }

    /// Build a machine with validated optional firmware for its selected hardware.
    /// The older infallible constructor retains its CGB-only compatibility policy.
    ///
    /// # Errors
    /// Rejects wrong firmware extents and unsupported SGB boot execution.
    pub fn try_new_with_boot_rom(
        cartridge: Cartridge,
        boot_profile: BootProfile,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let Some(bytes) = boot_rom else {
            return Ok(Self::new_with_boot_profile(cartridge, boot_profile));
        };
        let expected = if boot_profile.is_cgb() {
            cgb::CGB_BOOT_ROM_SIZE
        } else {
            MONO_BOOT_ROM_SIZE
        };
        if bytes.len() != expected {
            return Err(format!(
                "expected {expected} boot ROM bytes, found {}",
                bytes.len()
            ));
        }
        if matches!(boot_profile, BootProfile::Sgb | BootProfile::Sgb2) {
            return Err("SGB boot-ROM execution is not implemented".into());
        }
        if boot_profile.is_cgb() {
            return Ok(Self::new_cgb_with_boot_rom(cartridge, boot_profile, bytes));
        }
        let mut machine = Self::new_with_boot_profile(cartridge, boot_profile);
        machine.cpu.reset();
        machine.timer = Timer::new();
        machine.apu = Apu::new();
        machine.joypad = JoypadMatrix::new();
        machine.if_reg = 0;
        machine.ppu = Ppu::new();
        machine.ppu.write_lcdc(0);
        machine.vram.fill(0);
        machine.mono_boot = Some(MonoBootRom {
            bytes,
            mapped: true,
        });
        Ok(machine)
    }

    /// Parse a cartridge and execute validated optional boot firmware.
    ///
    /// # Errors
    /// Returns cartridge-header or boot-firmware validation failures.
    pub fn try_from_rom_with_boot_rom(
        rom: Vec<u8>,
        boot_profile: BootProfile,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<(CartridgeHeader, Self), String> {
        let header = CartridgeHeader::parse(&rom).map_err(|error| error.to_string())?;
        let cart = Cartridge::new(rom, board(header.cart_type), header.ram_size);
        let machine = Self::try_new_with_boot_rom(cart, boot_profile, boot_rom)?;
        Ok((header, machine))
    }

    /// Supplied firmware, retained after unmapping for cold-reset/snapshot use.
    #[must_use]
    pub fn boot_rom(&self) -> Option<&[u8]> {
        self.mono_boot
            .as_ref()
            .map(|boot| boot.bytes.as_slice())
            .or_else(|| self.cgb.as_ref().and_then(|cgb| cgb.boot_rom.as_deref()))
    }

    /// Validate boot state and selected hardware during runtime restore.
    ///
    /// # Errors
    /// Rejects malformed monochrome firmware or a monochrome overlay on other hardware.
    pub fn validate_boot_state(&self, profile: BootProfile) -> Result<(), String> {
        if self.cgb.is_some() != profile.is_cgb() {
            return Err("snapshot hardware does not match the selected boot profile".into());
        }
        if let Some(boot) = &self.mono_boot
            && (!matches!(
                profile,
                BootProfile::Dmg0 | BootProfile::DmgAbc | BootProfile::Mgb
            ) || self.cgb.is_some()
                || boot.bytes.len() != MONO_BOOT_ROM_SIZE)
        {
            return Err("invalid monochrome boot state".into());
        }
        Ok(())
    }

    /// Returns the framebuffer (160 × 144 post-palette 2-bit shades).
    /// On a CGB in CGB mode these are colour indices, not shades; use
    /// [`GameBoy::color_framebuffer`].
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        self.ppu.framebuffer()
    }

    /// A CGB's picture as 160 × 144 RGB555 pixels; `None` on a
    /// monochrome model.
    #[must_use]
    pub fn color_framebuffer(&self) -> Option<&[u16]> {
        self.ppu.color_framebuffer()
    }

    /// Whether this is a Game Boy Color.
    #[must_use]
    pub fn is_cgb(&self) -> bool {
        self.cgb.is_some()
    }

    /// Whether a Game Boy Color is in CGB mode (not DMG compatibility).
    #[must_use]
    pub fn cgb_mode(&self) -> bool {
        self.cgb.as_ref().is_some_and(|cgb| cgb.cgb_mode)
    }

    /// Whether a Game Boy Color's CPU is at double speed.
    #[must_use]
    pub fn double_speed(&self) -> bool {
        self.cgb.as_ref().is_some_and(|cgb| cgb.double_speed)
    }

    /// Current CPU program counter.
    #[must_use]
    pub const fn cpu_pc(&self) -> u16 {
        self.cpu.pc
    }

    /// Drain APU samples (stereo interleaved `f32`).
    pub fn drain_audio(&mut self, dest: &mut [f32]) -> usize {
        self.apu.drain_samples(dest)
    }

    /// Returns the host-side APU mixer controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.apu.audio_controls()
    }

    /// Replaces the host-side APU mixer controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.apu.set_audio_controls(controls);
    }

    /// Enables or mutes one APU channel in the host output mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.apu.set_channel_enabled(channel, enabled);
    }

    /// Sets one APU channel's host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.apu.set_channel_gain(channel, gain);
    }

    /// Drain bytes "transmitted" via the serial port (`SC = $81`).
    /// Blargg's CPU test ROMs use this to report progress.
    pub fn drain_serial(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.serial_output)
    }

    /// Set the pressed state of one joypad button.
    pub fn set_button(&mut self, button: JoypadButton, pressed: bool) {
        self.joypad.set(button, pressed);
    }

    /// Returns the parsed cartridge metadata via the underlying
    /// cartridge.
    #[must_use]
    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// Mutable access to the parsed cartridge state.
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cartridge
    }

    /// The CPU register state, for the debug surface.
    #[must_use]
    pub fn cpu(&self) -> &Sm83 {
        &self.cpu
    }

    /// Read a byte with no side effects (debug peek).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.bus_read(addr)
    }

    /// Write a byte directly into the address space (debug poke).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.bus_write(addr, value);
    }

    /// Run the CPU forward by one full instruction, returning the number of
    /// T-cycles it took. Steps whole m-cycles until the CPU is back at an
    /// instruction boundary (`m_cycle == 0`).
    pub fn step_instruction(&mut self) -> u64 {
        let mut t_cycles = 0u64;
        loop {
            self.step_m_cycle();
            t_cycles += 4;
            if self.cpu.instruction_complete() {
                break;
            }
        }
        t_cycles
    }

    /// One CPU m-cycle (4 T-cycles): tick the per-T-cycle components,
    /// collect IRQ sources, service the CPU's bus pins, then tick
    /// the CPU.
    ///
    /// At CGB double speed the timer still sees 4 T-cycles, but the
    /// PPU and APU see only 2 dots; the APU reads DIV one bit higher so
    /// its frame sequencer keeps its real-time rate (Pan Docs, "CGB
    /// Registers: KEY1"). While HDMA copies, the CPU is held and its
    /// pending access waits.
    pub fn step_m_cycle(&mut self) {
        if self.in_stop_mode() {
            self.tick_stop_mode();
            return;
        }
        let hdma_stall = self.tick_hdma();
        // The CPU's bus pins already hold this M-cycle's access. Some
        // PPU registers take a write part-way through the M-cycle, so
        // the PPU is handed those before its dots run.
        let ppu_took_write = !hdma_stall
            && self.cpu.mreq
            && self.cpu.wr
            && !self.cpu.rd
            && self.ppu.stage_cpu_write(self.cpu.addr, self.cpu.data);
        let double_speed = self.double_speed();
        let cgb = self.cgb.is_some();
        for t in 0..4 {
            let serial_clock_before = self.serial_clock_bit();
            self.timer.tick_t();
            let serial_clock_after = self.serial_clock_bit();
            self.tick_serial_irq(serial_clock_before, serial_clock_after);
            if double_speed && t & 1 == 0 {
                continue;
            }
            let div = if double_speed {
                self.timer.counter >> 1
            } else {
                self.timer.counter
            };
            self.apu.tick(div);
            let drawing = cgb && self.ppu.mode() == 3;
            self.ppu.tick(&self.vram, &self.oam);
            if drawing && self.ppu.mode() == 0 {
                self.hblank_started();
            }
        }

        if self.timer.consume_overflow() {
            self.if_reg |= IF_TIMER;
        }
        if self.ppu.consume_vblank_irq() {
            self.if_reg |= IF_VBLANK;
        }
        if self.ppu.consume_stat_irq() {
            self.if_reg |= IF_STAT;
        }

        // Joypad IRQ: rising edge of "any selected button pressed"
        // (the input nibble's high-to-low transition for any bit).
        let joypad_line = self.joypad.any_selected_pressed();
        if joypad_line && !self.joypad_line_prev {
            self.if_reg |= IF_JOYPAD;
        }
        self.joypad_line_prev = joypad_line;

        if !hdma_stall {
            self.service_cpu(ppu_took_write);
            if self.cpu.stopped {
                self.handle_stop();
            }
        }
        self.tick_speed_switch_halt();
        self.tick_oam_dma();
    }

    /// Run m-cycles until the PPU latches frame-ready (≈ 17 556
    /// m-cycles per frame, twice that at CGB double speed). Returns the
    /// number of m-cycles the frame took.
    pub fn run_frame(&mut self) -> u32 {
        self.run_frame_timed().0
    }

    /// Like [`GameBoy::run_frame`], but returns the frame's length in
    /// dots (4.194304 MHz ticks), which a CGB speed switch does not
    /// change: 70 224 for a full frame at either speed.
    pub fn run_frame_dots(&mut self) -> u32 {
        self.run_frame_timed().1
    }

    /// Attempt one frame, returning elapsed dots and whether the PPU completed it.
    /// LCD-off or stopped video can reach the existing bounded safety limit.
    /// A false outcome must not be counted as a completed video frame.
    pub fn run_frame_dots_bounded(&mut self) -> (u32, bool) {
        self.ppu.consume_frame_ready();
        let (_, dots, completed) = self.run_frame_outcome();
        (dots, completed)
    }

    fn run_frame_timed(&mut self) -> (u32, u32) {
        let (cycles, dots, _) = self.run_frame_outcome();
        (cycles, dots)
    }

    /// Runs a bounded attempt, retaining the native PPU completion signal.
    fn run_frame_outcome(&mut self) -> (u32, u32, bool) {
        const SAFETY_LIMIT: u32 = 30_000 * 4; // ~1.7× a frame, in dots
        let mut count = 0;
        let mut dots = 0;
        loop {
            dots += if self.double_speed() { 2 } else { 4 };
            self.step_m_cycle();
            count += 1;
            let completed = self.ppu.consume_frame_ready();
            if completed || dots >= SAFETY_LIMIT {
                return (count, dots, completed);
            }
        }
    }

    fn service_cpu(&mut self, ppu_took_write: bool) {
        if self.cpu.mreq {
            if self.cpu.rd {
                let value = self.read(self.cpu.addr);
                self.cpu.data_in = value;
            } else if self.cpu.wr && !ppu_took_write {
                let addr = self.cpu.addr;
                let data = self.cpu.data;
                self.write(addr, data);
            }
        }

        if self.cpu.int_ack {
            self.if_reg &= !(1u8 << self.cpu.int_ack_bit);
        }

        self.cpu.irq_pending = self.if_reg & self.ie_reg & 0x1F;
        self.cpu.tick();
    }

    fn read_io(&self, addr: u16) -> u8 {
        if addr == 0xFF50
            && let Some(boot) = &self.mono_boot
        {
            return 0xFE | u8::from(!boot.mapped);
        }
        if let Some(value) = self.read_cgb_io(addr) {
            return value;
        }
        match addr {
            0xFF00 => self.joypad.read_p1(),
            0xFF01 => self.serial_data,
            0xFF02 => self.serial_control | 0x7E, // bits 1-6 wired high
            0xFF04 => self.timer.read_div(),
            0xFF05 => self.timer.read_tima(),
            0xFF06 => self.timer.read_tma(),
            0xFF07 => self.timer.read_tac() | 0xF8, // upper bits wired high
            0xFF0F => self.if_reg | 0xE0,           // upper 3 bits wired high
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF40 => self.ppu.lcdc,
            0xFF41 => self.ppu.read_stat() | 0x80, // bit 7 wired high
            0xFF42 => self.ppu.scy,
            0xFF43 => self.ppu.scx,
            0xFF44 => self.ppu.read_ly(),
            0xFF45 => self.ppu.lyc,
            0xFF46 => self.oam_dma_reg,
            0xFF47 => self.ppu.bgp,
            0xFF48 => self.ppu.obp0,
            0xFF49 => self.ppu.obp1,
            0xFF4A => self.ppu.wy,
            0xFF4B => self.ppu.wx,
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        if addr == 0xFF50
            && let Some(boot) = &mut self.mono_boot
        {
            if value & 1 != 0 {
                boot.mapped = false;
            }
            return;
        }
        if self.write_cgb_io(addr, value) {
            return;
        }
        match addr {
            0xFF00 => self.joypad.write_p1(value),
            0xFF01 => self.serial_data = value,
            0xFF02 => {
                self.serial_control = value;
                if (value & 0x81) == 0x81 {
                    // Keep Blargg/mooneye's emulator-reporting channel
                    // immediate, but schedule IF.SERIAL on the real
                    // DIV-derived serial clock.
                    self.serial_output.push(self.serial_data);
                    self.serial_control &= 0x7F;
                    self.serial_irq_active = true;
                    self.serial_irq_bits_remaining = 8;
                } else if (value & 0x80) == 0 {
                    self.serial_irq_active = false;
                    self.serial_irq_bits_remaining = 0;
                }
            }
            0xFF04 => self.timer.write_div(),
            0xFF05 => self.timer.write_tima(value),
            0xFF06 => self.timer.write_tma(value),
            0xFF07 => self.timer.write_tac(value),
            0xFF0F => self.if_reg = value & 0x1F,
            0xFF10..=0xFF3F => self.apu.write(addr, value),
            0xFF40 => self.ppu.write_lcdc(value),
            0xFF41 => self.ppu.write_stat(value),
            0xFF42 => self.ppu.scy = value,
            0xFF43 => self.ppu.scx = value,
            0xFF44 => {} // LY is read-only; writes ignored
            0xFF45 => self.ppu.write_lyc(value),
            0xFF46 => self.start_oam_dma(value),
            0xFF47 => self.ppu.bgp = value,
            0xFF48 => self.ppu.obp0 = value,
            0xFF49 => self.ppu.obp1 = value,
            0xFF4A => self.ppu.write_wy(value),
            0xFF4B => self.ppu.write_wx(value),
            _ => {}
        }
    }

    fn start_oam_dma(&mut self, page: u8) {
        self.oam_dma_reg = page;
        if self.oam_dma_active() {
            self.oam_dma_pending_source_high = page;
            self.oam_dma_pending_start_delay = 2;
            return;
        }

        self.oam_dma_source_high = page;
        self.oam_dma_index = 0;
        self.oam_dma_start_delay = 2;
    }

    fn tick_oam_dma(&mut self) {
        let previous_dma_active = self.oam_dma_active();

        if self.oam_dma_index < OAM_SIZE as u8 {
            if self.oam_dma_start_delay != 0 {
                self.oam_dma_start_delay -= 1;
            } else {
                let offset = u16::from(self.oam_dma_index);
                let source = (u16::from(self.oam_dma_source_high) << 8) | offset;
                let value = self.oam_dma_source_read(source);
                self.oam[usize::from(self.oam_dma_index)] = value;
                self.oam_dma_last_byte = value;
                self.oam_dma_index = self.oam_dma_index.saturating_add(1);
            }
        }

        if self.oam_dma_pending_start_delay != 0 {
            self.oam_dma_pending_start_delay -= 1;
            if self.oam_dma_pending_start_delay == 0 {
                self.oam_dma_source_high = self.oam_dma_pending_source_high;
                self.oam_dma_index = 0;
                self.oam_dma_start_delay = 0;
            }
        } else if previous_dma_active && self.oam_dma_index >= OAM_SIZE as u8 {
            self.oam_dma_source_high = self.oam_dma_reg;
        }
    }

    fn oam_dma_active(&self) -> bool {
        self.oam_dma_start_delay == 0 && self.oam_dma_index < OAM_SIZE as u8
    }

    /// While an OAM DMA runs the CPU cannot use the bus the DMA is reading
    /// from: a conflicting access returns the byte currently in flight. The
    /// other bus, HRAM and the IO/IE registers stay accessible. Returns the
    /// in-flight byte when `addr` conflicts, else `None`. Models SameBoy's
    /// bus-aware `should_use_dma_conflict` (not a blanket non-HRAM block).
    fn oam_dma_conflict(&self, addr: u16) -> Option<u8> {
        if !self.oam_dma_active() {
            return None;
        }
        let cpu_bus = mem_bus(addr);
        if cpu_bus == MemBus::None {
            return None; // HRAM / IO / IE / OAM — never on a DMA bus
        }
        let source = (u16::from(self.oam_dma_source_high) << 8) | u16::from(self.oam_dma_index);
        (cpu_bus == mem_bus(source)).then_some(self.oam_dma_last_byte)
    }

    fn oam_dma_source_read(&self, addr: u16) -> u8 {
        let source = if addr >= 0xE000 {
            addr.wrapping_sub(0x2000)
        } else {
            addr
        };
        self.bus_read(source)
    }

    fn serial_clock_bit(&self) -> bool {
        ((self.timer.counter >> 8) & 1) != 0
    }

    fn tick_serial_irq(&mut self, clock_before: bool, clock_after: bool) {
        if !self.serial_irq_active || !clock_before || clock_after {
            return;
        }

        self.serial_irq_bits_remaining = self.serial_irq_bits_remaining.saturating_sub(1);
        if self.serial_irq_bits_remaining == 0 {
            self.serial_irq_active = false;
            self.if_reg |= IF_SERIAL;
        }
    }
}

impl MemoryBus for GameBoy {
    fn read(&mut self, addr: u16) -> u8 {
        // The trait declares a mutable receiver because IO reads can
        // have side effects on real hardware (e.g. wave-RAM access
        // during CH3 playback). We delegate to a private read that
        // handles the cases that are pure plus the few that aren't.
        //
        // A CPU read of the bus an in-progress OAM DMA is using returns the
        // in-flight DMA byte instead. The DMA's own source reads go through
        // `bus_read` directly, so they are never gated here.
        if let Some(byte) = self.oam_dma_conflict(addr) {
            return byte;
        }
        self.bus_read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        // Writes to the OAM DMA's bus are dropped while it runs.
        if self.oam_dma_conflict(addr).is_some() {
            return;
        }
        self.bus_write(addr, value);
    }
}

impl GameBoy {
    /// Index into VRAM for a CPU address, in the bank VBK selects.
    fn vram_index(&self, addr: u16) -> usize {
        let bank = self.cgb.as_ref().map_or(0, |cgb| cgb.vram_offset());
        bank + usize::from(addr & 0x1FFF)
    }

    /// Index into WRAM for a CPU address (`$C000-$FDFF`): bank 0, then
    /// the bank SVBK selects (always bank 1 on a DMG).
    fn wram_index(&self, addr: u16) -> usize {
        let offset = usize::from(addr & 0x1FFF);
        if offset < 0x1000 {
            offset
        } else {
            let bank = self.cgb.as_ref().map_or(1, |cgb| cgb.high_wram_bank());
            bank * 0x1000 + offset - 0x1000
        }
    }

    pub(crate) fn bus_read(&self, addr: u16) -> u8 {
        if addr < 0x100
            && let Some(boot) = &self.mono_boot
            && boot.mapped
        {
            return boot.bytes.get(usize::from(addr)).copied().unwrap_or(0xFF);
        }
        if addr < 0x0900
            && let Some(byte) = self.cgb.as_ref().and_then(|cgb| cgb.boot_rom_byte(addr))
        {
            return byte;
        }
        match addr {
            0x0000..=0x7FFF => self.cartridge.read_rom(addr),
            0x8000..=0x9FFF if self.ppu.cpu_blocks_vram_read() => 0xFF,
            0x8000..=0x9FFF => self.vram[self.vram_index(addr)],
            0xA000..=0xBFFF => self.cartridge.read_ram(addr),
            // Echo RAM mirrors $C000..=$DDFF.
            0xC000..=0xFDFF => self.wram[self.wram_index(addr)],
            0xFE00..=0xFE9F if self.oam_dma_active() || self.ppu.cpu_blocks_oam_read() => 0xFF,
            0xFE00..=0xFE9F => self.oam[usize::from(addr - 0xFE00)],
            0xFEA0..=0xFEFF => 0xFF, // unusable region
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)],
            0xFFFF => self.ie_reg,
        }
    }

    fn bus_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cartridge.write_rom(addr, value),
            0x8000..=0x9FFF if self.ppu.cpu_blocks_vram_write() => {}
            0x8000..=0x9FFF => {
                let index = self.vram_index(addr);
                self.vram[index] = value;
            }
            0xA000..=0xBFFF => self.cartridge.write_ram(addr, value),
            0xC000..=0xFDFF => {
                let index = self.wram_index(addr);
                self.wram[index] = value;
            }
            0xFE00..=0xFE9F if self.oam_dma_active() || self.ppu.cpu_blocks_oam_write() => {}
            0xFE00..=0xFE9F => self.oam[usize::from(addr - 0xFE00)] = value,
            0xFEA0..=0xFEFF => {} // unusable region
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)] = value,
            0xFFFF => self.ie_reg = value,
        }
    }
}
