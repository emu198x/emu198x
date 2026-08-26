//! Commodore Amiga A1200 (AGA chipset) machine — Stage A of the
//! A1200 rollout per `knowledge/decisions/amiga-machine-rollout-plan.md`.
//!
//! Wires the AGA chipset chips (Alice = `commodore-agnus-aga`,
//! Lisa = `commodore-denise-aga`) into the common Amiga board
//! substrate alongside Gayle (IDE / PCMCIA address decoder) at
//! `$D80000-$DFFFFF`. The stock MC68EC020 is held in the shared
//! [`ActiveCpu`] runtime-selection type and receives two input-clock
//! edges for each 7 MHz Amiga system tick.
//!
//! Structurally this is a parallel of `machine-commodore-amiga-ecs`
//! with the chipset types swapped (`AgnusAga` for `AgnusEcs`,
//! `DeniseAga` for `DeniseEcs`) and Gayle wired into the bus path.
//! `AgnusAga` Derefs to `AgnusEcs` which Derefs to OCS Agnus, so the
//! board-level wiring continues to pass through unchanged.

mod agnus;
mod denise;
use std::collections::VecDeque;

use common_commodore_amiga::board::{
    BusResponse, BusTransaction, SizedBusResponse, SizedBusTransaction, TICKS_PER_CCK,
    WatchingChipRamBus,
};
use common_commodore_amiga::driver::{AmigaDriver, CpuBoundary};
pub use common_commodore_amiga::{
    AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY, AmigaInputDiagnosticSnapshot, AmigaMemoryWriteRecord,
    AmigaMemoryWriteSource, AmigaSchedulerDiagnosticSnapshot, AmigaTrackStreamDiagnosticSnapshot,
};
use common_commodore_amiga::{ActiveCpu, CpuClock, CpuDomainPhase, cia, copper, memory, rtc};

pub use agnus::{
    Agnus, AgnusAga, AgnusEcs, AgnusRegion, BlitterCckOutcome, CckBusPlan, NTSC_CCKS_PER_FRAME,
    NTSC_FRAME_TICKS, NTSC_LINES_PER_FRAME, PAL_CCKS_PER_FRAME, PAL_FRAME_LINES, PAL_FRAME_TICKS,
    PAL_LINE_CCKS, PAL_LINE_TICKS, PAL_LINES_PER_FRAME, SlotOwner, VBL_END_LINE, bits,
};
pub use cia::{Cia, CiaExt};
pub use commodore_amiga_autoconfig::{AutoconfigBoard, AutoconfigState};
pub use commodore_gary::{ChipSelect, Gary};
pub use commodore_gayle::{Gayle, GayleDiagnosticSnapshot};
pub use copper::Copper;
pub use denise::{Denise, FB_HEIGHT, FB_WIDTH};
use emu198x_commodore_paula_8364::bits::{
    POTGOR_BTN_PORT0_MIDDLE, POTGOR_BTN_PORT0_RIGHT, POTGOR_BTN_PORT1_MIDDLE,
    POTGOR_BTN_PORT1_RIGHT,
};
use emu198x_commodore_paula_8364::decode as paula_decode;
pub use emu198x_commodore_paula_8364::{
    AudioControls, AudioField, IntSource, Paula8364, PaulaChannel,
};
pub use format_commodore_amiga_adf::Adf;
pub use memory::{CHIP_RAM_SIZE, DEFAULT_CHIP_RAM_SIZE, Memory};
pub use peripheral_commodore_amiga_floppy::{AmigaFloppyDrive, DriveStatus};
pub use peripheral_commodore_amiga_keyboard::AmigaKeyboard;
pub use rtc::{Msm6242RtcDiagnosticSnapshot, RTC_BASE};

use motorola_68000::bus::{
    DataPortSize, dynamic_transfer_bytes, extract_dynamic_bus_data, place_dynamic_read_data,
};
use motorola_68020::Cpu68020;
use rtc::Msm6242Rtc;

const CUSTOM_BASE: u32 = 0x00DF_0000;
const CUSTOM_TOP: u32 = 0x00E0_0000;
const SLOW_RAM_BASE: u32 = 0x00C0_0000;
/// Zorro-II autoconfig probe window — the first unconfigured board
/// answers here until `expansion.library` writes its base-address
/// pair to `$E8004A` / `$E80048`.
const AUTOCONFIG_BASE: u32 = 0x00E8_0000;
const AUTOCONFIG_TOP: u32 = 0x00E8_0080;

/// RAM layout for an Amiga instance.
///
/// Chip RAM lives at `$000000` and is required. Slow RAM is the A501-
/// style trapdoor expansion at `$C00000`. Fast RAM is a Zorro-II
/// autoconfig board; `fast_kb` keeps the runtime preset surface stable across
/// the autoconfig wiring.
///
/// One entry in the diagnostic blit log.
pub type BlitLogEntry = (u64, u32, u16, u16, u32, u32, u32, u32, u16);

// `RamConfig` is intentionally re-exported from
// `machine-commodore-amiga-ocs` rather than duplicated here. This
// keeps the type identity shared so `runtime-commodore-amiga` (or
// any other caller) can pass a single `RamConfig` value through both
// `AmigaOcs::with_ram_config` and `AmigaA1200::with_ram_config`. The
// memory layout, autoconfig wiring, and presets are identical for
// the A500-family configurations both machines share.
pub use machine_commodore_amiga_ocs::RamConfig;

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

/// Amiga A1200 (AGA chipset) machine.
pub struct AmigaA1200 {
    cpu: ActiveCpu,
    /// Exact active-CPU edges emitted per Amiga system tick.
    cpu_clock: CpuClock,
    /// Unconsumed CPU edges when exact instruction stepping stops part-way
    /// through one Amiga system tick.
    cpu_domain_phase: CpuDomainPhase,
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
    /// style configurations. Advances from completed emulated system ticks.
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
    agnus: AgnusAga,
    copper: Copper,
    denise: Denise,
    /// Gayle gate array — IDE task-file decode at `$DA0000-$DA3FFF`,
    /// Gayle control registers at `$DA8000-$DABFFF`, PCMCIA slot
    /// routing. Stage A: no IDE drive, no PCMCIA card; KS 3.x reads
    /// `$7F` from STATUS and `$FF` from the other IDE registers.
    gayle: Gayle,
    tick_count: u64,
    /// Sub-CCK phase: 0 at the first tick of a CCK (fetch/reload
    /// events fire here), 1 at the second tick. Flips each tick.
    cck_phase: u8,
    /// Previous vertical-blank interval state. The transition into
    /// blanking generates the once-per-frame `VERTB` request. Other
    /// frame-start events use their own beam predicates.
    prev_vertb_level: bool,
    /// Last sampled CIA-A interrupt-input state. Retained in machine
    /// snapshots; Paula request generation is level-sensitive.
    prev_cia_a_irq: bool,
    /// Last sampled CIA-B interrupt-input state.
    prev_cia_b_irq: bool,
    e_clock_phase: u64,
    /// Bounded instruction-boundary observations. This diagnostic queue is
    /// intentionally excluded from machine snapshots.
    cpu_boundaries: VecDeque<CpuBoundary>,
    /// Diagnostic: count of unique custom-register read offsets seen
    /// since reset, indexed by offset / 2.
    pub debug_reg_read_counts: std::collections::HashMap<u16, u64>,
    /// Diagnostic: full log of CPU chipset-register reads. Entry is
    /// `(cck, pc, offset, value_returned)`. Captures every CPU read
    /// from `$DFFxxx` so we can see what value KS observed for each
    /// chipset register at each query point. Useful for finding
    /// AGA-detection probes that depend on read-side values.
    pub debug_reg_read_log: Vec<(u64, u32, u16, u16)>,
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
    /// Diagnostic: log of BPLCON0 writes. Entry is `(cck, pc,
    /// val)`. Every write to $DFF100 — by the CPU or the copper —
    /// is captured so we can see whether KS ever sets BPU > 0.
    pub debug_bplcon0_log: Vec<(u64, u32, u16)>,
    /// Diagnostic: log of palette-touching writes. Each entry is
    /// `(cck, pc, offset, val, bplcon3_at_write)`. Captures every
    /// write to COLOR00..COLOR31 ($180..$1BE) together with the
    /// BPLCON3 BANK + LOCT state at the time, plus every BPLCON3
    /// write itself ($106). Lets us reconstruct the full AGA
    /// palette-programming sequence KS uses to set up Workbench.
    pub debug_palette_log: Vec<(u64, u32, u16, u16, Option<u16>)>,
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
    /// Stage O: CIA-A read counts per register. Surfaces which CIA-A
    /// registers KS polls when boot wedges in trackdisk / scheduler
    /// loops. The hottest read is usually PRA ($00 → drive status
    /// bits + FIR0/FIR1) when KS is polling DSKCHANGE / DSKRDY /
    /// TRACK0 for the floppy. ICRR ($0D) reads are also revealing —
    /// CIA timer interrupts route through here.
    pub debug_cia_a_read_counts: std::collections::HashMap<u8, u64>,
    /// Stage O: CIA-B read counts per register. Mirrors `cia_a_read`.
    /// CIA-B drives floppy step / motor / select via PRB and the
    /// disk-step timer.
    pub debug_cia_b_read_counts: std::collections::HashMap<u8, u64>,
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
    /// Diagnostic: half-open memory range observed by both the legacy
    /// CPU-only tuple stream and the source-aware all-writer stream.
    pub debug_watch_addr: Option<(u32, u32)>,
    /// Source-aware write stream used by the shared runtime watch. CPU entries
    /// mirror `debug_watch_writes`; blitter D and disk read-DMA writes appear
    /// only here.
    pub debug_memory_watch_writes: Vec<AmigaMemoryWriteRecord>,
    /// Legacy CPU-only tuple stream retained for existing diagnostics.
    pub debug_watch_writes: Vec<(u64, u32, u32, u16, bool)>,
    /// Diagnostic: bounded log of CPU RTC bus accesses. Entry is
    /// `(cck, pc, addr24, is_read, is_word, value)`, where `value`
    /// is the delivered word/byte payload. Used to trace KS 1.3's
    /// direct old-address clock probes at `$DC0000`.
    pub debug_rtc_log: Vec<(u64, u32, u32, bool, bool, u16)>,
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
pub struct AmigaA1200Snapshot {
    cpu: ActiveCpu,
    cpu_clock: CpuClock,
    cpu_domain_phase: CpuDomainPhase,
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
    agnus: AgnusAga,
    copper: Copper,
    denise: Denise,
    gayle: Gayle,
    tick_count: u64,
    cck_phase: u8,
    prev_vertb_level: bool,
    prev_cia_a_irq: bool,
    prev_cia_b_irq: bool,
    e_clock_phase: u64,
}

impl AmigaA1200 {
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
        let mut cpu = ActiveCpu::M68EC020(Cpu68020::new());
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
        gary.set_gayle_present(true);
        Self {
            cpu,
            cpu_clock: CpuClock::from_ratio(2, 1),
            cpu_domain_phase: CpuDomainPhase::default(),
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
            agnus: {
                // VPOSR bits 14-8 carry the Agnus revision id. KS reads
                // this to discriminate OCS / ECS / AGA Agnus chips. The
                // correct AGA Alice value is $23 (PAL) / $33 (NTSC) —
                // see WinUAE custom.cpp ~line 2535 (AGA sets bits
                // 0x2300, NTSC adds 0x1000). Stages AC / AD set that.
                //
                // But reporting full-AGA Alice puts KS 3.1 on a
                // Workbench boot path we can't yet complete: WB never
                // installs its view, the cop2lc list at $121E0 stays
                // pointing at KS's empty "Insert disk" bitmap, and the
                // user sees just disk-icon outlines. Empirically (Stage
                // AE investigation, 2026-05-25):
                //   - HIRES rendering is correct (poke test pixel-perfect)
                //   - chip RAM 512 KB / 2 MB / 2 MB + 4 MB fast: identical
                //   - cop2lc never switches; WB has not built its own
                //   - disk stops at cylinder 40 in both OCS-fallback and AGA
                //
                // Report the real AGA Alice identifier so KS 3.x
                // takes the full-AGA boot path. The Stage AE handoff
                // documented that this previously made WB not install
                // its view; Stage AE-k added ECS blitter extension
                // support (BLTCON0L / BLTSIZV / BLTSIZH) which the
                // KS 3.x WB-install code path likely uses too, and
                // AE-j wired DENISEID to return $FFF8 — so the full
                // AGA chipset identification chain is now honest.
                //
                // If WB still fails to install its view after this
                // change, the bug is something other than blitter
                // registers and chipset ID — the cpu_trace tooling
                // added in Stage AE-i is the next investigation step.
                let mut chained =
                    AgnusAga::from_ecs(AgnusEcs::from_ocs(Agnus::new_with_region(region)));
                chained.agnus_id = match region {
                    AgnusRegion::Pal => 0x2300,
                    AgnusRegion::Ntsc => 0x3300,
                };
                chained
            },
            copper: Copper::new(),
            denise: Denise::new(),
            gayle: Gayle::new(),
            tick_count: 0,
            cck_phase: 0,
            // Initialise as `true` because at reset the beam is at
            // vpos=0 (inside the VBL window), so the level signal is
            // already high. A `false` initial value would fake a
            // rising edge on the first tick and spuriously fire VERTB
            // before the first real frame.
            prev_vertb_level: true,
            prev_cia_a_irq: false,
            prev_cia_b_irq: false,
            e_clock_phase: 0,
            cpu_boundaries: VecDeque::new(),
            debug_reg_read_counts: std::collections::HashMap::new(),
            debug_reg_read_log: Vec::new(),
            debug_peak_intena: 0,
            debug_intena_writes: 0,
            debug_intena_log: Vec::new(),
            debug_cop1lc_log: Vec::new(),
            debug_cop2lc_log: Vec::new(),
            debug_dsk_log: Vec::new(),
            debug_dmacon_log: Vec::new(),
            debug_bplcon0_log: Vec::new(),
            debug_palette_log: Vec::new(),
            debug_blit_starts: 0,
            debug_blit_log: Vec::new(),
            debug_cia_a_cr_log: Vec::new(),
            debug_cia_a_read_counts: std::collections::HashMap::new(),
            debug_cia_b_read_counts: std::collections::HashMap::new(),
            debug_cia_b_cr_log: Vec::new(),
            debug_copper_move_log: Vec::new(),
            debug_custom_write_log: Vec::new(),
            debug_watch_addr: None,
            debug_memory_watch_writes: Vec::new(),
            debug_watch_writes: Vec::new(),
            debug_rtc_log: Vec::new(),
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

    /// Direct read-only access to the AGA Alice wrapper and its enhanced
    /// timing state.
    #[must_use]
    pub const fn agnus_aga(&self) -> &AgnusAga {
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

    /// Direct access to the underlying AGA Lisa chip — exposes
    /// BPLCON3 / BPLCON4 / palette_24 / sprite-width state that the
    /// generic `Denise` wrapper doesn't surface.
    #[must_use]
    pub fn denise_aga(&self) -> &commodore_denise_aga::DeniseAga {
        &self.denise.ocs
    }

    /// Complete side-effect-free Gayle register, interrupt, IDE, PCMCIA and
    /// address-decoder state.
    #[must_use]
    pub const fn gayle_diagnostic_snapshot(&self) -> GayleDiagnosticSnapshot {
        self.gayle.diagnostic_snapshot()
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

    /// Side-effect-free state of the board's battery-backed clock.
    #[must_use]
    pub fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        self.rtc.diagnostic_snapshot()
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
    /// pending (a "newly inserted" disk); `false` acknowledges it ("disk
    /// ready"). `writable = false` mounts read-only — an archive that
    /// reports `/DSKPROT` and rejects a SAVE (#97). The three `insert_adf*`
    /// helpers are thin presets over this.
    pub fn mount_adf(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        self.drive.insert_disk_writable(adf, writable);
        self.track_cache = None;
        if !change_pending {
            self.drive.acknowledge_disk_change();
        }
        self.refresh_cia_a_external_inputs();
    }

    /// Reconnect snapshot-owned DF0 media without generating a physical
    /// insertion event or invalidating the restored encoded-track stream.
    pub fn reattach_adf(&mut self, adf: Adf, writable: bool) {
        self.drive.reattach_disk_writable(adf, writable);
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
        self.track_cache = None;
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

    /// Push the next rotational MFM word from the drive's encoded track
    /// buffer into Paula's disk FIFO. Re-encodes the track when the
    /// drive head has moved since the last word, or when no cache
    /// exists yet. Cache replacement preserves the spindle's word phase
    /// modulo the new track length. Rotates the cursor back to 0 at end of
    /// track so successive revolutions keep delivering words.
    ///
    /// This advances the rotating stream and DSKBYTR/DSKDATR state only.
    /// Agnus-granted cells independently move queued words to chip RAM.
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
            let word_count = bytes.len() / 2;
            self.track_word_cursor = if word_count == 0 {
                0
            } else {
                self.track_word_cursor % word_count
            };
            self.track_cache = Some((cyl, head, bytes));
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

        self.paula.receive_disk_read_word(word);
    }

    /// Drain one rotational write-stream word from Paula's FIFO to the drive.
    fn feed_next_write_word(&mut self) {
        if let Some(write_word) = self.paula.take_disk_write_stream_word() {
            self.drive.note_write_mfm_word(write_word);
            if !self.paula.disk_write_stream_active() && self.drive.flush_write_capture() > 0 {
                self.track_cache = None;
            }
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
        if (0x00DA_0000..0x00DC_0000).contains(&addr24) {
            self.gayle.write_word(addr24, val);
            return;
        }
        match self.gary.decode(addr24) {
            ChipSelect::Rtc => self.rtc.write_word(addr24, val),
            _ => self.memory.write_word(addr24, val),
        }
    }

    /// Dispatch a Copper MOVE before Lisa renders the current output tick.
    fn dispatch_copper_write(&mut self, offset: u16, val: u16) {
        if (0x0180..=0x01BE).contains(&offset) && offset.is_multiple_of(2) {
            self.denise.write_word_before_output_tick(offset, val);
            self.record_palette_write(offset, val);
        } else {
            self.dispatch_custom_write(offset, val);
        }
    }

    fn record_palette_write(&mut self, offset: u16, val: u16) {
        // Lisa's live BPLCON3 value identifies BANK and LOCT for COLOR writes
        // and the selector context for BPLCON3/BPLCON4 diagnostics.
        if (((0x180..=0x1BE).contains(&offset) && offset.is_multiple_of(2))
            || offset == 0x0106
            || offset == 0x010C)
            && self.debug_palette_log.len() < 262144
        {
            let bplcon3 = self.denise.ocs.bplcon3;
            self.debug_palette_log.push((
                self.tick_count / TICKS_PER_CCK,
                self.cpu.regs.pc,
                offset,
                val,
                Some(bplcon3),
            ));
        }
    }

    /// Dispatch a custom-register word write to the right submodule.
    /// Shared between `poke_word` and the CPU bus servicer.
    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        if self.agnus.write_timing_register(offset, val) {
            return;
        }
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
            // ECS-only blitter extension registers (also wired for AGA,
            // which Derefs through ECS). KS 3.x uses BLTCON0L for cheap
            // LF updates and BLTSIZV/BLTSIZH for many text + icon
            // blits; without these handlers WB content would never
            // render once we report a real ECS / AGA chipset.
            0x05A => {
                self.agnus.write_bltcon0l(val);
            }
            0x05C => {
                self.agnus.write_bltsizv(val);
            }
            0x05E => {
                // BLTSIZH starts the ECS/AGA large blit; `write_bltsizh`
                // arms the incremental scheduler. Each granted CCK then
                // consumes a startup outcome or one DMA operation in the
                // tick loop (#31) instead of completing here.
                self.agnus.write_bltsizh(val);
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
            // Agnus-owned blitter registers. BLTSIZE ($058) arms the
            // incremental scheduler via `start_blit`; each granted CCK
            // consumes a startup outcome or one DMA operation in the tick
            // loop (#31). Mid-blit writes land at their normally arbitrated
            // CCK; software must WaitBlit before changing active state.
            0x040..=0x074 => {
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
                // The register mirror is immediate; the ECSENA copy used by
                // Lisa's programmable blanking crosses normal output stages.
                self.denise.write_word(offset, val);
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
            // FMODE ($1FC) is owned by Alice (Agnus side) on real
            // AGA silicon — it controls 16/32/64-bit bitplane and
            // sprite DMA fetch widths. Lisa mirrors bits 3..2 to
            // derive sprite display width, so forward to Denise too.
            0x1FC => {
                self.agnus.fmode = val;
                // The bitplane/sprite fetch scheduler reads FMODE off the
                // inner OCS Agnus (the type Denise's fetch loop is handed),
                // so propagate it there too — not only the AGA wrapper copy
                // that query/diagnostics read.
                self.agnus.as_inner_mut().fmode = val;
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
        self.record_palette_write(offset, val);
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
        } else if (0x00DA_0000..0x00DC_0000).contains(&(addr & 0xFF_FFFF)) {
            self.gayle.write(addr, val);
        } else if self.gary.decode(addr) == ChipSelect::Rtc {
            self.rtc.write_byte(addr, val);
        } else {
            self.memory.write_byte(addr, val);
        }
    }

    /// Stock MC68EC020 wrapper (read-only — mutating outside the tick loop
    /// breaks invariants).
    #[must_use]
    pub fn cpu(&self) -> &Cpu68020 {
        match &self.cpu {
            ActiveCpu::M68EC020(cpu) | ActiveCpu::M68020(cpu) => cpu,
            _ => unreachable!("an A1200 machine must retain an MC68020-family processor"),
        }
    }

    /// Runtime-selected processor, including its concrete model identity.
    #[must_use]
    pub const fn active_cpu(&self) -> &ActiveCpu {
        &self.cpu
    }

    /// Exact active-CPU clock conversion, including serialized phase.
    #[must_use]
    pub const fn cpu_clock(&self) -> CpuClock {
        self.cpu_clock
    }

    /// Whether persisted partial CPU-domain progress is compatible with the
    /// configured CPU clock.
    #[must_use]
    pub fn cpu_domain_phase_is_coherent(&self) -> bool {
        self.cpu_domain_phase
            .snapshot_is_coherent(self.cpu_clock, self.tick_count)
    }

    /// Drain instruction boundaries retained since the previous observation.
    ///
    /// The queue is bounded and diagnostic-only; draining it does not change
    /// emulated machine state.
    pub fn drain_cpu_boundaries(&mut self) -> std::collections::vec_deque::Drain<'_, CpuBoundary> {
        self.cpu_boundaries.drain(..)
    }

    /// Total Amiga system ticks (master/4, the lores pixel rate) elapsed
    /// since construction. The stock MC68EC020 clock domain emits two
    /// processor edges per system tick without changing this counter.
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

    /// Side-effect-free board scheduler and CPU-domain state.
    #[must_use]
    pub fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot {
        AmigaSchedulerDiagnosticSnapshot {
            tick_count: self.tick_count,
            cck_count: self.cck_count(),
            cck_phase: self.cck_phase,
            e_clock_phase: self.e_clock_phase,
            prev_vertb_level: self.prev_vertb_level,
            prev_cia_a_irq: self.prev_cia_a_irq,
            prev_cia_b_irq: self.prev_cia_b_irq,
            prev_cia_a_spmode: self.prev_cia_a_spmode,
            cpu_clock_numerator: self.cpu_clock.numerator(),
            cpu_clock_denominator: self.cpu_clock.denominator(),
            cpu_clock_phase: self.cpu_clock.phase(),
            cpu_clock_maximum_edges_per_tick: self.cpu_clock.maximum_edges_per_tick(),
            cpu_domain_idle: self.cpu_domain_phase.is_idle(),
            cpu_domain_edges_remaining: self.cpu_domain_phase.edges_remaining(),
            cpu_domain_motherboard_slot_pending: self.cpu_domain_phase.motherboard_slot_pending(),
            cpu_domain_coherent: self.cpu_domain_phase_is_coherent(),
            pending_cpu_boundaries: self.cpu_boundaries.iter().copied().collect(),
            pending_cpu_boundary_capacity: AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY,
        }
    }

    /// Side-effect-free encoded-track cache and delivery-pacer state.
    #[must_use]
    pub fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot {
        let (cache_cylinder, cache_head, cache_bytes) = self
            .track_cache
            .as_ref()
            .map_or((None, None, 0), |(cylinder, head, bytes)| {
                (Some(*cylinder), Some(*head), bytes.len())
            });
        AmigaTrackStreamDiagnosticSnapshot {
            cache_present: self.track_cache.is_some(),
            cache_cylinder,
            cache_head,
            cache_bytes,
            word_count: cache_bytes / 2,
            word_cursor: self.track_word_cursor,
            pacer_ccks: self.track_pacer,
            word_interval_ccks: self.disk_word_cck_interval(),
        }
    }

    /// Side-effect-free controller-port counters and host input latches.
    #[must_use]
    pub fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot {
        AmigaInputDiagnosticSnapshot {
            joy0_x: self.joy0_x,
            joy0_y: self.joy0_y,
            joy0dat: joydat(self.joy0_x, self.joy0_y),
            joy1_x: self.joy1_x,
            joy1_y: self.joy1_y,
            joy1dat: joydat(self.joy1_x, self.joy1_y),
            port0_primary_button_pressed: self.port0_left_button_pressed,
            port1_primary_button_pressed: self.port1_left_button_pressed,
            joystick1_up: self.joystick1.up,
            joystick1_down: self.joystick1.down,
            joystick1_left: self.joystick1.left,
            joystick1_right: self.joystick1.right,
            joystick1_fire: self.joystick1.fire,
            joystick1_button2: self.joystick1.button2,
            joystick1_button3: self.joystick1.button3,
        }
    }

    /// Read a word at the given 24-bit address — peeks state without
    /// side effects (does NOT clear ICR etc). For inspecting state
    /// during tests; not equivalent to a CPU bus cycle.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.peek_bus_word(addr & 0xFF_FFFF)
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
        let hi = self.peek_bus_word(addr & 0xFF_FFFF);
        let lo = self.peek_bus_word(addr.wrapping_add(2) & 0xFF_FFFF);
        (u32::from(hi) << 16) | u32::from(lo)
    }

    fn bus_read_word(&self, addr24: u32) -> u16 {
        self.inspect_bus_word(addr24, true)
    }

    fn peek_bus_word(&self, addr24: u32) -> u16 {
        self.inspect_bus_word(addr24, false)
    }

    fn inspect_bus_word(&self, addr24: u32, drive_memory_bus: bool) -> u16 {
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.peek(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.peek(reg));
        }
        if (0x00DA_0000..0x00DC_0000).contains(&addr24) {
            return self.gayle.read_word(addr24);
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
                // DENISEID — KS reads this to discriminate AGA Lisa
                // ($00F8) from ECS Super Denise ($FFFC) and OCS
                // Denise ($FFFF, open bus). A1200 = Lisa.
                0x07C => self.denise.deniseid(),
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
                0x180..=0x1BE => self.denise.ocs.read_color_register(offset),
                0x0A0..=0x0DA => paula_decode::audio_register(offset)
                    .map(|(ch, f)| self.paula.read_audio(ch, f))
                    .unwrap_or(0xFFFF),
                _ => 0xFFFF,
            };
        }
        if drive_memory_bus {
            self.memory.read_word(addr24)
        } else {
            self.memory.peek_word(addr24)
        }
    }

    /// Read a chip-RAM byte directly, ignoring the OVL overlay.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        self.memory.read_chip_ram_byte(addr)
    }

    /// Tick one primary period — the master/4 lores-pixel and board clock
    /// (7.09 MHz PAL). Chipset timing derives from this domain; the stock
    /// MC68EC020 receives two input-clock edges during the same period.
    ///
    /// Two ticks make one Agnus CCK, so chip-side events that the HRM
    /// describes at CCK granularity (beam advance, copper fetch slot,
    /// bitplane fetch, shift-register reload) fire on alternate ticks
    /// (`cck_phase == 0`). Board-tick events (lores pixel output and the
    /// CIA E-clock divisor) fire once, while CPU bus service runs before
    /// each emitted processor edge.
    /// One machine tick. The per-CCK body is the shared
    /// [`AmigaDriver::tick`]; this inherent method delegates so the many
    /// existing `AmigaA1200::tick` callers (tests, the `AmigaMachine`
    /// impl, MCP) keep working unchanged.
    pub fn tick(&mut self) {
        <Self as AmigaDriver>::tick(self);
    }

    /// Advance either to the next active-CPU instruction boundary or through
    /// one complete Amiga system tick when no boundary is crossed.
    #[must_use]
    pub fn advance_to_cpu_boundary(&mut self) -> bool {
        <Self as AmigaDriver>::advance_to_cpu_boundary(self)
    }

    /// CIA-A is wired to the low data byte (D0-D7) at `$BFExxx`. The
    /// chip side-effects on every access — reading ICR clears its
    /// flags — so the dispatcher routes all reads (byte or word, any
    /// parity) through the chip and lets `BusResponse::Byte` deliver
    /// the value.
    fn dispatch_cia_a(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        let reg = cia::decode_cia_a(tx.addr)?;
        Some(if tx.is_read {
            *self.debug_cia_a_read_counts.entry(reg).or_insert(0) += 1;
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
            *self.debug_cia_b_read_counts.entry(reg).or_insert(0) += 1;
            BusResponse::Byte(self.cia_b.read(reg))
        } else {
            let byte = if tx.is_word {
                (tx.data >> 8) as u8
            } else {
                tx.data as u8
            };
            self.debug_cia_b_cr_log.push((
                self.tick_count / TICKS_PER_CCK,
                self.cpu.regs.pc,
                reg,
                byte,
            ));
            self.cia_b.write(reg, byte);
            if matches!(reg, 0x01 | 0x03) {
                self.apply_df0_control_from_cia_b();
            }
            BusResponse::WriteAck
        })
    }

    /// Gayle gate array window at `$DA0000-$DBFFFF`. IDE task-file
    /// registers at `$DA0000-$DA3FFF` (Stage A: no drive — STATUS
    /// reads $7F, others $FF) and the four Gayle control registers
    /// at `$DA8000-$DABFFF`. Returns `None` for addresses outside
    /// the Gayle window so subsequent dispatchers can claim them.
    fn dispatch_gayle(&mut self, tx: &BusTransaction) -> Option<BusResponse> {
        if !(0x00DA_0000..0x00DC_0000).contains(&tx.addr) {
            return None;
        }
        Some(if tx.is_read {
            if tx.is_word {
                BusResponse::Word(self.gayle.read_word(tx.addr))
            } else {
                BusResponse::Byte(self.gayle.read(tx.addr))
            }
        } else {
            if tx.is_word {
                self.gayle.write_word(tx.addr, tx.data);
            } else {
                self.gayle.write(tx.addr, tx.data as u8);
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
                0x07C => self.denise.deniseid(),
                // FMODE write-side lives on Alice. On real AGA the
                // register reads back as the last-written value (not
                // open bus). KS 3.1 reads $1FC during init — without
                // this case it gets $FFFF where it expects 0.
                0x1FC => self.agnus.fmode,
                0x180..=0x1BE => self.denise.ocs.read_color_register(offset),
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
            self.record_memory_watch_write(
                AmigaMemoryWriteSource::Cpu,
                tx.addr,
                tx.data,
                tx.is_word,
            );
            if tx.is_word {
                self.memory.write_word(tx.addr, tx.data);
            } else {
                self.memory.write_byte(tx.addr, tx.data as u8);
            }
            BusResponse::WriteAck
        }
    }

    /// A1200 chip RAM is connected through Alice's 32-bit data path.
    ///
    /// This is the only dynamic-sized region enabled initially. OVL reads
    /// continue through the ROM compatibility path, and MMIO remains legacy
    /// until each device's lanes and external response width are pinned down.
    fn dispatch_sized_chip_ram(&mut self, tx: &SizedBusTransaction) -> Option<SizedBusResponse> {
        let is_chip_ram = tx.addr < 0x0020_0000 && (!tx.is_read || !self.memory.overlay());
        if !is_chip_ram {
            return None;
        }

        let port = DataPortSize::Long;
        let transferred = dynamic_transfer_bytes(tx.remaining, tx.addr, port);

        let data = if tx.is_read {
            let mut value = 0u32;
            for offset in 0..transferred {
                value = (value << 8)
                    | u32::from(
                        self.memory
                            .read_chip_ram_byte(tx.addr.wrapping_add(u32::from(offset))),
                    );
            }
            self.memory.set_last_bus_value(value as u16);
            place_dynamic_read_data(value, transferred, tx.addr, port)
        } else {
            let value = extract_dynamic_bus_data(tx.data, transferred, tx.addr, port);
            self.record_sized_watch_write(tx.addr, transferred, value);
            for offset in 0..transferred {
                let shift = u32::from(transferred - offset - 1) * 8;
                self.memory.write_byte(
                    tx.addr.wrapping_add(u32::from(offset)),
                    ((value >> shift) & 0xFF) as u8,
                );
            }
            self.memory.set_last_bus_value(value as u16);
            0
        };

        Some(SizedBusResponse { data, port })
    }

    /// Preserve the existing word-shaped write-watch format for a physical
    /// phase wider than 16 bits by recording sequential word/byte fragments.
    fn record_sized_watch_write(&mut self, addr: u32, transferred: u8, value: u32) {
        let mut done = 0u8;
        while done < transferred {
            let chunk = (transferred - done).min(2);
            let fragment_addr = addr.wrapping_add(u32::from(done));
            let remaining = transferred - done;
            let shift = u32::from(remaining - chunk) * 8;
            let mask = if chunk == 2 { 0xFFFF } else { 0xFF };
            self.record_memory_watch_write(
                AmigaMemoryWriteSource::Cpu,
                fragment_addr,
                ((value >> shift) & mask) as u16,
                chunk == 2,
            );
            done += chunk;
        }
    }

    fn record_memory_watch_write(
        &mut self,
        source: AmigaMemoryWriteSource,
        addr: u32,
        data: u16,
        is_word: bool,
    ) {
        if let Some((lo, len)) = self.debug_watch_addr {
            let hi = lo.wrapping_add(len);
            let access_len = if is_word { 2u32 } else { 1 };
            let access_hi = addr.wrapping_add(access_len);
            if addr < hi && access_hi > lo {
                let record = AmigaMemoryWriteRecord {
                    cck: self.tick_count / TICKS_PER_CCK,
                    pc: self.cpu.regs.pc,
                    addr,
                    value: data,
                    is_word,
                    source,
                };
                self.debug_memory_watch_writes.push(record);
                if source == AmigaMemoryWriteSource::Cpu {
                    self.debug_watch_writes
                        .push((record.cck, record.pc, addr, data, is_word));
                }
            }
        }
    }

    /// Build a persistable snapshot of the live machine state.
    ///
    /// Diagnostic logs (`debug_*` fields) are intentionally excluded —
    /// they are observability, not state. The inserted disk is also
    /// excluded; the runtime envelope is responsible for re-inserting
    /// disk media on restore.
    #[must_use]
    pub fn snapshot_state(&self) -> AmigaA1200Snapshot {
        AmigaA1200Snapshot {
            cpu: self.cpu.clone(),
            cpu_clock: self.cpu_clock,
            cpu_domain_phase: self.cpu_domain_phase,
            memory: self.memory.clone(),
            drive: self.drive.clone(),
            track_cache: self.track_cache.clone(),
            track_word_cursor: self.track_word_cursor,
            track_pacer: self.track_pacer,
            keyboard: self.keyboard.clone(),
            prev_cia_a_spmode: self.prev_cia_a_spmode,
            gary: self.gary.clone(),
            rtc: self.rtc.snapshot_state(),
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
            gayle: self.gayle.clone(),
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
    pub fn restore_snapshot_state(&mut self, snap: AmigaA1200Snapshot) {
        self.cpu = snap.cpu;
        self.cpu_clock = snap.cpu_clock;
        self.cpu_domain_phase = snap.cpu_domain_phase;
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
        self.gayle = snap.gayle;
        self.tick_count = snap.tick_count;
        self.cck_phase = snap.cck_phase;
        self.prev_vertb_level = snap.prev_vertb_level;
        self.prev_cia_a_irq = snap.prev_cia_a_irq;
        self.prev_cia_b_irq = snap.prev_cia_b_irq;
        self.e_clock_phase = snap.e_clock_phase;

        self.debug_reg_read_counts.clear();
        self.debug_reg_read_log.clear();
        self.debug_peak_intena = 0;
        self.debug_intena_writes = 0;
        self.debug_intena_log.clear();
        self.debug_cop1lc_log.clear();
        self.debug_cop2lc_log.clear();
        self.debug_dsk_log.clear();
        self.debug_dmacon_log.clear();
        self.debug_bplcon0_log.clear();
        self.debug_palette_log.clear();
        self.debug_blit_starts = 0;
        self.debug_blit_log.clear();
        self.debug_cia_a_cr_log.clear();
        self.debug_cia_a_read_counts.clear();
        self.debug_cia_b_read_counts.clear();
        self.debug_cia_b_cr_log.clear();
        self.debug_copper_move_log.clear();
        self.debug_custom_write_log.clear();
        self.debug_watch_addr = None;
        self.debug_memory_watch_writes.clear();
        self.debug_watch_writes.clear();
        self.debug_rtc_log.clear();
        self.cpu_boundaries.clear();
    }
}

// The shared per-CCK driver (#34). Common Agnus state is exposed
// through the OCS base, while behavior inherited from ECS resolves on
// concrete Alice before coercion. The CPU is the MC68EC020 arm of
// `ActiveCpu` and the bus-dispatch chain
// carries the extra Gayle arm.
impl AmigaDriver for AmigaA1200 {
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
    fn rtc_mut(&mut self) -> &mut Msm6242Rtc {
        &mut self.rtc
    }
    fn cpu_base(&self) -> &motorola_68000::Cpu68000 {
        self.cpu.as_base()
    }
    fn cpu_base_mut(&mut self) -> &mut motorola_68000::Cpu68000 {
        self.cpu.as_base_mut()
    }
    fn cpu_clock_mut(&mut self) -> &mut CpuClock {
        &mut self.cpu_clock
    }
    fn cpu_domain_phase(&self) -> &CpuDomainPhase {
        &self.cpu_domain_phase
    }
    fn cpu_domain_phase_mut(&mut self) -> &mut CpuDomainPhase {
        &mut self.cpu_domain_phase
    }

    fn reset_external_devices_from_cpu(&mut self) {
        if let Some(board) = self.autoconfig.as_mut() {
            board.reset();
        }
    }

    fn copper_tick_cck(
        &mut self,
        vpos: u16,
        hpos: u16,
        copper_slot_granted: bool,
        blitter_busy: bool,
    ) -> Option<(u16, u16)> {
        debug_assert_eq!(hpos, self.agnus.hpos);
        let comparator_hp = self.agnus.copper_comparator_hpos();
        self.copper.tick_cck(
            &self.memory,
            vpos,
            comparator_hp,
            copper_slot_granted,
            blitter_busy,
        )
    }

    fn blitter_dma_step(&mut self, progress_granted: bool) -> BlitterCckOutcome {
        let mut bus = WatchingChipRamBus::new(&mut self.memory);
        let outcome = self.agnus.tick_blitter_cck(progress_granted, &mut bus);
        let write = bus.take_write();
        if let Some((addr, value)) = write {
            self.record_memory_watch_write(AmigaMemoryWriteSource::Blitter, addr, value, true);
        }
        outcome
    }

    fn record_disk_dma_memory_write(&mut self, addr: u32, value: u16) {
        self.record_memory_watch_write(AmigaMemoryWriteSource::DiskDma, addr, value, true);
    }

    fn audio_tick_cck(&mut self, dmacon: u16, slot: Option<u8>) {
        let memory = &self.memory;
        self.paula
            .tick_audio_cck(dmacon, slot, |addr| memory.read_chip_ram_byte(addr));
    }

    fn service_sprite_dma(&mut self, channel: u8, second_word: bool) {
        let width = self.agnus.spr_fetch_width();
        let memory = &self.memory;
        let fetched =
            self.agnus
                .service_sprite_dma_cyc(channel as usize, second_word, width, |addr| {
                    memory.read_chip_ram_word(addr)
                });
        if let Some((is_control, value)) = fetched {
            let channel = channel as usize;
            if is_control {
                let reg = 0x140 + (channel as u16) * 8 + if second_word { 2 } else { 0 };
                self.denise.write_word(reg, value as u16);
            } else if second_word {
                self.denise.ocs.write_sprite_datb_wide(channel, value);
            } else {
                self.denise.ocs.write_sprite_data_wide(channel, value);
            }
        }
    }

    fn denise_tick(&mut self, phase: u8, bitplane_dma_fetch_plane: Option<u8>) {
        let width_words = self.agnus.bpl_fetch_width();
        let vertical_diw_active = self.agnus.vertical_diw_active();
        let horizontal_blanking = self.denise.ocs.programmed_hblank_for_output_phase(
            self.agnus.hpos,
            phase,
            self.agnus.bplcon0,
            self.agnus.hbstrt(),
            self.agnus.hbstop(),
        );
        let line_ccks = self.agnus.current_line_ccks();
        let bitplane_dma_fetch =
            bitplane_dma_fetch_plane.map(|plane| denise::BitplaneDmaFetch { plane, width_words });
        self.denise.tick_with_output_signals(
            phase,
            bitplane_dma_fetch,
            denise::DeniseOutputSignals::new(vertical_diw_active, horizontal_blanking),
            &mut self.agnus,
            &self.memory,
            line_ccks,
        );
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
    fn record_cpu_boundary(&mut self) {
        if self.cpu_boundaries.len() == AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY {
            self.cpu_boundaries.pop_front();
        }
        self.cpu_boundaries.push_back(CpuBoundary {
            system_tick: self.tick_count,
            instr_start_pc: self.cpu.instr_start_pc,
            sr: self.cpu.regs.sr,
            opcode: self.cpu.ir,
        });
    }

    fn advance_agnus_cck(&mut self) {
        self.agnus.tick_cck();
    }

    fn agnus_bus_plan(&self) -> CckBusPlan {
        self.agnus
            .cck_bus_plan_with_disk_request_mask(self.paula.disk_dma_slot_request_mask())
    }

    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        AmigaA1200::dispatch_custom_write(self, offset, val);
    }
    fn dispatch_copper_write(&mut self, offset: u16, val: u16) {
        AmigaA1200::dispatch_copper_write(self, offset, val);
    }
    fn feed_next_write_word(&mut self) {
        AmigaA1200::feed_next_write_word(self);
    }
    fn feed_next_mfm_word(&mut self) {
        AmigaA1200::feed_next_mfm_word(self);
    }
    fn refresh_cia_a_external_inputs(&mut self) {
        AmigaA1200::refresh_cia_a_external_inputs(self);
    }

    fn tick_cpu_with_ipl(&mut self) {
        // Dispatch through ActiveCpu so the selected processor wrapper owns
        // its variant behavior.
        self.cpu.ipl = self.paula.compute_ipl();
        self.cpu.tick();
    }

    fn dispatch_bus(&mut self, tx: &BusTransaction) -> BusResponse {
        // A1200 board-specific: the Gayle arm (IDE + control registers) sits
        // between the CIA pair and the RTC, matching the A600 integration.
        self.dispatch_cia_a(tx)
            .or_else(|| self.dispatch_cia_b(tx))
            .or_else(|| self.dispatch_gayle(tx))
            .or_else(|| self.dispatch_rtc(tx))
            .or_else(|| self.dispatch_autoconfig(tx))
            .or_else(|| self.dispatch_fast_ram(tx))
            .or_else(|| self.dispatch_custom_register(tx))
            .unwrap_or_else(|| self.dispatch_memory(tx))
    }

    fn dispatch_sized_bus(&mut self, tx: &SizedBusTransaction) -> Option<SizedBusResponse> {
        self.dispatch_sized_chip_ram(tx)
    }
}

#[cfg(test)]
mod bus_plan_dispatch_tests {
    use super::*;
    use format_commodore_amiga_adf::ADF_SIZE_DD;
    use motorola_68000::CpuModel;
    use motorola_68000::bus::{
        BusStatus, DataPortSize, FunctionCode, TransferSize, dynamic_write_data,
    };
    use motorola_68000::cpu::{ActiveBusTransfer, State};
    use motorola_68000::microcode::MicroOp;
    use peripheral_commodore_amiga_floppy::mfm::encode_mfm_track;

    #[test]
    fn copper_and_post_output_color_writes_keep_distinct_phases_and_diagnostics() {
        use common_commodore_amiga::DeniseChip as _;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.denise.write_word(0x0180, 0x0123);
        amiga.denise.ocs.advance_color_output_samples(1);

        amiga.dispatch_copper_write(0x0180, 0x0ABC);
        assert_eq!(amiga.denise.color(0), 0x0ABC);
        let pipeline = amiga.denise.ocs.diagnostic_snapshot();
        assert_eq!(
            pipeline
                .pending_early_color_write
                .and_then(|write| write.previous_rgb12),
            Some(0x0123),
        );
        assert_eq!(
            amiga.debug_palette_log.last().map(|entry| entry.2),
            Some(0x0180)
        );

        amiga.dispatch_custom_write(0x0182, 0x0456);
        assert_eq!(amiga.denise.color(1), 0x0456);
        assert_eq!(
            amiga.debug_palette_log.last().map(|entry| entry.2),
            Some(0x0182)
        );
    }

    #[test]
    fn track_change_preserves_rotational_word_phase() {
        const WORD_PHASE: usize = 173;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.insert_adf(Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF"));
        amiga.feed_next_mfm_word();
        amiga.track_word_cursor = WORD_PHASE;

        amiga.drive.update_control(false, false, true, true, false);
        let next_track = amiga.drive.encode_mfm_track().expect("head 1 track");
        let expected_word =
            u16::from_be_bytes([next_track[WORD_PHASE * 2], next_track[WORD_PHASE * 2 + 1]]);

        amiga.feed_next_mfm_word();

        let stream = amiga.track_stream_diagnostic_snapshot();
        assert_eq!(stream.cache_head, Some(1));
        assert_eq!(stream.word_cursor, WORD_PHASE + 1);
        assert_eq!(amiga.paula.dskdatr(), expected_word);
    }

    #[test]
    fn media_change_invalidates_encoded_track_cache() {
        const WORD_PHASE: usize = 32;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.insert_adf(Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF"));
        amiga.track_word_cursor = WORD_PHASE;
        amiga.feed_next_mfm_word();
        let old_word = amiga.paula.dskdatr();
        assert!(amiga.track_cache.is_some());

        amiga.track_word_cursor = WORD_PHASE;
        amiga.insert_adf(Adf::from_bytes(vec![0xFF; ADF_SIZE_DD]).expect("valid replacement ADF"));
        assert!(amiga.track_cache.is_none());
        assert_eq!(amiga.track_word_cursor, WORD_PHASE);

        let replacement_track = amiga.drive.encode_mfm_track().expect("replacement track");
        let expected_word = u16::from_be_bytes([
            replacement_track[WORD_PHASE * 2],
            replacement_track[WORD_PHASE * 2 + 1],
        ]);
        assert_ne!(expected_word, old_word);

        amiga.feed_next_mfm_word();
        assert_eq!(amiga.paula.dskdatr(), expected_word);
        assert_eq!(amiga.track_word_cursor, WORD_PHASE + 1);

        amiga.eject_disk();
        assert!(amiga.track_cache.is_none());
        let cursor_after_eject = amiga.track_word_cursor;
        amiga.feed_next_mfm_word();
        assert!(amiga.track_cache.is_none());
        assert_eq!(amiga.track_word_cursor, cursor_after_eject);
        assert_eq!(amiga.paula.dskdatr(), expected_word);
    }

    #[test]
    fn successful_track_write_invalidates_cache_and_preserves_word_phase() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.mount_adf(
            Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF"),
            false,
            true,
        );
        amiga.feed_next_mfm_word();

        let stale_track = amiga
            .track_cache
            .as_ref()
            .expect("initial read should encode the track")
            .2
            .clone();
        let replacement_data = vec![0xA5; 11 * 512];
        let replacement_track = encode_mfm_track(&replacement_data, 0, 11);
        let word_phase = stale_track
            .as_chunks::<2>()
            .0
            .iter()
            .zip(replacement_track.as_chunks::<2>().0.iter())
            .position(|(old, replacement)| old != replacement)
            .expect("distinct sector data should change the encoded track");
        let stale_word =
            u16::from_be_bytes([stale_track[word_phase * 2], stale_track[word_phase * 2 + 1]]);
        let expected_word = u16::from_be_bytes([
            replacement_track[word_phase * 2],
            replacement_track[word_phase * 2 + 1],
        ]);
        assert_ne!(stale_word, expected_word);
        amiga.track_word_cursor = word_phase;

        let mfm_words: Vec<u16> = replacement_track
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&bytes| u16::from_be_bytes(bytes))
            .collect();
        let dsklen = emu198x_commodore_paula_8364::bits::DSKLEN_DMAEN
            | emu198x_commodore_paula_8364::bits::DSKLEN_WRITE
            | u16::try_from(mfm_words.len()).expect("DD track fits DSKLEN");
        amiga.paula.write_dsklen(dsklen);
        amiga.paula.write_dsklen(dsklen);
        for word in mfm_words {
            assert!(amiga.paula.accept_disk_write_dma_slot(word));
            amiga.feed_next_write_word();
        }

        assert!(amiga.track_cache.is_none());
        assert_eq!(amiga.track_word_cursor, word_phase);
        assert_eq!(
            &amiga.drive.save_adf().expect("disk remains mounted")[..replacement_data.len()],
            replacement_data.as_slice()
        );

        amiga.feed_next_mfm_word();
        assert_eq!(amiga.paula.dskdatr(), expected_word);
        assert_eq!(amiga.track_word_cursor, word_phase + 1);
    }

    #[test]
    fn cia_b_bus_writes_are_recorded_for_diagnostics() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        let response = amiga.dispatch_cia_b(&BusTransaction {
            addr: 0x00BF_D100,
            is_read: false,
            is_word: true,
            data: 0xA500,
        });

        assert!(matches!(response, Some(BusResponse::WriteAck)));
        assert_eq!(amiga.debug_cia_b_cr_log, vec![(0, 0, 1, 0xA5)]);
    }

    #[test]
    fn aga_blitter_extension_write_does_not_complete_the_active_blit() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.poke_word(0x00DF_F058, (2 << 6) | 2);
        assert!(amiga.agnus.blitter_busy);

        amiga.poke_word(0x00DF_F05C, 3);
        assert!(amiga.agnus.blitter_busy);
        assert_eq!(amiga.paula.intreq() & 0x0040, 0);

        amiga.poke_word(0x00DF_F05E, 2);
        assert!(amiga.agnus.blitter_busy);
        assert_eq!(amiga.agnus.blitter_startup_ccks_remaining(), 2);
        assert_eq!(amiga.paula.intreq() & 0x0040, 0);
    }

    #[test]
    fn a1200_gary_reports_the_installed_gayle() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);

        assert!(amiga.gary().gayle_present());
        assert_eq!(amiga.chip_select(0x00DA_8000), ChipSelect::Gayle);
        amiga.poke_byte(0x00DA_8000, 0x5A);
        assert_eq!(amiga.read_word(0x00DA_8000), 0x005A);
        assert_eq!(
            amiga.gayle_diagnostic_snapshot().registers.card_status,
            0x5A,
        );
    }

    #[test]
    fn aga_rdram_reaches_debug_and_cpu_custom_register_reads() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.poke_word(0x00DF_F106, 0xA000); // BANK=5, high nibbles
        amiga.poke_word(0x00DF_F18A, 0x8A5C); // COLOR05 -> palette $A5, T=1
        amiga.poke_word(0x00DF_F106, 0xA200); // BANK=5, LOCT
        amiga.poke_word(0x00DF_F18A, 0x0123);

        assert_eq!(amiga.read_word(0x00DF_F18A), 0xFFFF);
        amiga.poke_word(0x00DF_F104, 0x0100); // BPLCON2 RDRAM

        amiga.poke_word(0x00DF_F106, 0xA000);
        assert_eq!(amiga.read_word(0x00DF_F18A), 0x8A5C);
        assert!(matches!(
            amiga.dispatch_custom_register(&BusTransaction {
                addr: 0x00DF_F18A,
                is_read: true,
                is_word: true,
                data: 0,
            }),
            Some(BusResponse::Word(0x8A5C)),
        ));

        amiga.poke_word(0x00DF_F106, 0xA200);
        assert_eq!(amiga.read_word(0x00DF_F18A), 0x0123);
        assert!(matches!(
            amiga.dispatch_custom_register(&BusTransaction {
                addr: 0x00DF_F18A,
                is_read: true,
                is_word: true,
                data: 0,
            }),
            Some(BusResponse::Word(0x0123)),
        ));

        amiga.poke_word(0x00DF_F18A, 0x0FED); // ignored while RDRAM is set
        amiga.poke_word(0x00DF_F106, 0xA000);
        assert_eq!(amiga.read_word(0x00DF_F18A), 0x8A5C);
        amiga.poke_word(0x00DF_F106, 0x8000); // BANK=4
        assert_eq!(amiga.read_word(0x00DF_F18A), 0x0000);

        amiga.poke_word(0x00DF_F104, 0x0000);
        assert_eq!(amiga.read_word(0x00DF_F18A), 0xFFFF);
    }

    #[test]
    fn copper_wait_uses_the_alice_programmed_horizontal_projection() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.write_htotal(0x00FA);
        amiga.agnus.write_beamcon0(
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_PAL,
        );
        amiga.agnus.hpos = 0x00F8;
        amiga.copper.waiting = true;
        amiga.copper.wait_target = 0x0010;
        amiga.copper.wait_mask = 0x80FE;
        amiga.copper.wait_bfd = true;

        <AmigaA1200 as AmigaDriver>::copper_tick_cck(&mut amiga, 0, 0x00F8, false, false);

        assert!(
            amiga.copper.waiting,
            "programmed physical $F8 must wrap to comparator position zero",
        );
    }

    fn observe_ddf_start(amiga: &mut AmigaA1200) {
        let start = amiga.agnus.ddfstrt & 0x00FE;
        assert!(start > 0, "test helper requires a non-zero DDFSTRT");
        amiga.agnus.hpos = start - 1;
        amiga.agnus.tick_cck();
        assert_eq!(amiga.agnus.ddf_start_match(), Some(start));
    }

    #[test]
    fn stock_a1200_clock_emits_two_cpu_edges_per_system_tick() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.cpu.state = State::Internal { cycles: 4 };
        let tick_count = amiga.tick_count;

        amiga.tick();

        assert_eq!(amiga.tick_count, tick_count + 1);
        assert!(matches!(amiga.cpu.state, State::Internal { cycles: 2 }));
        assert_eq!(amiga.cpu.model(), CpuModel::M68EC020);
        assert_eq!(amiga.cpu_clock.numerator(), 2);
        assert_eq!(amiga.cpu_clock.denominator(), 1);
        assert_eq!(amiga.cpu_clock.phase(), 0);
    }

    #[test]
    fn snapshot_preserves_the_a1200_clock_fixed_point_and_clears_boundaries() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());
        amiga.tick();
        <AmigaA1200 as AmigaDriver>::record_cpu_boundary(&mut amiga);
        assert_eq!(amiga.cpu_boundaries.len(), 1);

        let encoded =
            postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize A1200 snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&encoded).expect("deserialize A1200 snapshot");
        let mut restored = AmigaA1200::new(rom);
        restored.restore_snapshot_state(snapshot);

        assert_eq!(restored.cpu.model(), CpuModel::M68EC020);
        assert_eq!(restored.cpu_clock.numerator(), 2);
        assert_eq!(restored.cpu_clock.denominator(), 1);
        assert_eq!(restored.cpu_clock.phase(), 0);
        assert_eq!(restored.drain_cpu_boundaries().len(), 0);
    }

    #[test]
    fn instruction_boundary_queue_is_bounded_and_drains_in_order() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);

        for system_tick in 0..=AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY {
            amiga.tick_count = system_tick as u64;
            <AmigaA1200 as AmigaDriver>::record_cpu_boundary(&mut amiga);
        }

        let mut boundaries = amiga.drain_cpu_boundaries();
        assert_eq!(boundaries.len(), AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY);
        assert_eq!(
            boundaries
                .next()
                .expect("the bounded queue retains its first entry")
                .system_tick,
            1
        );
        assert_eq!(
            boundaries
                .next_back()
                .expect("the bounded queue retains its final entry")
                .system_tick,
            AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY as u64
        );
    }

    #[test]
    fn cpu_reset_output_restarts_autoconfig_without_clearing_fast_ram() {
        let mut amiga = AmigaA1200::with_ram_config(
            vec![0; 512 * 1024],
            RamConfig {
                chip_kb: 512,
                slow_kb: 0,
                fast_kb: 1024,
            },
        );
        let board = amiga
            .autoconfig
            .as_mut()
            .expect("fast RAM must install an Autoconfig board");
        board.write_word(0x4A, 0x0000);
        board.write_word(0x48, 0x2000);
        board.write_ram_byte(0x0020_0042, 0xA5);
        assert_eq!(
            board.state(),
            AutoconfigState::Configured { base: 0x0020_0000 }
        );
        amiga.cpu.reset_out = true;

        amiga.tick();

        assert!(!amiga.cpu.reset_out, "machine must consume RESET output");
        let board = amiga
            .autoconfig()
            .expect("Autoconfig board remains installed");
        assert_eq!(board.state(), AutoconfigState::Unconfigured);
        assert_eq!(board.ram_bytes()[0x42], 0xA5);
    }

    #[test]
    fn constructed_alice_schedules_all_eight_lowres_bitplanes() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.vpos = 0x0020;

        // Program the display through the guest-visible custom-register
        // path. BPU3 (BPLCON0 bit 4) extends the AGA plane count to eight.
        amiga.poke_word(0x00DF_F096, 0x8300); // SETCLR | DMAEN | BPLEN
        amiga.poke_word(0x00DF_F100, 0x0010); // BPU = 8, lowres
        amiga.poke_word(0x00DF_F08E, 0x2010); // VSTART matches current line
        amiga.poke_word(0x00DF_F090, 0xA020);
        amiga.poke_word(0x00DF_F092, 0x0038);
        amiga.poke_word(0x00DF_F094, 0x00D0);

        assert_eq!(amiga.agnus.max_bitplanes, 8);
        assert_eq!(amiga.agnus.num_bitplanes(), 8);

        observe_ddf_start(&mut amiga);
        assert_eq!(
            amiga.agnus.cck_bus_plan().slot_owner,
            SlotOwner::Bitplane(6)
        );
        amiga.agnus.hpos = 0x003C;
        assert_eq!(
            amiga.agnus.cck_bus_plan().slot_owner,
            SlotOwner::Bitplane(7)
        );
    }

    #[test]
    fn alice_hblank_reset_precedes_a_coincident_wide_bitplane_fetch() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.vpos = 0x001F;

        // Alice accepts the enhanced even-CCK DDF comparator at $12. The
        // ordinary early-start value $18 is six CCKs after the fixed HBLANK
        // boundary and therefore cannot exercise same-CCK ordering.
        amiga.poke_word(0x00DF_F096, 0x8300); // SETCLR | DMAEN | BPLEN
        amiga.poke_word(0x00DF_F100, 0x0010); // BPU = 8, lowres
        amiga.poke_word(0x00DF_F08E, 0x2010); // VSTART on the next line
        amiga.poke_word(0x00DF_F090, 0xA020);
        amiga.poke_word(0x00DF_F092, 0x0012);
        amiga.poke_word(0x00DF_F094, 0x00D0);
        amiga.poke_word(0x00DF_F1FC, 0x0001); // 32-bit bitplane fetches

        // The first wide-fetch slot carries BPL8. Its word-zero is blank and
        // its staged tail starts with one set pixel, making loss of that tail
        // observable after the shift register drains.
        amiga.agnus.bpl_pt[7] = 0x0000_2000;
        amiga.poke_word(0x0000_2000, 0x0000);
        amiga.poke_word(0x0000_2002, 0x8000);

        // Enter line $20 through a real raw wrap, retain its pre-$12 carry,
        // then stop after phase zero at the fixed HBLANK boundary. This avoids
        // manufacturing a boundary by calling Denise directly with an unset
        // line marker.
        amiga.agnus.hpos = amiga.agnus.current_line_ccks() - 1;
        amiga.cck_phase = 1;
        amiga.tick();
        let mut guard = 0;
        while !(amiga.agnus.vpos == 0x0020 && amiga.agnus.hpos == 0x0012 && amiga.cck_phase == 1) {
            amiga.tick();
            guard += 1;
            assert!(guard < 100, "beam did not reach line $20 hpos $12");
        }

        assert!(amiga.agnus.vertical_diw_active());
        assert_eq!(
            amiga.agnus.bpl_pt[7], 0x0000_2004,
            "Alice must grant one 32-bit BPL8 transfer at DDFSTRT=$12",
        );

        // Commit the fetched group and consume its blank first word. The next
        // source pixel must come from the staged 0x8000 tail. If HBLANK reset
        // ran after this CCK's DMA service, begin_beam_line() would have
        // discarded the tail and this pixel would remain zero.
        let denise = amiga.denise.ocs.as_inner_mut().as_inner_mut();
        denise.trigger_shift_load();
        for x in 0..16 {
            let pixel = denise.output_pixel_with_beam(x, 0, x, 0);
            assert_eq!(pixel.quad_samples[0].raw_color_idx, 0);
        }
        let tail_pixel = denise.output_pixel_with_beam(16, 0, 16, 0);
        assert_eq!(
            tail_pixel.quad_samples[0].raw_color_idx, 0x80,
            "the first wide transfer's staged BPL8 tail must survive line reset",
        );
    }

    #[test]
    fn guest_programmed_vbstop_drives_alice_sprite_control_fetch() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.poke_word(0x00DF_F1CC, 300); // VBSTRT
        amiga.poke_word(0x00DF_F1CE, 40); // VBSTOP
        amiga.poke_word(0x00DF_F1DC, 0x1020); // VARVBEN | PAL
        amiga.poke_word(0x00DF_F096, 0x8220); // SETCLR | DMAEN | SPREN
        amiga.agnus.vpos = 39;
        amiga.agnus.hpos = amiga.agnus.current_line_ccks() - 1;
        amiga.agnus.tick_cck();
        assert!(amiga.agnus.programmed_vblank_stop_event());
        amiga.agnus.hpos = 0x14;
        amiga.agnus.spr_pt[0] = 0x0000_2000;
        amiga.poke_word(0x0000_2000, 0x4100);
        amiga.cck_phase = 0;

        amiga.tick();

        assert_eq!(amiga.agnus.hpos, 0x15);
        assert_eq!(amiga.agnus.vbstop(), 40);
        assert_eq!(amiga.agnus.spr_pt[0], 0x0000_2002);
        assert_eq!(amiga.agnus.sprite_vstart(0), 0x41);
    }

    #[test]
    fn snapshot_preserves_programmed_vblank_latch_and_line_event() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());
        amiga.agnus.write_vbstrt(300);
        amiga.agnus.write_vbstop(40);
        amiga.agnus.write_beamcon0(0x1020); // VARVBEN | PAL
        amiga.agnus.vpos = 299;
        amiga.agnus.hpos = amiga.agnus.current_line_ccks() - 1;
        amiga.agnus.tick_cck();
        assert!(amiga.agnus.programmed_vblank_active());
        assert!(!amiga.agnus.programmed_vblank_stop_event());

        let bytes = postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&bytes).expect("deserialize snapshot");
        let mut restored = AmigaA1200::new(rom.clone());
        restored.restore_snapshot_state(snapshot);
        assert!(restored.agnus.programmed_vblank_active());

        restored.agnus.vpos = 39;
        restored.agnus.hpos = restored.agnus.current_line_ccks() - 1;
        restored.agnus.tick_cck();
        assert!(!restored.agnus.programmed_vblank_active());
        assert!(restored.agnus.programmed_vblank_stop_event());

        let bytes = postcard::to_allocvec(&restored.snapshot_state()).expect("serialize edge");
        let snapshot: AmigaA1200Snapshot = postcard::from_bytes(&bytes).expect("deserialize edge");
        let mut edge_restored = AmigaA1200::new(rom);
        edge_restored.restore_snapshot_state(snapshot);
        assert!(!edge_restored.agnus.programmed_vblank_active());
        assert!(edge_restored.agnus.programmed_vblank_stop_event());
    }

    #[test]
    fn guest_and_dma_extended_sprite_coordinates_survive_alice_snapshot() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());

        // Use the guest-visible custom-register path. CTL bits 6/5
        // supply VSTART[9]/VSTOP[9] on Alice; the later POS write must
        // preserve the start high bits.
        amiga.poke_word(0x00DF_F142, 0x0266);
        amiga.poke_word(0x00DF_F140, 0x0100);
        assert_eq!(amiga.agnus.sprite_vstart(0), 0x0301);
        assert_eq!(amiga.agnus.sprite_vstop(0), 0x0302);

        // Fetch an asymmetric pair through Alice's real sprite-1 bus
        // slots. This covers the machine's DMA-to-Agnus and DMA-to-Denise
        // control-word route rather than only direct register dispatch.
        amiga.poke_word(0x0000_2000, 0x0100); // SPR1POS low VSTART=$01
        amiga.poke_word(0x0000_2002, 0x0226); // VSTART=$101, VSTOP=$302
        amiga.poke_word(0x00DF_F124, 0x0000); // SPR1PTH
        amiga.poke_word(0x00DF_F126, 0x2000); // SPR1PTL
        amiga.poke_word(0x00DF_F14A, 30 << 8); // make line 30 a control fetch
        amiga.poke_word(0x00DF_F096, 0x8220); // SETCLR | DMAEN | SPREN
        amiga.agnus.vpos = 30;
        amiga.agnus.hpos = 0x18;
        amiga.cck_phase = 0;
        amiga.tick(); // sprite 1 first control slot at $19
        assert_eq!(amiga.agnus.spr_pt[1], 0x0000_2002);
        amiga.agnus.hpos = 0x1A;
        amiga.cck_phase = 0;
        amiga.tick(); // sprite 1 second control slot at $1B
        assert_eq!(amiga.agnus.spr_pt[1], 0x0000_2004);
        assert_eq!(amiga.agnus.sprite_vstart(1), 0x0101);
        assert_eq!(amiga.agnus.sprite_vstop(1), 0x0302);

        let bytes = postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&bytes).expect("deserialize snapshot");
        let mut restored = AmigaA1200::new(rom);
        restored.restore_snapshot_state(snapshot);

        assert_eq!(restored.agnus.sprite_vstart(0), 0x0301);
        assert_eq!(restored.agnus.sprite_vstop(0), 0x0302);
        assert_eq!(restored.agnus.sprite_vstart(1), 0x0101);
        assert_eq!(restored.agnus.sprite_vstop(1), 0x0302);

        // The restored values remain live comparator state, not merely
        // serialized diagnostics.
        restored.agnus.write_htotal(0);
        restored.agnus.write_vtotal(0x03FF);
        restored.agnus.write_beamcon0(
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_PAL,
        );
        restored.agnus.vpos = 0x0300;
        restored.agnus.hpos = 0;
        restored.agnus.tick_cck();
        assert!(restored.agnus.sprite_dma_on(0));
        restored.agnus.tick_cck();
        assert!(!restored.agnus.sprite_dma_on(0));
    }

    #[test]
    fn snapshot_preserves_mid_cck_sprite_bus_authority() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());
        amiga.agnus.dmacon = 0x0220; // DMAEN | SPREN
        amiga.agnus.spr_pt[0] = 0x0000_2000;
        amiga.poke_word(0x0000_2000, 0x4100);
        amiga.agnus.vpos = 25;
        amiga.agnus.hpos = 0x14;
        amiga.cck_phase = 0;

        amiga.tick();

        assert_eq!(amiga.cck_phase, 1);
        assert_eq!(amiga.agnus.hpos, 0x15);
        assert!(amiga.agnus.sprite_bus_used_this_cck());

        let bytes = postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&bytes).expect("deserialize snapshot");
        let mut restored = AmigaA1200::new(rom);
        restored.restore_snapshot_state(snapshot);

        assert_eq!(restored.cck_phase, 1);
        assert!(
            restored.agnus.sprite_bus_used_this_cck(),
            "phase-one restore must keep the phase-zero sprite's bus ownership"
        );
    }

    #[test]
    fn snapshot_preserves_current_line_ddf_start_origin() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());
        amiga.agnus.vpos = 0x0020;
        amiga.agnus.dmacon = 0x0300;
        amiga.agnus.bplcon0 = 0x9000; // hires, one bitplane
        amiga.agnus.write_ddfstrt(0x0038);
        amiga.agnus.write_ddfstop(0x00D0);
        amiga.agnus.write_diwstrt(0x2010);
        amiga.agnus.write_diwstop(0xA020);
        amiga.agnus.bpl_pt[0] = 0x0000_2000;
        observe_ddf_start(&mut amiga);

        amiga.agnus.write_ddfstrt(0x0080);
        while amiga.agnus.hpos < 0x003F {
            amiga.agnus.tick_cck();
        }
        assert_eq!(amiga.agnus.ddf_start_match(), Some(0x0038));
        assert_eq!(amiga.agnus.cck_bus_plan().bitplane_dma_fetch_plane, Some(0));
        amiga.cck_phase = 1;

        let bytes = postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&bytes).expect("deserialize snapshot");
        let mut restored = AmigaA1200::new(rom);
        restored.restore_snapshot_state(snapshot);

        assert_eq!(restored.agnus.hpos, 0x003F);
        assert_eq!(restored.agnus.ddfstrt, 0x0080);
        assert_eq!(restored.agnus.ddf_start_match(), Some(0x0038));
        let fetch = restored.agnus.cck_bus_plan().bitplane_dma_fetch_plane;
        assert_eq!(fetch, Some(0));
        <AmigaA1200 as AmigaDriver>::denise_tick(&mut restored, 0, fetch);
        assert_eq!(restored.agnus.bpl_pt[0], 0x0000_2002);
    }

    #[test]
    fn snapshot_preserves_matched_ddf_stop_and_pending_final_fetch() {
        let rom = vec![0; 512 * 1024];
        let mut amiga = AmigaA1200::new(rom.clone());
        amiga.agnus.vpos = 0x0020;
        amiga.agnus.dmacon = 0x0300;
        amiga.agnus.bplcon0 = 0x9000; // hires, one bitplane
        amiga.agnus.write_ddfstrt(0x0038);
        amiga.agnus.write_ddfstop(0x0040);
        amiga.agnus.write_diwstrt(0x2010);
        amiga.agnus.write_diwstop(0xA020);
        observe_ddf_start(&mut amiga);
        while amiga.agnus.hpos < 0x0040 {
            amiga.agnus.tick_cck();
        }

        assert_eq!(amiga.agnus.ddf_stop_match(), Some(0x0040));
        assert_eq!(amiga.agnus.ddf_fetch_end(), Some(0x0047));
        amiga.agnus.write_ddfstop(0x0080);

        let bytes = postcard::to_allocvec(&amiga.snapshot_state()).expect("serialize snapshot");
        let snapshot: AmigaA1200Snapshot =
            postcard::from_bytes(&bytes).expect("deserialize snapshot");
        let mut restored = AmigaA1200::new(rom);
        restored.restore_snapshot_state(snapshot);

        assert_eq!(restored.agnus.hpos, 0x0040);
        assert_eq!(restored.agnus.ddfstop, 0x0080);
        assert_eq!(restored.agnus.ddf_stop_match(), Some(0x0040));
        assert_eq!(restored.agnus.ddf_fetch_end(), Some(0x0047));

        while restored.agnus.hpos < 0x0047 {
            restored.agnus.tick_cck();
        }
        assert_eq!(
            restored.agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(0),
            "the frozen final fetch unit must remain pending after restore"
        );
        restored.agnus.tick_cck();
        assert_eq!(
            restored.agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "the restored endpoint must terminate the current fetch run"
        );
    }

    #[test]
    fn concrete_alice_plan_releases_demoted_bitplane_slot_to_cpu() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.vpos = 0x0020;
        amiga.agnus.hpos = 0x0023;
        amiga.agnus.dmacon = 0x0300; // DMAEN | BPLEN
        amiga.agnus.bplcon0 = 0x1000; // one bitplane
        amiga.agnus.ddfstrt = 0x001C;
        amiga.agnus.ddfstop = 0x001C;
        amiga.agnus.diwstrt = 0x1010;
        amiga.agnus.diwstop = 0xA020;
        amiga.agnus.write_diwhigh(0x0101);
        observe_ddf_start(&mut amiga);
        amiga.agnus.hpos = 0x0023;

        let plan = amiga.agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert!(plan.cpu_chip_bus_granted);

        amiga.cpu.state = State::BusCycle {
            op: MicroOp::WriteWord,
            addr: 0x0000_1000,
            fc: FunctionCode::SupervisorData,
            is_read: false,
            is_word: true,
            data: Some(0x5AA5),
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(amiga.cpu.bus_status, BusStatus::Ready(0));
        assert_eq!(amiga.memory.read_chip_ram_word(0x1000), 0x5AA5);
    }

    #[test]
    fn memory_watch_records_cpu_blitter_and_disk_dma_sources() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.memory.set_overlay(false);
        amiga.debug_watch_addr = Some((0x1000, 0x2002));

        let response = amiga.dispatch_memory(&BusTransaction {
            addr: 0x1000,
            is_read: false,
            is_word: true,
            data: 0x1234,
        });
        assert!(matches!(response, BusResponse::WriteAck));

        amiga.poke_word(0x00DF_F040, 0x01FF);
        amiga.poke_word(0x00DF_F042, 0x0000);
        amiga.poke_word(0x00DF_F054, 0x0000);
        amiga.poke_word(0x00DF_F056, 0x2000);
        amiga.poke_word(0x00DF_F058, 0x0041);
        for _ in 0..16 {
            let _ = amiga.blitter_dma_step(true);
            if !amiga.agnus.blitter_busy {
                break;
            }
        }
        assert!(!amiga.agnus.blitter_busy);

        amiga.poke_word(0x00DF_F020, 0x0000);
        amiga.poke_word(0x00DF_F022, 0x3000);
        amiga.poke_word(0x00DF_F096, 0x8210);
        amiga.poke_word(0x00DF_F024, 0x8001);
        amiga.poke_word(0x00DF_F024, 0x8001);
        amiga.paula.receive_disk_read_word(0xA55A);
        assert!(<AmigaA1200 as AmigaDriver>::service_disk_dma_slot(
            &mut amiga
        ));

        assert_eq!(
            amiga
                .debug_memory_watch_writes
                .iter()
                .map(|record| record.source)
                .collect::<Vec<_>>(),
            vec![
                AmigaMemoryWriteSource::Cpu,
                AmigaMemoryWriteSource::Blitter,
                AmigaMemoryWriteSource::DiskDma,
            ]
        );
        assert_eq!(
            amiga
                .debug_memory_watch_writes
                .iter()
                .map(|record| record.addr)
                .collect::<Vec<_>>(),
            vec![0x1000, 0x2000, 0x3000]
        );
        assert_eq!(amiga.debug_watch_writes.len(), 1);
    }

    #[test]
    fn cpu_bus_preserves_odd_chip_ram_word_address_and_value() {
        const ADDR: u32 = 0x0000_1001;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.memory.set_overlay(false);
        amiga.agnus.hpos = 0x0035;
        assert_eq!(amiga.agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
        assert!(amiga.agnus.cck_bus_plan().cpu_chip_bus_granted);

        amiga.memory.write_byte(ADDR - 1, 0xA5);
        amiga.memory.write_byte(ADDR, 0);
        amiga.memory.write_byte(ADDR + 1, 0);
        amiga.memory.write_byte(ADDR + 2, 0x5A);
        amiga.cpu.state = State::BusCycle {
            op: MicroOp::WriteWord,
            addr: ADDR,
            fc: FunctionCode::SupervisorData,
            is_read: false,
            is_word: true,
            data: Some(0x1234),
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(amiga.cpu.bus_status, BusStatus::Ready(0));
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR - 1), 0xA5);
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR), 0x12);
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR + 1), 0x34);
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR + 2), 0x5A);

        amiga.cpu.state = State::BusCycle {
            op: MicroOp::ReadWord,
            addr: ADDR,
            fc: FunctionCode::SupervisorData,
            is_read: true,
            is_word: true,
            data: None,
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(amiga.cpu.bus_status, BusStatus::Ready(0x1234));
    }

    #[test]
    fn held_dynamic_long_response_does_not_repeat_chip_ram_write_side_effects() {
        const ADDR: u32 = 0x0000_1000;
        const VALUE: u32 = 0x1234_5678;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.memory.set_overlay(false);
        amiga.debug_watch_addr = Some((ADDR, 4));
        amiga.agnus.hpos = 0x0035;
        assert_eq!(amiga.agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
        assert!(amiga.agnus.cck_bus_plan().cpu_chip_bus_granted);

        amiga.cpu.active_bus_transfer = Some(ActiveBusTransfer {
            logical_size: TransferSize::Long,
            remaining: TransferSize::Long,
            write_data: VALUE,
            read_data: 0,
        });
        amiga.cpu.bus_transfer_size = TransferSize::Long;
        amiga.cpu.bus_data_out = dynamic_write_data(VALUE, TransferSize::Long, ADDR);
        amiga.cpu.state = State::BusCycle {
            op: MicroOp::WriteLong,
            addr: ADDR,
            fc: FunctionCode::SupervisorData,
            is_read: false,
            is_word: true,
            data: Some((VALUE >> 16) as u16),
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(
            amiga.cpu.bus_status,
            BusStatus::ReadySized {
                data: 0,
                port: DataPortSize::Long
            }
        );
        assert_eq!(amiga.memory.read_long(ADDR), VALUE);
        assert_eq!(amiga.debug_watch_writes.len(), 2);
        assert_eq!(
            (
                amiga.debug_watch_writes[0].2,
                amiga.debug_watch_writes[0].3,
                amiga.debug_watch_writes[0].4,
            ),
            (ADDR, 0x1234, true)
        );
        assert_eq!(
            (
                amiga.debug_watch_writes[1].2,
                amiga.debug_watch_writes[1].3,
                amiga.debug_watch_writes[1].4,
            ),
            (ADDR + 2, 0x5678, true)
        );

        let first_watch_writes = amiga.debug_watch_writes.clone();
        amiga.memory.write_byte(ADDR, 0xA5);

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(
            amiga.cpu.bus_status,
            BusStatus::ReadySized {
                data: 0,
                port: DataPortSize::Long
            }
        );
        assert_eq!(
            amiga.memory.read_chip_ram_byte(ADDR),
            0xA5,
            "a held response must not dispatch the write again"
        );
        assert_eq!(
            amiga.debug_watch_writes, first_watch_writes,
            "a held response must not duplicate diagnostic side effects"
        );
    }

    #[test]
    fn dynamic_long_write_stops_at_chip_ram_end_before_compatibility_phase() {
        const ADDR: u32 = 0x001F_FFFD;
        const VALUE: u32 = 0x1234_5678;

        let mut amiga = AmigaA1200::with_ram_config(
            vec![0; 512 * 1024],
            RamConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_kb: 0,
            },
        );
        amiga.memory.set_overlay(false);
        amiga.debug_watch_addr = Some((ADDR, 4));
        amiga.agnus.hpos = 0x0035;
        assert_eq!(amiga.agnus.cck_bus_plan().slot_owner, SlotOwner::Cpu);
        assert!(amiga.agnus.cck_bus_plan().cpu_chip_bus_granted);
        amiga.memory.write_byte(0, 0xA5);

        amiga.cpu.active_bus_transfer = Some(ActiveBusTransfer {
            logical_size: TransferSize::Long,
            remaining: TransferSize::Long,
            write_data: VALUE,
            read_data: 0,
        });
        amiga.cpu.bus_transfer_size = TransferSize::Long;
        amiga.cpu.bus_data_out = dynamic_write_data(VALUE, TransferSize::Long, ADDR);
        amiga.cpu.state = State::BusCycle {
            op: MicroOp::WriteLong,
            addr: ADDR,
            fc: FunctionCode::SupervisorData,
            is_read: false,
            is_word: true,
            data: Some((VALUE >> 16) as u16),
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(
            amiga.cpu.bus_status,
            BusStatus::ReadySized {
                data: 0,
                port: DataPortSize::Long
            }
        );
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR), 0x12);
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR + 1), 0x34);
        assert_eq!(amiga.memory.read_chip_ram_byte(ADDR + 2), 0x56);
        assert_eq!(amiga.debug_watch_writes.len(), 2);

        amiga.cpu.tick();

        let transfer = amiga
            .cpu
            .active_bus_transfer
            .expect("the final byte must remain pending");
        assert_eq!(transfer.remaining, TransferSize::Byte);
        match &amiga.cpu.state {
            State::BusCycle {
                addr,
                is_word,
                data,
                cycle_count,
                ..
            } => {
                assert_eq!(*addr, 0x0020_0000);
                assert!(!is_word);
                assert_eq!(*data, Some(0x78));
                assert_eq!(*cycle_count, 0);
            }
            _ => panic!("expected the final byte bus phase"),
        }

        let State::BusCycle { cycle_count, .. } = &mut amiga.cpu.state else {
            unreachable!("the transfer was checked as a bus cycle above");
        };
        *cycle_count = 2;
        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(
            amiga.cpu.bus_status,
            BusStatus::Ready(0),
            "the byte outside chip RAM must use compatibility dispatch"
        );
        assert_eq!(
            amiga.memory.read_chip_ram_byte(0),
            0xA5,
            "the compatibility phase must not wrap into chip RAM"
        );
        assert_eq!(amiga.debug_watch_writes.len(), 3);
        assert_eq!(
            (
                amiga.debug_watch_writes[2].2,
                amiga.debug_watch_writes[2].3,
                amiga.debug_watch_writes[2].4,
            ),
            (0x0020_0000, 0x78, false)
        );
    }

    #[test]
    fn overlay_read_keeps_dynamic_long_on_the_rom_compatibility_path() {
        let mut rom = vec![0; 512 * 1024];
        rom[0] = 0x12;
        rom[1] = 0x34;
        let mut amiga = AmigaA1200::new(rom);
        assert!(amiga.memory.overlay());

        amiga.cpu.active_bus_transfer = Some(ActiveBusTransfer {
            logical_size: TransferSize::Long,
            remaining: TransferSize::Long,
            write_data: 0,
            read_data: 0,
        });
        amiga.cpu.bus_transfer_size = TransferSize::Long;
        amiga.cpu.bus_data_out = 0;
        amiga.cpu.state = State::BusCycle {
            op: MicroOp::ReadLong,
            addr: 0,
            fc: FunctionCode::SupervisorData,
            is_read: true,
            is_word: true,
            data: None,
            cycle_count: 2,
        };
        amiga.cpu.bus_status = BusStatus::Wait;

        <AmigaA1200 as AmigaDriver>::service_cpu_bus(&mut amiga);

        assert_eq!(amiga.cpu.bus_status, BusStatus::Ready(0x1234));
    }

    #[test]
    fn alice_explicit_zero_equal_window_blanks_denise_output() {
        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.vpos = 0x002C;
        amiga.agnus.write_diwstrt(0x2C81);
        amiga.agnus.write_diwstop(0x2CC1);
        amiga.agnus.write_diwhigh(0x0000); // VSTART=VSTOP=$02C; stop wins
        assert!(!amiga.agnus.vertical_diw_active());

        amiga.agnus.dmacon = 0x0300;
        amiga.agnus.ddfstrt = 0x0038;
        amiga.agnus.ddfstop = 0x00D0;
        amiga.agnus.bplcon0 = 0x1000;
        amiga.denise.write_word(0x0180, 0x000F);
        amiga.denise.write_word(0x0182, 0x0F00);
        amiga.denise.write_word(0x0110, 0x8000);
        observe_ddf_start(&mut amiga);
        amiga.agnus.hpos = 0x0040;

        <AmigaA1200 as AmigaDriver>::denise_tick(&mut amiga, 1, None);

        let y = usize::from(0x002Cu16 - 0x0019) * 2;
        let x = (usize::from(0x0040u16 - 0x002C) * 2 + 1) * 2;
        assert_eq!(
            amiga.denise.framebuffer()[y * FB_WIDTH as usize + x],
            0xFF00_00FF
        );
    }

    #[test]
    fn a1200_denise_tick_uses_lisa_fine_hblank_phase() {
        use common_commodore_amiga::DeniseChip as _;

        let mut amiga = AmigaA1200::new(vec![0; 512 * 1024]);
        amiga.agnus.vpos = 0x0032;
        amiga.agnus.write_diwstop(0x64FF);
        amiga.agnus.write_diwstrt(0x3200);
        assert!(amiga.agnus.vertical_diw_active());

        // Enable the enhanced comparator path, make the unblanked background
        // green, and place HBSTOP at Lisa fine phase seven. The renderer's
        // four-sample CCK grid pairs the eight Lisa phases, so the first
        // output sample in phase one is blank and the second is visible.
        amiga.agnus.bplcon0 = 0x0001; // ECSENA
        amiga.denise.write_word(0x0100, 0x0001);
        amiga.denise.write_word(0x0106, 0x0001); // EXTBLKEN
        amiga.denise.write_word(0x0180, 0x00F0);
        // This test isolates the fine HBLANK edge, so retire Lisa's separate
        // one-hires-sample COLOR delay before sampling the comparator.
        amiga.denise.ocs.advance_color_output_samples(1);
        amiga.agnus.write_hbstrt(0x0080);
        amiga.agnus.write_hbstop(0x07A0);
        amiga.denise.ocs.set_programmed_hblank_input(0x0080, 0x07A0);
        for _ in 0..3 {
            amiga.denise.ocs.advance_register_output_pipeline();
        }
        amiga.agnus.hpos = 0x0080;
        <AmigaA1200 as AmigaDriver>::denise_tick(&mut amiga, 0, None);
        amiga.agnus.hpos = 0x00A0;

        <AmigaA1200 as AmigaDriver>::denise_tick(&mut amiga, 1, None);

        let y = usize::from(0x0032u16 - 0x0019) * 2;
        let x = usize::from(0x00A0u16 - 0x002C) * 4 + 2;
        assert_eq!(
            amiga.denise.framebuffer()[y * FB_WIDTH as usize + x],
            0xFF00_0000,
        );
        assert_eq!(
            amiga.denise.framebuffer()[y * FB_WIDTH as usize + x + 1],
            0xFF00_FF00,
        );
    }
}
