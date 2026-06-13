//! Commodore Amiga (OCS chipset) machine — incremental restart.
//!
//! Built milestone-by-milestone per
//! `knowledge/decisions/amiga-restart-plan.md`. Each milestone adds the
//! minimum hardware behaviour the running ROM demands; nothing more.
//!
//! Current milestone: **M6 — beam counter + VBL interrupt.**

mod agnus;
mod denise;
use common_commodore_amiga::board::{BusResponse, BusTransaction, ChipRamBus, TICKS_PER_CCK};
use common_commodore_amiga::driver::AmigaDriver;
use common_commodore_amiga::{cia, copper, memory, rtc};

pub use agnus::{
    Agnus, AgnusRegion, CckBusPlan, NTSC_CCKS_PER_FRAME, NTSC_FRAME_TICKS, NTSC_LINES_PER_FRAME,
    PAL_CCKS_PER_FRAME, PAL_FRAME_LINES, PAL_FRAME_TICKS, PAL_LINE_CCKS, PAL_LINE_TICKS,
    PAL_LINES_PER_FRAME, SlotOwner, VBL_END_LINE, bits,
};
pub use cia::{Cia, CiaExt};
pub use commodore_amiga_autoconfig::{AutoconfigBoard, AutoconfigState};
pub use commodore_gary::{ChipSelect, Gary};
use commodore_paula_8364::bits::{
    POTGOR_BTN_PORT0_MIDDLE, POTGOR_BTN_PORT0_RIGHT, POTGOR_BTN_PORT1_MIDDLE,
    POTGOR_BTN_PORT1_RIGHT,
};
use commodore_paula_8364::decode as paula_decode;
pub use commodore_paula_8364::{AudioControls, AudioField, IntSource, Paula8364, PaulaChannel};
pub use copper::Copper;
pub use denise::{Denise, FB_HEIGHT, FB_WIDTH};
pub use format_commodore_amiga_adf::Adf;
pub use memory::{CHIP_RAM_SIZE, DEFAULT_CHIP_RAM_SIZE, Memory};
pub use peripheral_commodore_amiga_floppy::{AmigaFloppyDrive, DriveStatus};
pub use peripheral_commodore_amiga_keyboard::AmigaKeyboard;
pub use rtc::RTC_BASE;

use motorola_68000::Cpu68000;
use rtc::Msm6242Rtc;

const CUSTOM_BASE: u32 = 0x00DF_0000;
const CUSTOM_TOP: u32 = 0x00E0_0000;
const SLOW_RAM_BASE: u32 = 0x00C0_0000;
/// Zorro-II autoconfig probe window — the first unconfigured board
/// answers here until `expansion.library` writes its base-address
/// pair to `$E80048` / `$E8004A`.
const AUTOCONFIG_BASE: u32 = 0x00E8_0000;
const AUTOCONFIG_TOP: u32 = 0x00E8_0080;

/// RAM layout for an Amiga instance.
///
/// Chip RAM lives at `$000000` and is required. Slow RAM is the A501-
/// style trapdoor expansion at `$C00000`. Fast RAM is a Zorro-II
/// autoconfig board (implementation lands in a follow-up commit);
/// `fast_kb` is carried here so the runtime preset surface is stable
/// across the autoconfig wiring.
///
/// Sizes are in kilobytes. Only the sizes listed in
/// `memory::is_valid_chip_ram_size` / `is_valid_slow_ram_size` are
/// accepted by `AmigaOcs::with_ram_config`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RamConfig {
    /// Chip RAM in KiB. One of 256, 512, 1024, 2048.
    pub chip_kb: u32,
    /// Slow RAM in KiB. One of 0, 256, 512, 1024, 1536.
    pub slow_kb: u32,
    /// Fast RAM in KiB. Multiples of 64 up to 8192; autoconfig
    /// protocol supports sizes {64, 128, 256, 512, 1024, 2048, 4096,
    /// 8192}. Zero means "no board present".
    pub fast_kb: u32,
}

impl RamConfig {
    /// Stock A500: 512K chip, no expansion.
    #[must_use]
    pub const fn bare() -> Self {
        Self {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 0,
        }
    }

    /// A500 with A501 trapdoor: 512K chip + 512K slow.
    #[must_use]
    pub const fn a501_trapdoor() -> Self {
        Self {
            chip_kb: 512,
            slow_kb: 512,
            fast_kb: 0,
        }
    }

    /// A500Plus-equivalent chip layout: 1M chip, no slow, no fast.
    #[must_use]
    pub const fn a500_plus() -> Self {
        Self {
            chip_kb: 1024,
            slow_kb: 0,
            fast_kb: 0,
        }
    }

    /// Maxed A500: 1M chip + 512K slow + 8M Zorro-II fast.
    #[must_use]
    pub const fn a500_maxed() -> Self {
        Self {
            chip_kb: 1024,
            slow_kb: 512,
            fast_kb: 8192,
        }
    }

    /// `true` if the sizes are all within the supported set.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        memory::is_valid_chip_ram_size(self.chip_kb as usize * 1024)
            && memory::is_valid_slow_ram_size(self.slow_kb as usize * 1024)
            && self.fast_kb <= 8192
            && self.fast_kb.is_multiple_of(64)
    }
}

/// One entry in the diagnostic blit log.
pub type BlitLogEntry = (u64, u32, u16, u16, u32, u32, u32, u32, u16);

impl Default for RamConfig {
    fn default() -> Self {
        Self::bare()
    }
}

/// Convert drive status (active-high booleans) into the CIA-A PRA
/// external-input byte Kickstart reads via `$BFE001`.
///
/// Non-disk bits (PA0=OVL out, PA1=/LED out, PA6=FIR0, PA7=FIR1)
/// default high. Disk bits default high and are pulled low when the
/// corresponding drive signal is asserted.
fn drive_pra_byte(s: &DriveStatus) -> u8 {
    let mut v = 0b1111_1111u8;
    if s.disk_change {
        v &= !(1 << 2);
    }
    if s.write_protect {
        v &= !(1 << 3);
    }
    if s.track0 {
        v &= !(1 << 4);
    }
    if s.ready {
        v &= !(1 << 5);
    }
    v
}

fn joydat(x: u8, y: u8) -> u16 {
    (u16::from(y) << 8) | u16::from(x)
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
struct JoystickState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
    /// Second / third fire buttons (two-button or CD32-style pad). Read
    /// back via POTGOR, not CIA-A PRA — see `set_joystick_control`.
    button2: bool,
    button3: bool,
}

impl JoystickState {
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name.to_ascii_lowercase().as_str() {
            "up" => self.up = pressed,
            "down" => self.down = pressed,
            "left" => self.left = pressed,
            "right" => self.right = pressed,
            "fire" | "button" | "button1" => self.fire = pressed,
            "fire2" | "button2" => self.button2 = pressed,
            "fire3" | "button3" => self.button3 = pressed,
            _ => return false,
        }
        true
    }

    // Amiga joysticks are cross-wired into the two pot pairs: the X pair
    // (JOYxDAT bits 1,0) carries RIGHT (both bits) and DOWN (low-bit
    // toggle); the Y pair (bits 9,8) carries LEFT and UP. Verified
    // against vAmiga Joystick::joydat() + HRM Appendix A:
    //   Right = X1 · Left = Y1 · Down = X0 xor X1 · Up = Y0 xor Y1
    fn x_bits(self) -> u8 {
        let right = self.right && !self.left;
        let down = self.down && !self.up;
        (u8::from(right) << 1) | u8::from(right ^ down)
    }

    fn y_bits(self) -> u8 {
        let left = self.left && !self.right;
        let up = self.up && !self.down;
        (u8::from(left) << 1) | u8::from(left ^ up)
    }
}

/// Decode CIA-B PRB (active-low) into DF0 control booleans for the
/// drive's `update_control(step, dir_inward, side_upper, sel, motor)`
/// signature.
///
/// HRM Appendix F:
///   PB0 /STEP     — step pulse, falling edge advances head
///   PB1  DIR      — 0 = step inward, 1 = step outward
///   PB2 /SIDE     — 0 = upper head
///   PB3 /SEL0     — 0 = DF0 selected
///   PB7 /MTR      — 0 = motor on
fn decode_cia_b_prb_for_df0(prb: u8) -> (bool, bool, bool, bool, bool) {
    let step = (prb & 0x01) == 0;
    let dir_inward = (prb & 0x02) == 0;
    let side_upper = (prb & 0x04) == 0;
    let sel_df0 = (prb & 0x08) == 0;
    let motor_on = (prb & 0x80) == 0;
    (step, dir_inward, side_upper, sel_df0, motor_on)
}

const DEBUG_RTC_LOG_LIMIT: usize = 4096;

// `ChipRamBus`, `BusTransaction`, `BusResponse`, `TICKS_PER_CCK`, and
// `CIA_E_CLOCK_DIVISOR` are shared board glue, relocated to
// `common_commodore_amiga::board` (#34) and imported above.

/// Amiga (OCS) machine.
pub struct AmigaOcs {
    cpu: Cpu68000,
    memory: Memory,
    /// DF0 floppy drive — head / motor / MFM track encoder. Responds
    /// to CIA-B PRB control pulses and feeds CIA-A PRA status bits +
    /// MFM words into Paula's disk DMA engine.
    drive: AmigaFloppyDrive,
    /// Cached encoded MFM track for the drive's current (cyl, head).
    /// Re-encoded on demand when the head moves or a new disk is
    /// inserted; `None` if no disk is present or the head is at a
    /// cylinder we haven't encoded yet this access.
    track_cache: Option<(u32, u32, Vec<u8>)>,
    /// Word cursor into `track_cache.2` — next word to feed to Paula.
    track_word_cursor: usize,
    /// CCKs remaining before the next MFM word is pushed to Paula.
    /// One word every `DISK_BYTE_CCK_SLOW * 2` CCKs in 250 kbit/s
    /// (ADKCON.FAST clear) mode, or `DISK_BYTE_CCK_FAST * 2` in fast.
    track_pacer: u16,
    /// Keyboard controller — produces $FD + $FE power-up sequence and
    /// encoded key events, ticked at the CIA E-clock rate.
    keyboard: AmigaKeyboard,
    /// Last-observed CIA-A CRA bit 6 (SPMODE). The keyboard treats a
    /// 0→1 transition as a handshake: the host has read SDR and is
    /// ready for the next byte.
    prev_cia_a_spmode: bool,
    /// Address decoder — maps 24-bit CPU addresses to chip selects.
    /// Configured once at construction (A500 + slow RAM) and read-
    /// only thereafter.
    gary: Gary,
    /// Battery-backed old-address RTC (`$DC0000`) used by A500+A501-
    /// style configurations. Backed by host time so `SetClock load`
    /// has something real to read.
    rtc: Msm6242Rtc,
    /// Zorro-II autoconfig board, present when the `RamConfig` asks
    /// for fast RAM. `None` when `fast_kb == 0`. Answers at the probe
    /// window `$E80000-$E8007F` until `expansion.library` writes both
    /// halves of the base-address pair; thereafter serves RAM from
    /// its assigned base.
    autoconfig: Option<AutoconfigBoard>,
    cia_a: Cia,
    cia_b: Cia,
    paula: Paula8364,
    /// Mouse/joystick counter state for controller port 0 (JOY0DAT).
    joy0_x: u8,
    joy0_y: u8,
    /// Mouse/joystick counter state for controller port 1 (JOY1DAT).
    joy1_x: u8,
    joy1_y: u8,
    /// Active-low left mouse/fire buttons sampled through CIA-A PRA.
    port0_left_button_pressed: bool,
    port1_left_button_pressed: bool,
    /// Digital joystick state for controller port 1.
    joystick1: JoystickState,
    agnus: Agnus,
    copper: Copper,
    denise: Denise,
    tick_count: u64,
    /// Sub-CCK phase: 0 at the first tick of a CCK (fetch/reload
    /// events fire here), 1 at the second tick. Flips each tick.
    cck_phase: u8,
    /// Paula's latched state of Agnus's `/VERTB` level signal. Used
    /// to detect rising edges — INTREQ.VERTB is re-latched whenever
    /// the CPU clears it and the beam is still inside the blanking
    /// window.
    prev_vertb_level: bool,
    /// Paula's latched state of the CIA-A `/IRQ` line (level-
    /// sensitive on the CIA, edge-latched on Paula). Set to true
    /// when CIA-A has any unmasked ICR flag active.
    prev_cia_a_irq: bool,
    /// Same for CIA-B.
    prev_cia_b_irq: bool,
    e_clock_phase: u64,
    /// Diagnostic: count of unique custom-register read offsets seen
    /// since reset, indexed by offset / 2.
    pub debug_reg_read_counts: std::collections::HashMap<u16, u64>,
    /// Diagnostic: peak INTENA value seen during boot. Bit 14 set
    /// here would prove the boot has reached the master-enable code
    /// path even if INTENA is later cleared.
    pub debug_peak_intena: u16,
    /// Diagnostic: cumulative count of CPU writes to INTENA ($DFF09A).
    pub debug_intena_writes: u64,
    /// Diagnostic: per-write log of every INTENA store, captured to
    /// help trace the master-enable lifecycle. Each entry is
    /// `(cck, pc, written_word, intena_before, intena_after)`. Only
    /// writes that actually change INTENA are kept (purely-no-op
    /// writes still count toward `debug_intena_writes`).
    pub debug_intena_log: Vec<(u64, u32, u16, u16, u16)>,
    /// Diagnostic: log of COP1LC writes (when either high or low
    /// half is written). Entry: (cck, pc, new_cop1lc). Lets us see
    /// who installs the strap copper list (or doesn't).
    pub debug_cop1lc_log: Vec<(u64, u32, u32)>,
    /// Same for COP2LC.
    pub debug_cop2lc_log: Vec<(u64, u32, u32)>,
    /// Diagnostic: log of Paula disk-register writes. Entry is
    /// `(cck, pc, reg_offset, value)` where reg_offset is the
    /// custom-register offset ($020/$022 = DSKPT, $024 = DSKLEN,
    /// $026 = DSKDAT, $07E = DSKSYNC). Lets us see how trackdisk
    /// pokes the disk controller before we add any behaviour.
    pub debug_dsk_log: Vec<(u64, u32, u16, u16)>,
    /// Diagnostic: log of DMACON writes. Entry is
    /// `(cck, pc, raw_val, dmacon_before, dmacon_after)`. Captures
    /// every write to $DFF096; lets us see who enables / disables
    /// BPLEN / SPREN / etc. during boot.
    pub debug_dmacon_log: Vec<(u64, u32, u16, u16, u16)>,
    /// Diagnostic: count of BLTSIZE writes (every one starts a
    /// blit). Independent of whether the blit actually touched
    /// chip RAM — just counts the "CPU kicked a blit" events.
    pub debug_blit_starts: u64,
    /// Diagnostic: log of BLTSIZE writes that triggered a blit.
    /// Entry is `(cck, pc, bltcon0, bltcon1, bltapt, bltbpt,
    /// bltcpt, bltdpt, bltsize)`. Captures the parameters at the
    /// moment the blit was kicked off — so we can replay any
    /// suspicious blit and find issues with B→D copy paths.
    pub debug_blit_log: Vec<BlitLogEntry>,
    /// Diagnostic: log of CIA-A register writes. Entry is
    /// `(cck, pc, reg, raw_val)` where reg is 0..=$F. Lets us see
    /// how timer.device and other code start/stop the CIA-A timers.
    pub debug_cia_a_cr_log: Vec<(u64, u32, u8, u8)>,
    /// Same for CIA-B.
    pub debug_cia_b_cr_log: Vec<(u64, u32, u8, u8)>,
    /// Diagnostic: log of Copper MOVEs routed through the custom
    /// register dispatcher. Entry is `(cck, vpos, hpos, reg, val)`.
    /// Useful for confirming which BPLCON0 mode word is actually
    /// applied during the visible desktop, rather than inferring
    /// from the source template in RAM.
    pub debug_copper_move_log: Vec<(u64, u16, u16, u16, u16)>,
    /// Diagnostic: log of CPU custom-register writes. Entry is
    /// `(cck, pc, addr24, offset, raw_val, is_word)`. Used to catch
    /// byte-write behaviour differences against the archive machine,
    /// especially for display registers like BPLCON0.
    pub debug_custom_write_log: Vec<(u64, u32, u32, u16, u16, bool)>,
    /// Diagnostic: when set, every CPU-initiated memory write whose
    /// address falls in `[watch_addr, watch_addr+watch_len)` is
    /// recorded as `(cck, pc, addr, val, is_word)`. Used by task #96
    /// (chip-only LOFlist investigation) to see which instruction
    /// writes what to a specific memory cell.
    pub debug_watch_addr: Option<(u32, u32)>,
    pub debug_watch_writes: Vec<(u64, u32, u32, u16, bool)>,
    /// Diagnostic: bounded log of CPU RTC bus accesses. Entry is
    /// `(cck, pc, addr24, is_read, is_word, value)`, where `value`
    /// is the delivered word/byte payload. Used to trace KS 1.3's
    /// direct old-address clock probes at `$DC0000`.
    pub debug_rtc_log: Vec<(u64, u32, u32, bool, bool, u16)>,
    /// Diagnostic: log of every BPLCON0 (`$0100`) write. Entry is
    /// `(cck, pc, val)`. Mirrored from the A1200 instrumentation so
    /// the family MCP `bplcon0_log` tool surfaces data on every
    /// chipset (not just AGA). Bounded at 8192 entries.
    pub debug_bplcon0_log: Vec<(u64, u32, u16)>,
    /// Diagnostic: log of palette writes — COLOR (`$0180..$01BE`),
    /// BPLCON3 (`$0106`) and BPLCON4 (`$010C`). Entry is `(cck, pc,
    /// reg_offset, value, bplcon3_at_write)`. On OCS the fifth
    /// field is always `None` because BPLCON3 isn't backed by any
    /// chip register at this address — captures the write-attempt
    /// trace anyway so games / probes that poke $0106 / $010C on OCS
    /// still show up.
    pub debug_palette_log: Vec<(u64, u32, u16, u16, Option<u16>)>,
    /// Diagnostic: log of every CPU read from a chipset register.
    /// Entry is `(cck, pc, offset, returned_value)`. Lets us watch
    /// what an app or KS reads back from the custom-register window —
    /// notably how Kickstart probes the chipset for identification.
    pub debug_reg_read_log: Vec<(u64, u32, u16, u16)>,
}

/// Persistable Amiga (OCS) machine state.
///
/// Captures every chip + every machine-level field whose value affects
/// future behaviour. Diagnostic logs (`debug_*`) are deliberately
/// excluded — they are observability, not state. Disk media is also
/// excluded; the runtime envelope re-mounts disks separately so
/// snapshots reference disks by source rather than embedding ~1 MiB of
/// MFM bytes per inserted floppy.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AmigaOcsSnapshot {
    cpu: Cpu68000,
    memory: Memory,
    drive: AmigaFloppyDrive,
    track_cache: Option<(u32, u32, Vec<u8>)>,
    track_word_cursor: usize,
    track_pacer: u16,
    keyboard: AmigaKeyboard,
    prev_cia_a_spmode: bool,
    gary: Gary,
    rtc: Msm6242Rtc,
    autoconfig: Option<AutoconfigBoard>,
    cia_a: Cia,
    cia_b: Cia,
    paula: Paula8364,
    joy0_x: u8,
    joy0_y: u8,
    joy1_x: u8,
    joy1_y: u8,
    port0_left_button_pressed: bool,
    port1_left_button_pressed: bool,
    joystick1: JoystickState,
    agnus: Agnus,
    copper: Copper,
    denise: Denise,
    tick_count: u64,
    cck_phase: u8,
    prev_vertb_level: bool,
    prev_cia_a_irq: bool,
    prev_cia_b_irq: bool,
    e_clock_phase: u64,
}

impl AmigaOcs {
    fn apply_df0_control_from_cia_b(&mut self) {
        let prb = self.cia_b.port_b_output();
        let (step, dir_in, side_upper, sel0, motor) = decode_cia_b_prb_for_df0(prb);
        self.drive
            .update_control(step, dir_in, side_upper, sel0, motor);
    }

    fn refresh_cia_a_external_inputs(&mut self) {
        let mut pra = drive_pra_byte(&self.drive.status());
        // CIA-A PRA fire bits are active-low: PA6 = /FIR0 (controller
        // port 0 — mouse left button), PA7 = /FIR1 (controller port 1).
        if self.port0_left_button_pressed {
            pra &= !(1 << 6);
        }
        if self.port1_left_button_pressed {
            pra &= !(1 << 7);
        }
        self.cia_a.set_external_a(pra);
    }

    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// and stock 512K chip RAM only (no expansion).
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::with_ram_config(kickstart, RamConfig::bare())
    }

    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// plus a trapdoor slow-RAM expansion at `$C00000` (common A500
    /// config: 512 KiB). Chip RAM stays at stock 512K. Thin wrapper
    /// around `with_ram_config` for test / integration callers that
    /// don't need a full `RamConfig`.
    #[must_use]
    pub fn with_slow_ram(kickstart: Vec<u8>, slow_ram_bytes: usize) -> Self {
        Self::with_ram_config(
            kickstart,
            RamConfig {
                chip_kb: DEFAULT_CHIP_RAM_SIZE as u32 / 1024,
                slow_kb: (slow_ram_bytes / 1024) as u32,
                fast_kb: 0,
            },
        )
    }

    /// Build a new Amiga (OCS) with a fully explicit RAM layout.
    ///
    /// Panics if `cfg` is not one of the supported size combinations
    /// (see `RamConfig::is_valid`). When `cfg.fast_kb > 0` a single
    /// Zorro-II fast-RAM board is attached and starts unconfigured;
    /// `expansion.library` discovers it during boot and assigns its
    /// base address.
    ///
    /// PAL Agnus is used; the matching NTSC entry point is
    /// `with_ram_config_ntsc`.
    #[must_use]
    pub fn with_ram_config(kickstart: Vec<u8>, cfg: RamConfig) -> Self {
        Self::with_ram_config_region(kickstart, cfg, AgnusRegion::Pal)
    }

    /// NTSC counterpart of `with_ram_config`. Same RAM/autoconfig
    /// rules; the Agnus is constructed with NTSC line/frame counts
    /// and the per-line short/long alternation enabled.
    #[must_use]
    pub fn with_ram_config_ntsc(kickstart: Vec<u8>, cfg: RamConfig) -> Self {
        Self::with_ram_config_region(kickstart, cfg, AgnusRegion::Ntsc)
    }

    fn with_ram_config_region(kickstart: Vec<u8>, cfg: RamConfig, region: AgnusRegion) -> Self {
        assert!(
            cfg.is_valid(),
            "RamConfig out of range: {cfg:?}; allowed chip=256/512/1024/2048 KiB, \
             slow=0/256/512/1024/1536 KiB, fast multiple-of-64 up to 8192 KiB"
        );
        let memory = Memory::new_with_ram(
            kickstart,
            cfg.chip_kb as usize * 1024,
            cfg.slow_kb as usize * 1024,
        );
        Self::with_memory_config(memory, cfg, true, region)
    }

    /// Build a real A1000-style machine: a small bootstrap ROM at
    /// `$F80000` plus writable WOM behind the normal 256K Kickstart
    /// window. The WOM remains writable through `$FC0000-$FFFFFF`
    /// until the bootstrap writes into `$F80000-$FBFFFF`, at which
    /// point the bootstrap ROM disappears and the WOM becomes
    /// read-only Kickstart. PAL Agnus.
    #[must_use]
    pub fn with_a1000_bootstrap_rom(boot_rom: Vec<u8>, cfg: RamConfig) -> Self {
        Self::with_a1000_bootstrap_rom_region(boot_rom, cfg, AgnusRegion::Pal)
    }

    /// NTSC counterpart of `with_a1000_bootstrap_rom`. The A1000
    /// shipped in both PAL (Europe) and NTSC (US) configurations
    /// with identical bootstrap ROMs; only the Agnus revision
    /// differs.
    #[must_use]
    pub fn with_a1000_bootstrap_rom_ntsc(boot_rom: Vec<u8>, cfg: RamConfig) -> Self {
        Self::with_a1000_bootstrap_rom_region(boot_rom, cfg, AgnusRegion::Ntsc)
    }

    fn with_a1000_bootstrap_rom_region(
        boot_rom: Vec<u8>,
        cfg: RamConfig,
        region: AgnusRegion,
    ) -> Self {
        assert!(
            cfg.is_valid(),
            "RamConfig out of range: {cfg:?}; allowed chip=256/512/1024/2048 KiB, \
             slow=0/256/512/1024/1536 KiB, fast multiple-of-64 up to 8192 KiB"
        );
        let memory = Memory::new_a1000_bootstrap_with_ram(
            boot_rom,
            cfg.chip_kb as usize * 1024,
            cfg.slow_kb as usize * 1024,
        );
        Self::with_memory_config(memory, cfg, true, region)
    }

    fn with_memory_config(
        memory: Memory,
        cfg: RamConfig,
        slow_ram_decode: bool,
        region: AgnusRegion,
    ) -> Self {
        // Autoconfig only supports the eight Zorro-II sizes; other
        // (still-valid) fast_kb values are rounded down to the nearest
        // supported size, dropping the remainder. In practice the
        // preset surface only asks for supported sizes.
        let autoconfig = if cfg.fast_kb == 0 {
            None
        } else {
            let size = Self::zorro_size_for_kib(cfg.fast_kb);
            size.map(AutoconfigBoard::fast_ram)
        };
        let mut cpu = Cpu68000::new();
        let ssp = memory.read_long(0x000000);
        let pc = memory.read_long(0x000004);
        cpu.reset_to(ssp, pc);
        // CIA-A PRA disk-subsystem input pins (per the Amiga HRM's
        // "Disk Subsystem" table — the ROM reads /DSKCHANGE via
        // `btst #2, $BFE001`, confirming these live on CIA-A PRA,
        // not CIA-B). Bits are active-low — 1 = deasserted, 0 = asserted:
        //   PA5 /DSKRDY — reflects drive-ID stream when motor off,
        //                 otherwise motor-at-speed.
        //   PA4 /DSKTRACK0 — low when head is at cylinder 0.
        //   PA3 /DSKPROT — low when disk is write-protected.
        //   PA2 /DSKCHANGE — low when disk changed since last step.
        // The other bits (PA0=OVL output, PA1=/LED output, PA6=FIR0,
        // PA7=FIR1) default high / inactive.
        //
        // Prior to the floppy port we used a static `$EB` constant
        // here. That still matches `drive_pra_byte(drive.status())`
        // on a fresh drive with no disk, so boot reaches the insert-
        // disk screen identically.
        let drive = AmigaFloppyDrive::new();
        let mut cia_a = Cia::new();
        cia_a.set_external_a(drive_pra_byte(&drive.status()));
        // Gary decode is model-shaped here, not hard-coded to the
        // A500 path: even machines without installed slow RAM still
        // need the `Cxxxxx` aperture so KS 1.x can detect the
        // mirrored custom-register side effects in absent blocks.
        let mut gary = Gary::new();
        gary.set_slow_ram_present(slow_ram_decode);
        gary.set_rtc_present(cfg.slow_kb > 0);
        Self {
            cpu,
            memory,
            drive,
            track_cache: None,
            track_word_cursor: 0,
            track_pacer: 0,
            keyboard: AmigaKeyboard::new(),
            prev_cia_a_spmode: false,
            gary,
            rtc: Msm6242Rtc::new(),
            autoconfig,
            cia_a,
            cia_b: Cia::new(),
            paula: Paula8364::new(),
            joy0_x: 0,
            joy0_y: 0,
            joy1_x: 0,
            joy1_y: 0,
            port0_left_button_pressed: false,
            port1_left_button_pressed: false,
            joystick1: JoystickState::default(),
            agnus: Agnus::new_with_region(region),
            copper: Copper::new(),
            denise: Denise::new(),
            tick_count: 0,
            cck_phase: 0,
            // Initialise as `true` because at reset the beam is at
            // vpos=0 (inside the VBL window), so the level signal is
            // already high. A `false` initial value would fake a
            // rising edge on the first tick and spuriously fire TOD
            // / copper-restart before the first real VBL.
            prev_vertb_level: true,
            prev_cia_a_irq: false,
            prev_cia_b_irq: false,
            e_clock_phase: 0,
            debug_reg_read_counts: std::collections::HashMap::new(),
            debug_peak_intena: 0,
            debug_intena_writes: 0,
            debug_intena_log: Vec::new(),
            debug_cop1lc_log: Vec::new(),
            debug_cop2lc_log: Vec::new(),
            debug_dsk_log: Vec::new(),
            debug_dmacon_log: Vec::new(),
            debug_blit_starts: 0,
            debug_blit_log: Vec::new(),
            debug_cia_a_cr_log: Vec::new(),
            debug_cia_b_cr_log: Vec::new(),
            debug_copper_move_log: Vec::new(),
            debug_custom_write_log: Vec::new(),
            debug_watch_addr: None,
            debug_watch_writes: Vec::new(),
            debug_rtc_log: Vec::new(),
            debug_bplcon0_log: Vec::new(),
            debug_palette_log: Vec::new(),
            debug_reg_read_log: Vec::new(),
        }
    }

    fn log_rtc_access(&mut self, addr24: u32, is_read: bool, is_word: bool, value: u16) {
        if self.debug_rtc_log.len() >= DEBUG_RTC_LOG_LIMIT {
            return;
        }
        self.debug_rtc_log.push((
            self.tick_count / TICKS_PER_CCK,
            self.cpu.regs.pc,
            addr24,
            is_read,
            is_word,
            value,
        ));
    }

    /// Read-only Agnus access.
    #[must_use]
    pub fn agnus(&self) -> &Agnus {
        &self.agnus
    }

    /// Active video region — PAL or NTSC. Drives runtime frame timing
    /// and is exposed to the runtime layer so query callers and the
    /// `AmigaMachine` impl can ask the machine which region it's
    /// emulating without poking into Agnus directly.
    #[must_use]
    pub fn region(&self) -> AgnusRegion {
        self.agnus.region
    }

    /// Read-only Copper access.
    #[must_use]
    pub fn copper(&self) -> &Copper {
        &self.copper
    }

    /// Read-only Denise access.
    #[must_use]
    pub fn denise(&self) -> &Denise {
        &self.denise
    }

    /// Read-only memory access (for tests inspecting OVL state etc.).
    #[must_use]
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Read-only CIA-A access.
    #[must_use]
    pub fn cia_a(&self) -> &Cia {
        &self.cia_a
    }

    /// Mutable CIA-A access. Counterpart to `cia_b_mut` — for tests
    /// driving input-pin-level behaviour (e.g. `receive_serial_byte`
    /// for the keyboard path). Not used by the runtime tick loop.
    pub fn cia_a_mut(&mut self) -> &mut Cia {
        &mut self.cia_a
    }

    /// Read-only CIA-B access.
    #[must_use]
    pub fn cia_b(&self) -> &Cia {
        &self.cia_b
    }

    /// Mutable CIA-B access. Only for tests/integrations that need to
    /// drive input pins the machine itself doesn't yet wire — currently
    /// the floppy index pulse via `flag_falling_edge()`. Runtime code
    /// must not reach across this boundary.
    pub fn cia_b_mut(&mut self) -> &mut Cia {
        &mut self.cia_b
    }

    /// Convenience: CIA-A PRA byte (the latched data register).
    #[must_use]
    pub fn cia_a_pra(&self) -> u8 {
        self.cia_a.port_a_latch()
    }

    /// Convenience: CIA-A DDRA byte.
    #[must_use]
    pub fn cia_a_ddra(&self) -> u8 {
        self.cia_a.ddr_a()
    }

    /// Convenience: current INTENA value (from Paula).
    #[must_use]
    pub fn intena(&self) -> u16 {
        self.paula.intena()
    }

    /// Convenience: current INTREQ value (from Paula).
    #[must_use]
    pub fn intreq(&self) -> u16 {
        self.paula.intreq()
    }

    /// Convenience: current DMACON value.
    #[must_use]
    pub fn dmacon(&self) -> u16 {
        self.agnus.dmacon
    }

    /// Convenience: current ADKCON value (from Paula).
    #[must_use]
    pub fn adkcon(&self) -> u16 {
        self.paula.adkcon()
    }

    /// Read-only Paula access.
    #[must_use]
    pub fn paula(&self) -> &Paula8364 {
        &self.paula
    }

    /// Mutable Paula access for tests / integrations that need to
    /// drive input pins the machine itself doesn't yet wire — currently
    /// `flag_falling_edge` via CIA-B. Runtime code must not reach
    /// across this boundary.
    pub fn paula_mut(&mut self) -> &mut Paula8364 {
        &mut self.paula
    }

    /// Current host-side Paula audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.paula.audio_controls()
    }

    /// Replace all host-side Paula audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.paula.set_audio_controls(controls);
    }

    /// Enable or disable one Paula channel in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: PaulaChannel, enabled: bool) {
        self.paula.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one Paula channel's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: PaulaChannel, gain: f32) {
        self.paula.set_audio_channel_gain(channel, gain);
    }

    /// Read-only DF0 drive access.
    #[must_use]
    pub fn drive(&self) -> &AmigaFloppyDrive {
        &self.drive
    }

    /// Canonical DF0 mount. `change_pending = true` leaves `/DSKCHANGE`
    /// pending (a "newly inserted" disk — the A1000 boot path needs it);
    /// `false` acknowledges it ("disk ready"). `writable = false` mounts
    /// read-only — an archive that reports `/DSKPROT` and rejects a SAVE
    /// (#97). The three `insert_adf*` helpers are thin presets over this.
    pub fn mount_adf(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        self.drive.insert_disk_writable(adf, writable);
        if !change_pending {
            self.drive.acknowledge_disk_change();
        }
        self.refresh_cia_a_external_inputs();
    }

    /// Insert an ADF into DF0, writable, acknowledging the change so
    /// Kickstart sees "disk ready" rather than "newly inserted".
    pub fn insert_adf(&mut self, adf: Adf) {
        self.mount_adf(adf, false, true);
    }

    /// Insert an ADF into DF0 with an explicit writability (archives
    /// mount read-only), acknowledging the change (#97).
    pub fn insert_adf_writable(&mut self, adf: Adf, writable: bool) {
        self.mount_adf(adf, false, writable);
    }

    /// Insert an ADF into DF0, writable, but leave `/DSKCHANGE` pending
    /// (a "newly inserted" disk still awaiting acknowledgement).
    pub fn insert_adf_with_change_pending(&mut self, adf: Adf) {
        self.mount_adf(adf, true, true);
    }

    /// Eject the disk from DF0.
    pub fn eject_disk(&mut self) {
        self.drive.eject_disk();
        self.refresh_cia_a_external_inputs();
    }

    /// Move the emulated mouse in controller port 0. JOY0DAT stores
    /// wrapping 8-bit relative counters; right and down increment.
    pub fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        self.joy0_x = self.joy0_x.wrapping_add(dx.rem_euclid(256) as u8);
        self.joy0_y = self.joy0_y.wrapping_add(dy.rem_euclid(256) as u8);
    }

    /// Set one emulated mouse button for controller port 0.
    pub fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        match button {
            "left" => {
                self.port0_left_button_pressed = pressed;
                self.refresh_cia_a_external_inputs();
            }
            "right" => self
                .paula
                .set_pot_pin_level(POTGOR_BTN_PORT0_RIGHT, !pressed),
            "middle" => self
                .paula
                .set_pot_pin_level(POTGOR_BTN_PORT0_MIDDLE, !pressed),
            _ => {}
        }
    }

    /// Set one emulated digital joystick control for controller port 1.
    ///
    /// Returns `false` when the port or control name is unknown. Port 0
    /// remains reserved for the mouse in the current native verifier path.
    pub fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) -> bool {
        if port != 1 || !self.joystick1.set_control(name, pressed) {
            return false;
        }
        self.port1_left_button_pressed = self.joystick1.fire;
        self.joy1_x = (self.joy1_x & 0xFC) | self.joystick1.x_bits();
        self.joy1_y = (self.joy1_y & 0xFC) | self.joystick1.y_bits();
        // Second / third fire buttons sit on port 1's POTGOR pot lines
        // (active-low), the same pins the mouse right / middle buttons
        // use. Per vAmiga Joystick::changePotgo: button2 → the RIGHT
        // pin, button3 → the MIDDLE pin.
        self.paula
            .set_pot_pin_level(POTGOR_BTN_PORT1_RIGHT, !self.joystick1.button2);
        self.paula
            .set_pot_pin_level(POTGOR_BTN_PORT1_MIDDLE, !self.joystick1.button3);
        self.refresh_cia_a_external_inputs();
        true
    }

    /// Read-only JOY0DAT (`$DFF00A`, controller port 0 / mouse) — the
    /// raw 16-bit pot-counter register a program reads to sense the
    /// mouse or a joystick plugged into port 0.
    #[must_use]
    pub fn joy0dat(&self) -> u16 {
        joydat(self.joy0_x, self.joy0_y)
    }

    /// Read-only JOY1DAT (`$DFF00C`, controller port 1 / joystick) —
    /// the raw 16-bit pot-counter register. Digital-joystick direction
    /// bits are decoded here exactly as the hardware presents them, so
    /// a headless query sees the same value `Joyx(1)`/`Joyy(1)` would.
    #[must_use]
    pub fn joy1dat(&self) -> u16 {
        joydat(self.joy1_x, self.joy1_y)
    }

    /// Read-only keyboard controller access — useful for tests
    /// inspecting the power-up state or queued key count.
    #[must_use]
    pub fn keyboard(&self) -> &AmigaKeyboard {
        &self.keyboard
    }

    /// Queue a keyboard event for transmission to the host. `pressed`
    /// = true sets the raw keycode (bit 7 clear); `pressed` = false
    /// raises bit 7 to signal key-up. Bytes are rotated + inverted
    /// before they leave via CIA-A SDR.
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        self.keyboard.key_event(keycode, pressed);
    }

    /// Read-only Gary access — useful for tests / diagnostics that
    /// want to inspect the chip-select decode.
    #[must_use]
    pub fn gary(&self) -> &Gary {
        &self.gary
    }

    /// Read-only access to the Zorro-II fast-RAM autoconfig board, if
    /// one is present. `None` when the `RamConfig` had `fast_kb == 0`
    /// or the size wasn't one of the supported autoconfig sizes.
    #[must_use]
    pub fn autoconfig(&self) -> Option<&AutoconfigBoard> {
        self.autoconfig.as_ref()
    }

    /// Round an arbitrary fast-RAM size in KiB down to the nearest
    /// Zorro-II board size. `None` for zero or sub-64 KiB requests.
    /// Zorro-II boards come in {64, 128, 256, 512, 1024, 2048, 4096,
    /// 8192} KiB; smaller-than-max requests that fall between tiers
    /// round down (e.g. 1024 + 64 → 1024).
    fn zorro_size_for_kib(kib: u32) -> Option<u32> {
        const TIERS: &[u32] = &[8192, 4096, 2048, 1024, 512, 256, 128, 64];
        TIERS.iter().copied().find(|&t| kib >= t)
    }

    /// Read a word from the autoconfig board's RAM window, if the
    /// board is configured and the address lands inside its assigned
    /// range. Returns `None` otherwise so the caller falls through to
    /// the next handler.
    fn autoconfig_fast_ram_read_word(&self, addr24: u32) -> Option<u16> {
        let board = self.autoconfig.as_ref()?;
        let hi = board.read_ram_byte(addr24)?;
        let lo = board.read_ram_byte(addr24.wrapping_add(1)).unwrap_or(0);
        Some((u16::from(hi) << 8) | u16::from(lo))
    }

    /// Read a byte from the autoconfig board's RAM window, if the
    /// board is configured and the address lands inside its assigned
    /// range. Returns `None` otherwise.
    fn autoconfig_fast_ram_read_byte(&self, addr24: u32) -> Option<u8> {
        self.autoconfig.as_ref()?.read_ram_byte(addr24)
    }

    /// Write a byte to the autoconfig board's RAM window if the
    /// address lands inside its range. Returns `true` when the write
    /// was absorbed by the board, `false` to let the caller continue
    /// with the default handler.
    fn autoconfig_fast_ram_write_byte(&mut self, addr24: u32, val: u8) -> bool {
        let Some(board) = self.autoconfig.as_mut() else {
            return false;
        };
        let Some(base) = board.base() else {
            return false;
        };
        let size = board.ram_size();
        if addr24 < base || addr24 >= base + size {
            return false;
        }
        board.write_ram_byte(addr24, val);
        true
    }

    /// Write a word to the autoconfig board's RAM window if the
    /// address lands inside its range. Returns `true` on absorption.
    fn autoconfig_fast_ram_write_word(&mut self, addr24: u32, val: u16) -> bool {
        if !self.autoconfig_fast_ram_write_byte(addr24, (val >> 8) as u8) {
            return false;
        }
        self.autoconfig_fast_ram_write_byte(addr24.wrapping_add(1), val as u8);
        true
    }

    /// Decode a 24-bit CPU address to its chip select. Convenience
    /// wrapper around `gary.decode(addr)`.
    #[must_use]
    pub fn chip_select(&self, addr: u32) -> ChipSelect {
        self.gary.decode(addr)
    }

    /// Resolve a CPU-visible custom-register offset for `addr24`.
    ///
    /// Besides the normal `$DFFxxx` window, Kickstart 1.x probes
    /// absent A500/A2000 slow-RAM blocks via addresses like
    /// `$C0F09A` / `$C0F01C`. On real hardware those unbacked
    /// addresses alias to the custom-register window; backed slow RAM
    /// must still win and behave like RAM.
    fn custom_offset_for_addr(&self, addr24: u32) -> Option<u16> {
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr24) {
            return Some((addr24 - CUSTOM_BASE) as u16 & 0x1FE);
        }
        self.slow_ram_hole_custom_offset(addr24)
    }

    /// On A500/A2000-style machines, the unbacked portion of the
    /// slow-RAM aperture exposes the same low-16-bit `x?Fxxx`
    /// custom-register mirror that KS 1.x uses for its slow-RAM
    /// probe. Installed slow RAM must mask this completely.
    fn slow_ram_hole_custom_offset(&self, addr24: u32) -> Option<u16> {
        if self.gary.decode(addr24) != ChipSelect::SlowRam {
            return None;
        }
        let slow_off = addr24.checked_sub(SLOW_RAM_BASE)? as usize;
        if slow_off < self.memory.slow_ram_size() {
            return None;
        }
        if (addr24 & 0x0000_F000) != 0x0000_F000 {
            return None;
        }
        Some((addr24 & 0x1FE) as u16)
    }

    /// Push the next MFM word from the drive's encoded track buffer
    /// into Paula's disk DMA engine. Re-encodes the track when the
    /// drive head has moved since the last word, or when no cache
    /// exists yet. Rotates the cursor back to 0 at end of track so
    /// successive revolutions keep delivering words.
    ///
    /// Once Paula is asked to consume the word via
    /// [`Paula8364::tick_disk_dma_slot`], it returns the word the
    /// machine should write to chip RAM at DSKPT — or `None` when
    /// WORDSYNC is still gating, the transfer is a write, or no
    /// transfer is in flight. Paula self-clears `disk_dma_pending`
    /// and raises DSKBLK when the transfer's word count hits zero.
    fn feed_next_mfm_word(&mut self) {
        let cyl = self.drive.cylinder();
        let head = self.drive.head();
        let need_refresh = match &self.track_cache {
            Some((c, h, _)) => *c != cyl || *h != head,
            None => true,
        };
        if need_refresh {
            let Some(bytes) = self.drive.encode_mfm_track() else {
                self.track_cache = None;
                return;
            };
            self.track_cache = Some((cyl, head, bytes));
            self.track_word_cursor = 0;
        }
        let Some((_, _, bytes)) = &self.track_cache else {
            return;
        };
        let word_count = bytes.len() / 2;
        if word_count == 0 {
            return;
        }
        // Wrap the cursor at end-of-track so a DMA transfer that
        // exceeds one revolution gets a second copy of the track's
        // front sectors, just like a real disk rotating continuously.
        // KS 1.3's trackdisk *relies* on this wrap: when it decodes
        // sector N>0 it scans forward `(11 - N) * 1088` bytes from
        // the current sync and expects to find sector 0's sync (the
        // start of the next revolution). Returning $AAAA filler
        // past the end of the track would starve that scan and
        // every sector past sector 0 would decode to garbage, so
        // wrap deliberately.
        if self.track_word_cursor >= word_count {
            self.track_word_cursor = 0;
        }
        let i = self.track_word_cursor * 2;
        let word = (u16::from(bytes[i]) << 8) | u16::from(bytes[i + 1]);
        self.track_word_cursor += 1;

        // Disk-DMA glue. Paula owns the WORDSYNC gate, the word
        // countdown, and the DSKBLK interrupt; the machine owns
        // chip RAM and DSKPT (Agnus's pointer register).
        if let Some(write_word) = self.paula.tick_disk_dma_slot(word) {
            let addr = self.agnus.dsk_pt & 0x001F_FFFE;
            self.memory.write_word(addr, write_word);
            self.agnus.dsk_pt = self.agnus.dsk_pt.wrapping_add(2);
        }
    }

    /// Disk *write* DMA glue — the chip-RAM → drive mirror of
    /// `feed_next_mfm_word`. Pull the next word from chip RAM at DSKPT,
    /// hand it to Paula's write slot, and on `Some` feed it to the
    /// drive's MFM write capture and advance DSKPT (Paula owns the word
    /// countdown + the DSKBLK interrupt). When the transfer drains,
    /// decode the captured MFM track and persist the decoded sectors to
    /// the disk image — turning a Workbench SAVE into real bytes instead
    /// of silently dropping it.
    fn feed_next_write_word(&mut self) {
        let addr = self.agnus.dsk_pt & 0x001F_FFFE;
        let word = self.memory.read_word(addr);
        if let Some(write_word) = self.paula.tick_disk_write_dma_slot(word) {
            self.drive.note_write_mfm_word(write_word);
            self.agnus.dsk_pt = self.agnus.dsk_pt.wrapping_add(2);
            if !self.paula.disk_dma_write_active() {
                // Final word transferred — decode the captured MFM and
                // write the decoded sectors back to the disk image.
                self.drive.flush_write_capture();
            }
        }
    }

    /// CCKs between consecutive MFM words at 250 kbit/s (ADKCON.FAST
    /// clear) or 500 kbit/s (FAST set). Paula's internal byte pacer
    /// uses 28 / 14 CCKs per byte; a word is two bytes.
    fn disk_word_cck_interval(&self) -> u16 {
        const ADKCON_FAST: u16 = 0x0100;
        if self.paula.adkcon() & ADKCON_FAST != 0 {
            28
        } else {
            56
        }
    }

    /// Convenience: current BPLCON0 value.
    #[must_use]
    pub fn bplcon0(&self) -> u16 {
        self.agnus.bplcon0
    }

    /// Convenience: a colour table entry.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        self.denise.color(idx)
    }

    /// Backdoor for tests: write a word as if the CPU did it.
    pub fn poke_word(&mut self, addr: u32, val: u16) {
        let addr24 = addr & 0xFF_FFFF;
        // Zorro-II autoconfig probe window: base-address assignment
        // and shut-up writes go here. Test harnesses use this path
        // to walk the `expansion.library` probe scan step by step.
        if (AUTOCONFIG_BASE..AUTOCONFIG_TOP).contains(&addr24) {
            if let Some(board) = self.autoconfig.as_mut() {
                board.write_word((addr24 - AUTOCONFIG_BASE) as u16, val);
            }
            return;
        }
        // Fast-RAM window for a configured autoconfig board.
        if self.autoconfig_fast_ram_write_word(addr24, val) {
            return;
        }
        if let Some(offset) = self.custom_offset_for_addr(addr24) {
            self.dispatch_custom_write(offset, val);
            return;
        }
        match self.gary.decode(addr24) {
            ChipSelect::Rtc => self.rtc.write_word(addr24, val),
            _ => self.memory.write_word(addr24, val),
        }
    }

    /// Dispatch a custom-register word write to the right submodule.
    /// Shared between `poke_word` and the CPU bus servicer.
    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        let intena_before = self.paula.intena();
        match offset {
            0x080 => {
                self.copper.cop1lc = (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(val) << 16);
                self.debug_cop1lc_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    self.copper.cop1lc,
                ));
            }
            0x082 => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
                self.debug_cop1lc_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    self.copper.cop1lc,
                ));
            }
            0x084 => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(val) << 16);
                self.debug_cop2lc_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    self.copper.cop2lc,
                ));
            }
            0x086 => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
                self.debug_cop2lc_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    self.copper.cop2lc,
                ));
            }
            0x088 => self.copper.jump1(),
            0x08A => self.copper.jump2(),
            0x02E => self.copper.write_copcon(val),
            // Paula-owned register space: INTENA / INTREQ / ADKCON.
            0x096 => {
                let before = self.agnus.dmacon;
                self.agnus.write_dmacon(val);
                let after = self.agnus.dmacon;
                if before != after {
                    self.debug_dmacon_log.push((
                        self.tick_count / TICKS_PER_CCK,
                        self.cpu.regs.pc,
                        val,
                        before,
                        after,
                    ));
                }
            }
            0x09A => self.paula.write_intena(val),
            0x09C => self.paula.write_intreq(val),
            0x09E => self.paula.write_adkcon(val),
            // Paula-owned audio channel storage ($0A0..=$0DA).
            0x0A0..=0x0DA => {
                if let Some((ch, field)) = paula_decode::audio_register(offset) {
                    self.paula.write_audio(ch, field, val);
                }
            }
            // Paula-owned disk registers.
            0x024 => self.paula.write_dsklen(val),
            0x026 => self.paula.write_dskdat(val),
            0x07E => self.paula.set_dsksync(val),
            // Paula-owned serial registers.
            0x030 => self.paula.write_serdat(val),
            0x032 => self.paula.write_serper(val),
            // Paula-owned POTGO (\$034).
            0x034 => self.paula.write_potgo(val),
            // Denise JOYTEST ($036): writes the upper six bits of all
            // four mouse counters; low two bits remain live switch
            // state on real hardware, so leave them untouched.
            0x036 => {
                self.joy0_x = (self.joy0_x & 0x03) | ((val as u8) & 0xFC);
                self.joy0_y = (self.joy0_y & 0x03) | (((val >> 8) as u8) & 0xFC);
                self.joy1_x = (self.joy1_x & 0x03) | ((val as u8) & 0xFC);
                self.joy1_y = (self.joy1_y & 0x03) | (((val >> 8) as u8) & 0xFC);
            }
            // Agnus-owned blitter registers. BLTSIZE ($058) fires
            // `start_blit` inside `write_blitter_register`, arming the
            // incremental scheduler; the blit then drains one DMA op per
            // granted CCK in the tick loop (#31) — BBUSY stays set in
            // DMACONR until it finishes — instead of completing here.
            //
            // Real Agnus CPU-stalls a blitter-register write that lands
            // while a blit is in flight (BBUSY) until the blitter is
            // free. We approximate that serialization by draining the
            // in-flight blit before applying the write, so code that
            // reprograms the blitter without an intervening WaitBlit
            // still sees the first blit complete rather than have it
            // aborted by the next `start_blit`.
            0x040..=0x074 => {
                if self.agnus.blitter_busy {
                    let mut bus = ChipRamBus(&mut self.memory);
                    self.agnus.run_blit_to_completion(&mut bus);
                    self.paula.raise(IntSource::Blit);
                }
                if self.agnus.write_blitter_register(offset, val) && offset == 0x058 {
                    self.debug_blit_starts += 1;
                    self.debug_blit_log.push((
                        self.tick_count / TICKS_PER_CCK,
                        self.cpu.regs.pc,
                        self.agnus.bltcon0,
                        self.agnus.bltcon1,
                        self.agnus.blt_apt,
                        self.agnus.blt_bpt,
                        self.agnus.blt_cpt,
                        self.agnus.blt_dpt,
                        self.agnus.bltsize,
                    ));
                }
            }
            // Agnus-owned bitplane + display-window + DSK pointer.
            0x020 => self.agnus.write_dsk_pointer(true, val),
            0x022 => self.agnus.write_dsk_pointer(false, val),
            0x08E => self.agnus.write_diwstrt(val),
            0x090 => self.agnus.write_diwstop(val),
            0x092 => self.agnus.write_ddfstrt(val),
            0x094 => self.agnus.write_ddfstop(val),
            0x100 => {
                self.agnus.write_bplcon0(val);
                // Mirror into Denise so HIRES/HAM/DBLPF/LACE bits take
                // effect at the next pixel, not only next tick.
                self.denise.ocs.bplcon0 = val;
                if self.debug_bplcon0_log.len() < 8192 {
                    self.debug_bplcon0_log.push((
                        self.tick_count / TICKS_PER_CCK,
                        self.cpu.regs.pc,
                        val,
                    ));
                }
            }
            0x108 => self.agnus.write_bpl1mod(val),
            0x10A => self.agnus.write_bpl2mod(val),
            0x0E0..=0x0F5 => {
                let plane_idx = ((offset - 0x0E0) / 4) as usize;
                let high = (offset & 2) == 0;
                self.agnus.write_bpl_pointer(plane_idx, high, val);
            }
            // Agnus-owned sprite pointers SPR0PTH..SPR7PTL at $120..=$13E.
            // Per HRM 4-4, pointer low-word write commits the pair.
            0x120..=0x13E => {
                let sprite = ((offset - 0x120) / 4) as usize;
                let high = (offset & 2) == 0;
                self.agnus.write_sprite_pointer_reg(sprite, high, val);
            }
            // Direct (CPU/copper) writes to SPRxPOS ($140+8n) / SPRxCTL
            // ($142+8n) must update Agnus's VSTART/VSTOP comparators, not
            // just Denise — otherwise a DMA sprite positioned by direct
            // register writes (Blitz `ShowSprite`, whose chip-RAM control
            // words stay zero) never activates. Mirrors vAmiga, where
            // every SPRxPOS/CTL write pokes both Agnus and Denise (#455).
            0x140..=0x17F => {
                let channel = ((offset - 0x140) / 8) as usize;
                match offset & 0x7 {
                    0 => self.agnus.poke_sprite_pos(channel, val),
                    2 => self.agnus.poke_sprite_ctl(channel, val),
                    _ => {} // SPRxDATA / SPRxDATB are Denise-only.
                }
                self.denise.write_word(offset, val);
            }
            _ => self.denise.write_word(offset, val),
        }
        if matches!(offset, 0x020 | 0x022 | 0x024 | 0x026 | 0x07E) {
            self.debug_dsk_log.push((
                self.tick_count / TICKS_PER_CCK,
                self.cpu.regs.pc,
                offset,
                val,
            ));
        }
        // Capture COLOR ($180..$1BE), BPLCON3 ($0106) and BPLCON4
        // ($010C) writes. On OCS BPLCON3 isn't backed by any chip
        // register (the address writes nowhere) — we still record the
        // attempt so games / probes that hit $0106 / $010C show up.
        // The fifth field is `None` because there's no live BPLCON3
        // state to sample; the AGA / ECS impls sample their real
        // BPLCON3 register so callers can reconstruct AGA-bank /
        // sprite-resolution context.
        if (((0x180..=0x1BE).contains(&offset) && (offset & 1) == 0)
            || offset == 0x0106
            || offset == 0x010C)
            && self.debug_palette_log.len() < 262144
        {
            self.debug_palette_log.push((
                self.tick_count / TICKS_PER_CCK,
                self.cpu.regs.pc,
                offset,
                val,
                None,
            ));
        }
        if offset == 0x09A {
            self.debug_intena_writes += 1;
            let intena_after = self.paula.intena();
            if intena_after > self.debug_peak_intena {
                self.debug_peak_intena = intena_after;
            }
            if intena_after != intena_before {
                self.debug_intena_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    val,
                    intena_before,
                    intena_after,
                ));
            }
        }
    }

    /// Backdoor for tests: write a byte as if the CPU did it.
    pub fn poke_byte(&mut self, addr: u32, val: u8) {
        if let Some(reg) = cia::decode_cia_a(addr) {
            self.cia_a.write(reg, val);
            self.memory.set_overlay(self.cia_a.ovl());
        } else if let Some(reg) = cia::decode_cia_b(addr) {
            self.cia_b.write(reg, val);
            if matches!(reg, 0x01 | 0x03) {
                self.apply_df0_control_from_cia_b();
            }
        } else if let Some(offset) = self.custom_offset_for_addr(addr & 0xFF_FFFF) {
            // Custom registers are word-only; byte writes pad with
            // the same byte in both halves on real hardware. For our
            // purposes a byte write just writes the byte value.
            let word = u16::from(val) << 8 | u16::from(val);
            self.dispatch_custom_write(offset, word);
        } else if self.gary.decode(addr) == ChipSelect::Rtc {
            self.rtc.write_byte(addr, val);
        } else {
            self.memory.write_byte(addr, val);
        }
    }

    /// CPU access (read-only — mutating outside the tick loop breaks
    /// invariants).
    #[must_use]
    pub fn cpu(&self) -> &Cpu68000 {
        &self.cpu
    }

    /// Total master/4 ticks (= 68000 CPU clocks = lores pixels)
    /// elapsed since construction. This is the finest-grained clock
    /// in the machine.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Total Agnus CCKs (colour clocks, master/8) elapsed since
    /// construction. Derived from `tick_count` — 2 ticks per CCK.
    /// Useful for comparing timestamps against HRM beam-coordinate
    /// register values.
    #[must_use]
    pub fn cck_count(&self) -> u64 {
        self.tick_count / TICKS_PER_CCK
    }

    /// Read a word at the given 24-bit address — peeks state without
    /// side effects (does NOT clear ICR etc). For inspecting state
    /// during tests; not equivalent to a CPU bus cycle.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.bus_read_word(addr & 0xFF_FFFF)
    }

    /// Read a word as if the CPU did the bus cycle. Side-effecting:
    /// CIA-A ICR reads clear ICR; future read-side-effect registers
    /// behave like the CPU sees them.
    pub fn cpu_read_word(&mut self, addr: u32) -> u16 {
        let addr24 = addr & 0xFF_FFFF;
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.read(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.read(reg));
        }
        self.bus_read_word(addr24)
    }

    /// Read a longword (big-endian) at the given 24-bit address.
    #[must_use]
    pub fn read_long(&self, addr: u32) -> u32 {
        let hi = self.bus_read_word(addr & 0xFF_FFFF);
        let lo = self.bus_read_word(addr.wrapping_add(2) & 0xFF_FFFF);
        (u32::from(hi) << 16) | u32::from(lo)
    }

    fn bus_read_word(&self, addr24: u32) -> u16 {
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.peek(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.peek(reg));
        }
        // Zorro-II autoconfig probe window.
        if (AUTOCONFIG_BASE..AUTOCONFIG_TOP).contains(&addr24) {
            if let Some(board) = &self.autoconfig {
                return board.read_word((addr24 - AUTOCONFIG_BASE) as u16);
            }
            return 0xFFFF;
        }
        // Fast-RAM window served by a configured autoconfig board.
        if let Some(val) = self.autoconfig_fast_ram_read_word(addr24) {
            return val;
        }
        if self.gary.decode(addr24) == ChipSelect::Rtc {
            return self.rtc.read_word(addr24);
        }
        if let Some(offset) = self.custom_offset_for_addr(addr24) {
            return match offset {
                0x002 => self.agnus.dmaconr(),
                0x004 => self.agnus.vposr(),
                0x006 => self.agnus.vhposr(),
                0x00A => joydat(self.joy0_x, self.joy0_y),
                0x00C => joydat(self.joy1_x, self.joy1_y),
                // CLXDAT — debug peek, non-clearing (the real CPU read
                // clears on read via `dispatch_custom_register`).
                0x00E => self.denise.peek_clxdat(),
                // Paula-owned read-side registers.
                0x01C => self.paula.intena(),
                0x01E => self.paula.intreq(),
                0x010 => self.paula.adkcon(),
                0x01A => self.paula.peek_dskbytr(self.agnus.dmacon),
                0x018 => self.paula.peek_serdatr(),
                // Paula POT read registers (read-only).
                0x012 => self.paula.pot0dat(),
                0x014 => self.paula.pot1dat(),
                0x016 => self.paula.peek_potgor(),
                0x0A0..=0x0DA => paula_decode::audio_register(offset)
                    .map(|(ch, f)| self.paula.read_audio(ch, f))
                    .unwrap_or(0xFFFF),
                _ => 0xFFFF,
            };
        }
        self.memory.read_word(addr24)
    }

    /// Read a chip-RAM byte directly, ignoring the OVL overlay.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        self.memory.read_chip_ram_byte(addr)
    }

    /// Tick one primary period — master/4 = 68000 CPU clock = lores
    /// pixel rate (7.09 MHz PAL). This is the finest granularity in
    /// the machine; everything coarser (CCK, CIA E-clock, 68000 bus
    /// cycle) derives from it.
    ///
    /// Two ticks make one Agnus CCK, so chip-side events that the HRM
    /// describes at CCK granularity (beam advance, copper fetch slot,
    /// bitplane fetch, shift-register reload) fire on alternate ticks
    /// (`cck_phase == 0`). Per-tick events (CPU clock, lores pixel
    /// output, CIA E-clock divisor, CPU bus service) fire every tick.
    /// One machine tick. The per-CCK body is the shared
    /// [`AmigaDriver::tick`]; this inherent method delegates so the many
    /// existing `AmigaOcs::tick` callers (tests, the `AmigaMachine` impl,
    /// MCP) keep working unchanged.
    pub fn tick(&mut self) {
        <Self as AmigaDriver>::tick(self);
    }

    /// CIA-A is wired to the low data byte (D0-D7) at `$BFExxx`. The
    /// chip side-effects on every access — reading ICR clears its
    /// flags — so the dispatcher routes all reads (byte or word, any
    /// parity) through the chip and lets `BusResponse::Byte` deliver
    /// the value.
    fn dispatch_cia_a(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        let reg = cia::decode_cia_a(tx.addr)?;
        Some(if tx.is_read {
            BusResponse::Byte(self.cia_a.read(reg))
        } else {
            self.debug_cia_a_cr_log.push((
                self.tick_count / TICKS_PER_CCK,
                self.cpu.regs.pc,
                reg,
                tx.data as u8,
            ));
            self.cia_a.write(reg, tx.data as u8);
            self.memory.set_overlay(self.cia_a.ovl());
            BusResponse::WriteAck
        })
    }

    /// CIA-B sits on the high data byte (D8-D15) at `$BFDxxx`. Word
    /// writes deliver the data via the high byte; byte writes deliver
    /// via the low byte (LDS-side). Reads are byte-wide, like CIA-A.
    fn dispatch_cia_b(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        let reg = cia::decode_cia_b(tx.addr)?;
        Some(if tx.is_read {
            BusResponse::Byte(self.cia_b.read(reg))
        } else {
            let byte = if tx.is_word {
                (tx.data >> 8) as u8
            } else {
                tx.data as u8
            };
            self.cia_b.write(reg, byte);
            if matches!(reg, 0x01 | 0x03) {
                self.apply_df0_control_from_cia_b();
            }
            BusResponse::WriteAck
        })
    }

    /// Old-address battery-backed RTC at `$DC0000-$DC003F`. Word
    /// accesses route through `read_word` / `write_word`; byte
    /// accesses through `read_byte` / `write_byte`. Either path logs
    /// the access for the `amiga.debug.rtc` query surface.
    fn dispatch_rtc(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        if self.gary.decode(tx.addr) != ChipSelect::Rtc {
            return None;
        }
        Some(if tx.is_read {
            let val = if tx.is_word {
                self.rtc.read_word(tx.addr)
            } else {
                u16::from(self.rtc.read_byte(tx.addr))
            };
            self.log_rtc_access(tx.addr, true, tx.is_word, val);
            if tx.is_word {
                BusResponse::Word(val)
            } else {
                BusResponse::Byte(val as u8)
            }
        } else {
            self.log_rtc_access(tx.addr, false, tx.is_word, tx.data);
            if tx.is_word {
                self.rtc.write_word(tx.addr, tx.data);
            } else {
                self.rtc.write_byte(tx.addr, tx.data as u8);
            }
            BusResponse::WriteAck
        })
    }

    /// Zorro-II autoconfig probe window `$E80000-$E8007F`. The board
    /// drives every nibble on D15-D12, so byte writes at even
    /// addresses deliver their data via the high byte — mirror it
    /// into the high half before handing the word to the board.
    fn dispatch_autoconfig(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        if !(AUTOCONFIG_BASE..AUTOCONFIG_TOP).contains(&tx.addr) {
            return None;
        }
        let offset = (tx.addr - AUTOCONFIG_BASE) as u16;
        Some(if tx.is_read {
            let val = self
                .autoconfig
                .as_ref()
                .map_or(0xFFFF, |b| b.read_word(offset));
            BusResponse::Word(val)
        } else {
            if let Some(board) = self.autoconfig.as_mut() {
                let written = if tx.is_word {
                    tx.data
                } else if tx.addr & 1 == 0 {
                    (tx.data & 0xFF) << 8
                } else {
                    tx.data & 0xFF
                };
                board.write_word(offset, written);
            }
            BusResponse::WriteAck
        })
    }

    /// Fast-RAM window served by the configured autoconfig board.
    /// Checked before custom-register / memory dispatch so writes
    /// land in the board's backing store.
    fn dispatch_fast_ram(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        if tx.is_read {
            let word = self.autoconfig_fast_ram_read_word(tx.addr)?;
            if tx.is_word {
                Some(BusResponse::Word(word))
            } else {
                let byte = self.autoconfig_fast_ram_read_byte(tx.addr).unwrap_or(0);
                Some(BusResponse::Byte(byte))
            }
        } else {
            let absorbed = if tx.is_word {
                self.autoconfig_fast_ram_write_word(tx.addr, tx.data)
            } else {
                self.autoconfig_fast_ram_write_byte(tx.addr, tx.data as u8)
            };
            absorbed.then_some(BusResponse::WriteAck)
        }
    }

    /// Custom-register space (`$DFF000-$DFFFFE`) dispatches to the
    /// chipset module. DSKBYTR has a read side-effect; everything
    /// else is pure-read or routed via `dispatch_custom_write`.
    fn dispatch_custom_register(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        let offset = self.custom_offset_for_addr(tx.addr)?;
        Some(if tx.is_read {
            *self.debug_reg_read_counts.entry(offset).or_insert(0) += 1;
            let val = match offset {
                0x004 => self.agnus.vposr(),
                0x006 => self.agnus.vhposr(),
                0x00A => joydat(self.joy0_x, self.joy0_y),
                0x00C => joydat(self.joy1_x, self.joy1_y),
                // CLXDAT — latched sprite/playfield collisions, cleared on read.
                0x00E => self.denise.read_clxdat(),
                0x01C => self.paula.intena(),
                0x01E => self.paula.intreq(),
                0x010 => self.paula.adkcon(),
                0x01A => self.paula.read_dskbytr(self.agnus.dmacon),
                0x018 => self.paula.read_serdatr(),
                0x012 => self.paula.pot0dat(),
                0x014 => self.paula.pot1dat(),
                0x016 => self.paula.peek_potgor(),
                0x002 => self.agnus.dmaconr(),
                // DENISEID at $07C. OCS Denise 8362 has no version
                // register — `denise.deniseid()` returns `$FFFF` (open
                // bus). Wired explicitly so the intent is visible at
                // the dispatch table; KS 2.x / 3.x compares this value
                // against ECS ($FFFC) and AGA ($FFF8) markers.
                0x07C => self.denise.deniseid(),
                0x0A0..=0x0DA => paula_decode::audio_register(offset)
                    .map(|(ch, f)| self.paula.read_audio(ch, f))
                    .unwrap_or(0xFFFF),
                _ => 0xFFFF,
            };
            if self.debug_reg_read_log.len() < 262144 {
                self.debug_reg_read_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    offset,
                    val,
                ));
            }
            BusResponse::Word(val)
        } else {
            if self.debug_custom_write_log.len() < 1_048_576 {
                self.debug_custom_write_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    tx.addr,
                    offset,
                    tx.data,
                    tx.is_word,
                ));
            }
            self.dispatch_custom_write(offset, tx.data);
            BusResponse::WriteAck
        })
    }

    /// Memory fallback: chip RAM (with OVL gating reads to ROM), slow
    /// RAM, ROM, and unmapped reads (floating bus) all live here.
    /// Writes go through the watch-range diagnostic before landing.
    fn dispatch_memory(&mut self, tx: &BusTransaction) -> BusResponse {
        if tx.is_read {
            if tx.is_word {
                BusResponse::Word(self.memory.read_word(tx.addr))
            } else {
                BusResponse::Byte(self.memory.read_byte(tx.addr))
            }
        } else {
            if let Some((lo, len)) = self.debug_watch_addr {
                let hi = lo.wrapping_add(len);
                let access_len = if tx.is_word { 2u32 } else { 1 };
                let access_hi = tx.addr.wrapping_add(access_len);
                if tx.addr < hi && access_hi > lo {
                    self.debug_watch_writes.push((
                        self.tick_count / TICKS_PER_CCK,
                        self.cpu.regs.pc,
                        tx.addr,
                        tx.data,
                        tx.is_word,
                    ));
                }
            }
            if tx.is_word {
                self.memory.write_word(tx.addr, tx.data);
            } else {
                self.memory.write_byte(tx.addr, tx.data as u8);
            }
            BusResponse::WriteAck
        }
    }

    /// Build a persistable snapshot of the live machine state.
    ///
    /// Diagnostic logs (`debug_*` fields) are intentionally excluded —
    /// they are observability, not state. The inserted disk is also
    /// excluded; the runtime envelope is responsible for re-inserting
    /// disk media on restore.
    #[must_use]
    pub fn snapshot_state(&self) -> AmigaOcsSnapshot {
        AmigaOcsSnapshot {
            cpu: self.cpu.clone(),
            memory: self.memory.clone(),
            drive: self.drive.clone(),
            track_cache: self.track_cache.clone(),
            track_word_cursor: self.track_word_cursor,
            track_pacer: self.track_pacer,
            keyboard: self.keyboard.clone(),
            prev_cia_a_spmode: self.prev_cia_a_spmode,
            gary: self.gary.clone(),
            rtc: self.rtc.clone(),
            autoconfig: self.autoconfig.clone(),
            cia_a: self.cia_a.clone(),
            cia_b: self.cia_b.clone(),
            paula: self.paula.clone(),
            joy0_x: self.joy0_x,
            joy0_y: self.joy0_y,
            joy1_x: self.joy1_x,
            joy1_y: self.joy1_y,
            port0_left_button_pressed: self.port0_left_button_pressed,
            port1_left_button_pressed: self.port1_left_button_pressed,
            joystick1: self.joystick1,
            agnus: self.agnus.clone(),
            copper: self.copper.clone(),
            denise: self.denise.clone(),
            tick_count: self.tick_count,
            cck_phase: self.cck_phase,
            prev_vertb_level: self.prev_vertb_level,
            prev_cia_a_irq: self.prev_cia_a_irq,
            prev_cia_b_irq: self.prev_cia_b_irq,
            e_clock_phase: self.e_clock_phase,
        }
    }

    /// Restore machine state from a snapshot. Diagnostic logs are
    /// cleared (snapshots do not preserve observability state). Disk
    /// media is not restored here — re-mount via `insert_disk` after
    /// restore.
    pub fn restore_snapshot_state(&mut self, snap: AmigaOcsSnapshot) {
        self.cpu = snap.cpu;
        self.memory = snap.memory;
        self.drive = snap.drive;
        self.track_cache = snap.track_cache;
        self.track_word_cursor = snap.track_word_cursor;
        self.track_pacer = snap.track_pacer;
        self.keyboard = snap.keyboard;
        self.prev_cia_a_spmode = snap.prev_cia_a_spmode;
        self.gary = snap.gary;
        self.rtc = snap.rtc;
        self.autoconfig = snap.autoconfig;
        self.cia_a = snap.cia_a;
        self.cia_b = snap.cia_b;
        self.paula = snap.paula;
        self.joy0_x = snap.joy0_x;
        self.joy0_y = snap.joy0_y;
        self.joy1_x = snap.joy1_x;
        self.joy1_y = snap.joy1_y;
        self.port0_left_button_pressed = snap.port0_left_button_pressed;
        self.port1_left_button_pressed = snap.port1_left_button_pressed;
        self.joystick1 = snap.joystick1;
        self.agnus = snap.agnus;
        self.copper = snap.copper;
        self.denise = snap.denise;
        self.tick_count = snap.tick_count;
        self.cck_phase = snap.cck_phase;
        self.prev_vertb_level = snap.prev_vertb_level;
        self.prev_cia_a_irq = snap.prev_cia_a_irq;
        self.prev_cia_b_irq = snap.prev_cia_b_irq;
        self.e_clock_phase = snap.e_clock_phase;

        self.debug_reg_read_counts.clear();
        self.debug_peak_intena = 0;
        self.debug_intena_writes = 0;
        self.debug_intena_log.clear();
        self.debug_cop1lc_log.clear();
        self.debug_cop2lc_log.clear();
        self.debug_dsk_log.clear();
        self.debug_dmacon_log.clear();
        self.debug_blit_starts = 0;
        self.debug_blit_log.clear();
        self.debug_cia_a_cr_log.clear();
        self.debug_cia_b_cr_log.clear();
        self.debug_copper_move_log.clear();
        self.debug_custom_write_log.clear();
        self.debug_watch_addr = None;
        self.debug_watch_writes.clear();
        self.debug_rtc_log.clear();
        self.debug_bplcon0_log.clear();
        self.debug_palette_log.clear();
        self.debug_reg_read_log.clear();
    }
}

// The shared per-CCK driver (#34). The `tick` / `service_cpu_bus` /
// `apply_bus_response` bodies live as provided defaults on `AmigaDriver`
// in `common-commodore-amiga`; this impl supplies the OCS accessors,
// targeted multi-borrow operations, scalar bookkeeping, and the
// variant-specific helpers (no Gayle arm; stock Cpu68000).
impl AmigaDriver for AmigaOcs {
    fn agnus(&self) -> &Agnus {
        &self.agnus
    }
    fn agnus_mut(&mut self) -> &mut Agnus {
        &mut self.agnus
    }
    fn copper(&self) -> &Copper {
        &self.copper
    }
    fn copper_mut(&mut self) -> &mut Copper {
        &mut self.copper
    }
    fn paula(&self) -> &Paula8364 {
        &self.paula
    }
    fn paula_mut(&mut self) -> &mut Paula8364 {
        &mut self.paula
    }
    fn cia_a(&self) -> &Cia {
        &self.cia_a
    }
    fn cia_a_mut(&mut self) -> &mut Cia {
        &mut self.cia_a
    }
    fn cia_b(&self) -> &Cia {
        &self.cia_b
    }
    fn cia_b_mut(&mut self) -> &mut Cia {
        &mut self.cia_b
    }
    fn drive(&self) -> &AmigaFloppyDrive {
        &self.drive
    }
    fn drive_mut(&mut self) -> &mut AmigaFloppyDrive {
        &mut self.drive
    }
    fn keyboard_mut(&mut self) -> &mut AmigaKeyboard {
        &mut self.keyboard
    }
    fn memory(&self) -> &Memory {
        &self.memory
    }
    fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }
    fn cpu_base(&self) -> &motorola_68000::Cpu68000 {
        &self.cpu
    }
    fn cpu_base_mut(&mut self) -> &mut motorola_68000::Cpu68000 {
        &mut self.cpu
    }

    fn copper_tick_cck(
        &mut self,
        vpos: u16,
        hpos: u16,
        copper_slot_granted: bool,
        blitter_busy: bool,
    ) -> Option<(u16, u16)> {
        self.copper
            .tick_cck(&self.memory, vpos, hpos, copper_slot_granted, blitter_busy)
    }

    fn blitter_dma_step(&mut self) -> bool {
        let mut bus = ChipRamBus(&mut self.memory);
        self.agnus.tick_blitter_dma(&mut bus)
    }

    fn audio_tick_cck(&mut self, dmacon: u16, slot: Option<u8>) {
        // Memory borrow lives outside the closure so the closure borrows
        // only Memory, not all of Self.
        let memory = &self.memory;
        self.paula
            .tick_audio_cck(dmacon, slot, true, |addr| memory.read_chip_ram_byte(addr));
    }

    fn service_sprite_dma(&mut self, channel: u8, second_word: bool, width: u8) {
        let memory = &self.memory;
        let fetched =
            self.agnus
                .service_sprite_dma_cyc(channel as usize, second_word, width, |addr| {
                    memory.read_chip_ram_word(addr)
                });
        if let Some((is_control, value)) = fetched {
            let channel = channel as usize;
            if is_control {
                // SPRxPOS / SPRxCTL — 16-bit, via the unified dispatch.
                let reg = 0x140 + (channel as u16) * 8 + if second_word { 2 } else { 0 };
                self.denise.write_word(reg, value as u16);
            } else if second_word {
                // SPRxDATB — full FMODE width into the serial shifter.
                self.denise.ocs.write_sprite_datb_wide(channel, value);
            } else {
                // SPRxDATA — full FMODE width into the serial shifter.
                self.denise.ocs.write_sprite_data_wide(channel, value);
            }
        }
    }

    fn denise_tick(&mut self, phase: u8) {
        let vpos = self.agnus.vpos;
        let hpos = self.agnus.hpos;
        let dmacon = self.agnus.dmacon;
        self.denise
            .tick(phase, vpos, hpos, dmacon, &mut self.agnus, &self.memory);
    }

    fn cck_phase(&self) -> u8 {
        self.cck_phase
    }
    fn set_cck_phase(&mut self, value: u8) {
        self.cck_phase = value;
    }
    fn prev_vertb_level(&self) -> bool {
        self.prev_vertb_level
    }
    fn set_prev_vertb_level(&mut self, value: bool) {
        self.prev_vertb_level = value;
    }
    fn prev_cia_a_spmode(&self) -> bool {
        self.prev_cia_a_spmode
    }
    fn set_prev_cia_a_spmode(&mut self, value: bool) {
        self.prev_cia_a_spmode = value;
    }
    fn prev_cia_a_irq(&self) -> bool {
        self.prev_cia_a_irq
    }
    fn set_prev_cia_a_irq(&mut self, value: bool) {
        self.prev_cia_a_irq = value;
    }
    fn prev_cia_b_irq(&self) -> bool {
        self.prev_cia_b_irq
    }
    fn set_prev_cia_b_irq(&mut self, value: bool) {
        self.prev_cia_b_irq = value;
    }
    fn e_clock_phase(&self) -> u64 {
        self.e_clock_phase
    }
    fn set_e_clock_phase(&mut self, value: u64) {
        self.e_clock_phase = value;
    }
    fn track_pacer(&self) -> u16 {
        self.track_pacer
    }
    fn set_track_pacer(&mut self, value: u16) {
        self.track_pacer = value;
    }
    fn tick_count(&self) -> u64 {
        self.tick_count
    }
    fn set_tick_count(&mut self, value: u64) {
        self.tick_count = value;
    }
    fn push_copper_move_log(&mut self, entry: common_commodore_amiga::driver::CopperMoveLogEntry) {
        self.debug_copper_move_log.push(entry);
    }

    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        AmigaOcs::dispatch_custom_write(self, offset, val);
    }
    fn feed_next_write_word(&mut self) {
        AmigaOcs::feed_next_write_word(self);
    }
    fn feed_next_mfm_word(&mut self) {
        AmigaOcs::feed_next_mfm_word(self);
    }
    fn disk_word_cck_interval(&self) -> u16 {
        AmigaOcs::disk_word_cck_interval(self)
    }
    fn refresh_cia_a_external_inputs(&mut self) {
        AmigaOcs::refresh_cia_a_external_inputs(self);
    }

    fn tick_cpu_with_ipl(&mut self) {
        self.cpu.ipl = self.paula.compute_ipl();
        self.cpu.tick();
    }

    fn dispatch_bus(&mut self, tx: &BusTransaction) -> BusResponse {
        self.dispatch_cia_a(tx)
            .or_else(|| self.dispatch_cia_b(tx))
            .or_else(|| self.dispatch_rtc(tx))
            .or_else(|| self.dispatch_autoconfig(tx))
            .or_else(|| self.dispatch_fast_ram(tx))
            .or_else(|| self.dispatch_custom_register(tx))
            .unwrap_or_else(|| self.dispatch_memory(tx))
    }
}
