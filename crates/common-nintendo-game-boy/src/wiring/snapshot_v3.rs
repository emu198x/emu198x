//! Migration only: retain the version-3 postcard field order verbatim.
use super::*;

/// Exact pre-monochrome-boot machine layout used by runtime snapshot version 3.
#[derive(Deserialize)]
pub struct GameBoyV3 {
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
}

impl From<GameBoyV3> for GameBoyState {
    fn from(old: GameBoyV3) -> Self {
        Self {
            cpu: old.cpu,
            timer: old.timer,
            ppu: old.ppu,
            apu: old.apu,
            cartridge: old.cartridge,
            wram: old.wram,
            vram: old.vram,
            oam: old.oam,
            hram: old.hram,
            if_reg: old.if_reg,
            ie_reg: old.ie_reg,
            joypad: old.joypad,
            joypad_line_prev: old.joypad_line_prev,
            serial_data: old.serial_data,
            serial_control: old.serial_control,
            serial_irq_active: old.serial_irq_active,
            serial_irq_bits_remaining: old.serial_irq_bits_remaining,
            serial_output: old.serial_output,
            oam_dma_reg: old.oam_dma_reg,
            oam_dma_source_high: old.oam_dma_source_high,
            oam_dma_index: old.oam_dma_index,
            oam_dma_start_delay: old.oam_dma_start_delay,
            oam_dma_pending_source_high: old.oam_dma_pending_source_high,
            oam_dma_pending_start_delay: old.oam_dma_pending_start_delay,
            oam_dma_last_byte: old.oam_dma_last_byte,
            cgb: old.cgb,
            dmg_stop_mode: old.dmg_stop_mode,
            mono_boot: None,
        }
    }
}
