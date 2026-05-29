//! Headless Dragon runner — `--script` / `--headless` mode.
//!
//! The bring-up and verification harness: CAS/VDK/PAK smoke matrices,
//! typed-command runs, direct DragonDOS `.BIN` loading, opcode-fetch /
//! write trace watches, snapshot trace signatures, and optional
//! patched-XRoar screenshot comparison. The non-interactive half of the
//! `emu198x-dragon` binary; the dispatcher in `main.rs` routes here when
//! a headless-only flag is present.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use emu198x_shell::StopReason as RuntimeStopReason;
use emu198x_shell::{
    CapturedFrame, FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineError,
    MachineTime, MediaImage, MediaKind, MediaSet, PixelFormat, TraceEvent, TraceSink,
    read_media_asset,
};
use format_dragon_bin::{DragonBinImage, parse_dragon_bin};
use format_dragon_cas::{CasFileType, CasHeader, CasImage, parse_cas_tolerant};
use format_dragon_disk::{DragonDiskImage, parse_vdk};
use format_dragon_pak::{
    DragonCartridgeKind as ParsedDragonCartridgeKind, DragonPakImage, PcDragonPeripherals,
    PcDragonSnapshot, parse_dragon_pak, parse_pcdragon_snapshot,
};
use machine_dragon_32::{
    AddressRange, CpuInterruptAcceptTrace, CpuInterruptKind, CpuInterruptLineTrace,
    CpuRegisterTrace, DRAGON_CPU_HZ, DRAGON_FRAME_CYCLES, DRAGON_MASTER_HZ, DeviceAccess,
    DeviceRegion, Dragon32, DragonCartridgeKind, DragonKey, DragonKeyboard, DragonVideoPhase,
    FetchTrace, MatrixKey, MemoryWriteTrace, PiaSignalTrace, ROM_SIZE, ReadonlyWrite, RunOptions,
    RunReport, StopReason, VdgModeWriteTrace, VdgSampleTrace, WatchedFetchTrace,
};
use motorola_vdg_6847::{
    TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette,
    VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT, VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH,
    VDG_PAL_OVERSCAN_VISIBLE_X, VDG_PAL_OVERSCAN_VISIBLE_Y, VdgPalette,
};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use serde::Serialize;
use zip::ZipArchive;

const DEFAULT_CYCLES: u64 = 100_000;
const DEFAULT_TRACE_LIMIT: usize = 64;
const DEFAULT_SMOKE_RUN_LIMIT: usize = 8;
const DEFAULT_XROAR_SETTLE_SECONDS: f32 = 3.0;
const DEFAULT_XROAR_TIMEOUT_SECONDS: f32 = 45.0;
const XROAR_ZOOMED_WIDTH: u32 = 512;
const XROAR_ZOOMED_HEIGHT: u32 = 384;
const BOOT_FRAME_BUDGET: u32 = 100;
const TYPED_COMMAND_BOOT_FRAME_BUDGET: u32 = 600;
const TYPED_COMMAND_POST_BOOT_SETTLE_FRAMES: u32 = 1_000;
const DIRECT_PROGRAM_BOOT_SETTLE_FRAMES: u64 = 30;
const KEY_EDGE_FRAMES: u32 = 8;
// Completed-frame screenshots are taken on CPU bus-cycle boundaries; the SAM
// can stretch a transition cycle to 25 master ticks.
#[cfg(test)]
const MAX_SAM_BUS_CYCLE_MASTER_TICKS: u64 = 25;
const SMOKE_START_SETTLE_FRAMES: u32 = 60;

const USAGE: &str = "\
Usage: emu198x-dragon --headless --rom PATH [OPTIONS]   (add --no-default-features for graphics-free builds)

Firmware:
    --model MODEL       dragon32 | dragon64 [default: dragon32]
    --rom PATH          Dragon 32 BASIC ROM, or Dragon 64 compatible-mode ROM; .zip archives are accepted
    --rom64 PATH        Dragon 64 64-mode BASIC ROM, required with --model dragon64
    --cart PATH         Dragon cartridge ROM/DGN image; .zip archives are accepted
    --disk PATH         DragonDOS VDK disk image; .zip archives are accepted
    --bin PATH          DragonDOS .BIN program image; .zip archives are accepted
    --snapshot PATH     PC-Dragon PAK snapshot; .zip archives are accepted

Execution:
    --cycles N         maximum MC6809 bus cycles to run [default: 100000]
    --type-command S   boot through the runtime path, type a BASIC/DragonDOS command, then run --cycles
    --trace-limit N    number of recent instruction fetches to retain [default: 64]
    --watch-fetch A[-B]
                       retain opcode fetches in inclusive hex/decimal address range A..B; may be repeated
    --watch-write A[-B]
                       retain bus writes to inclusive hex/decimal address range A..B; may be repeated
    --press KEY        hold a named Dragon key closed; may be repeated
    --press-matrix R,C hold a raw keyboard matrix switch closed; may be repeated
    --dump-ram P       write the current 32 KiB RAM image as raw bytes
    --disk-output P    write the current mutated drive-1 VDK image to PATH
    --dump-text        print the current 32x16 MC6847 text snapshot
    --dump-text-png P  write the current border-inclusive MC6847 text framebuffer as a PNG
    --screenshot P     write the current border-inclusive MC6847 framebuffer as a PNG
    --screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --screenshot-phase PHASE
                       screenshot capture phase: immediate | completed-frame [default: immediate]
    --screenshot-source SOURCE
                       screenshot source: beam | static [default: beam]
    --smoke-root PATH  recursively scan .cas/.zip Dragon tape images
    --bin-smoke-root PATH
                       recursively scan .bin/.zip DragonDOS binary images
    --snapshot-smoke-root PATH
                       recursively scan .pak/.zip PC-Dragon snapshots
    --disk-smoke-root PATH
                       recursively scan .vdk/.zip DragonDOS disks and run DIR
    --disk-smoke-launch
                       with --disk-smoke-root, launch the first BASIC/BIN program instead of DIR
    --smoke-run-limit N
                       run real-ROM CLOAD/CLOADM or snapshot smoke for first N parsed media [default: 8]
    --smoke-report P   write smoke matrix JSON to PATH
    --smoke-screenshot-dir PATH
                       write load/start screenshots for runtime-smoked tapes
    --smoke-screenshot-format FORMAT
                       screenshot format: diagnostic | xroar-zoomed [default: diagnostic]
    --smoke-audio-dir PATH
                       write load/start WAV audio captures for runtime-smoked tapes
    --smoke-joystick PORT,CONTROL,FRAMES
                       after start, hold joystick control on port 1/2 for N frames;
                       CONTROL is up, down, left, right, fire, or idle; may be repeated
    --smoke-joystick-axis PORT,AXIS,VALUE,FRAMES
                       after start, hold analogue axis x/y on port 1/2 at VALUE for N frames;
                       VALUE is normalized from -1.0 to 1.0; may be repeated
    --smoke-joystick-axis-sweep PORT,AXIS,START,END,STEPS,FRAMES
                       after start, sweep analogue axis x/y over normalized START..END;
                       records whether each step changes visible output
    --smoke-idle-after-start FRAMES
                       after start, run N frames without extra input and capture idle output
    --xroar-bin PATH   patched XRoar binary used to write reference PNGs
    --xroar-reference-dir PATH
                       write patched-XRoar reference PNGs for runtime-smoked media
    --xroar-snapshot-out PATH
                       write the synthetic XRoar v2 snapshot used for reference comparison
    --xroar-motoroff N capture CAS reference on the Nth tape motor-off [default: auto]
    --xroar-settle-seconds N
                       wait N emulated seconds after CAS reference trigger before capture [default: 3];
                       snapshot references instead use the local screenshot cycle count
    --xroar-timeout-seconds N
                       hard XRoar run timeout in emulated seconds [default: 45]

Other:
    --help             print this help text
";

#[derive(Debug, Clone, PartialEq)]
struct Cli {
    model: Model,
    rom: PathBuf,
    mode_rom: Option<PathBuf>,
    cart: Option<PathBuf>,
    disk: Option<PathBuf>,
    bin: Option<PathBuf>,
    snapshot: Option<PathBuf>,
    cycles: u64,
    type_command: Option<String>,
    trace_limit: usize,
    fetch_watch: Vec<AddressRange>,
    write_watch: Vec<AddressRange>,
    pressed_keys: Vec<MatrixKey>,
    dump_ram: Option<PathBuf>,
    disk_output: Option<PathBuf>,
    dump_text: bool,
    dump_text_png: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    screenshot_format: SmokeScreenshotFormat,
    screenshot_phase: SmokeScreenshotPhase,
    screenshot_source: ScreenshotSource,
    smoke_root: Option<PathBuf>,
    bin_smoke_root: Option<PathBuf>,
    snapshot_smoke_root: Option<PathBuf>,
    disk_smoke_root: Option<PathBuf>,
    disk_smoke_launch: bool,
    smoke_run_limit: usize,
    smoke_report: Option<PathBuf>,
    smoke_screenshot_dir: Option<PathBuf>,
    smoke_screenshot_format: SmokeScreenshotFormat,
    smoke_audio_dir: Option<PathBuf>,
    smoke_joystick: Vec<SmokeJoystickStep>,
    smoke_joystick_axis: Vec<SmokeJoystickAxisStep>,
    smoke_joystick_axis_sweep: Vec<SmokeJoystickAxisSweep>,
    smoke_idle_after_start: u32,
    xroar_bin: Option<PathBuf>,
    xroar_reference_dir: Option<PathBuf>,
    xroar_snapshot_out: Option<PathBuf>,
    xroar_motoroff: Option<usize>,
    xroar_settle_seconds: f32,
    xroar_timeout_seconds: f32,
}

struct LoadedDragonFirmware {
    model: Model,
    rom: [u8; ROM_SIZE],
    mode_rom: Option<[u8; ROM_SIZE]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessReport {
    stop_reason: StopReason,
    cycles: u64,
    master_ticks: u64,
    instructions: u64,
    pc: u16,
    addr: u16,
    rw: bool,
    last_fetch: Option<FetchTrace>,
    trace: Vec<FetchTrace>,
    dropped_trace: usize,
    device_accesses: Vec<DeviceAccess>,
    dropped_device_accesses: usize,
    readonly_writes: Vec<ReadonlyWrite>,
    dropped_readonly_writes: usize,
    watched_fetches: Vec<WatchedFetchTrace>,
    dropped_watched_fetches: usize,
    watched_writes: Vec<MemoryWriteTrace>,
    dropped_watched_writes: usize,
    pia_signals: Vec<PiaSignalTrace>,
    dropped_pia_signals: usize,
    interrupt_lines: Vec<CpuInterruptLineTrace>,
    dropped_interrupt_lines: usize,
    interrupt_accepts: Vec<CpuInterruptAcceptTrace>,
    dropped_interrupt_accepts: usize,
    vdg_samples: Vec<VdgSampleTrace>,
    dropped_vdg_samples: usize,
    vdg_mode_writes: Vec<VdgModeWriteTrace>,
    dropped_vdg_mode_writes: usize,
    text_screen_base: u16,
    text_screen: Option<String>,
    ram: Option<Vec<u8>>,
    text_framebuffer: Option<Vec<u32>>,
    framebuffer: Option<Vec<u32>>,
    framebuffer_cycles: Option<u64>,
    framebuffer_master_ticks: Option<u64>,
    video_phase: DragonVideoPhase,
    disk_vdk: Option<Vec<u8>>,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixReport {
    tape_count: usize,
    runtime_smokes: usize,
    rows: Vec<SmokeMatrixRow>,
}

#[derive(Debug, Serialize)]
struct SmokeMatrixRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksums_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_segments: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<CasHeaderSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<CasRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnapshotSmokeMatrixReport {
    snapshot_count: usize,
    runtime_smokes: usize,
    rows: Vec<SnapshotSmokeRow>,
}

#[derive(Debug, Serialize)]
struct BinSmokeMatrixReport {
    program_count: usize,
    runtime_smokes: usize,
    rows: Vec<BinSmokeRow>,
}

#[derive(Debug, Serialize)]
struct DiskSmokeMatrixReport {
    disk_count: usize,
    runtime_smokes: usize,
    rows: Vec<DiskSmokeRow>,
}

#[derive(Debug, Serialize)]
struct DiskSmokeRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tracks: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sides: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sectors_per_track: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sector_size: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    directory_entries: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    directory: Option<Vec<DragonDosDirectoryEntrySummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<DiskRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DragonDosDirectoryEntrySummary {
    name: String,
    extension: String,
}

#[derive(Debug, Serialize)]
struct BinSmokeRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_address: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_address: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<BinRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BinRuntimeSmoke {
    classification: SnapshotSmokeClassification,
    stop_reason: String,
    cycles: u64,
    instructions: u64,
    pc: u16,
    text_screen_base: u16,
    distinct_colors: usize,
    non_background_pixels: usize,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_cycles: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_master_ticks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_frame_phase_cycles: Option<u64>,
    video_phase: VideoPhaseSummary,
    trace_signature: SnapshotTraceSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    vdg_trace: Option<VdgTraceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DiskRuntimeSmoke {
    classification: DiskSmokeClassification,
    command: String,
    stop_reason: String,
    cycles: u64,
    instructions: u64,
    pc: u16,
    text_screen_base: u16,
    distinct_colors: usize,
    non_background_pixels: usize,
    screen_text: Vec<String>,
    disk_trace_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
    trace_signature: SnapshotTraceSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum DiskSmokeClassification {
    Error,
    NoDiskAccess,
    DirectoryError,
    DirectoryVisible,
    NoLaunchCandidate,
    LaunchError,
    LaunchVisible,
    LaunchBlank,
}

#[derive(Debug, Serialize)]
struct SnapshotSmokeRow {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_member: Option<String>,
    parse_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<SnapshotRuntimeSmoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SnapshotRuntimeSmoke {
    classification: SnapshotSmokeClassification,
    stop_reason: String,
    cycles: u64,
    instructions: u64,
    pc: u16,
    load_address: u16,
    ram_len: usize,
    text_screen_base: u16,
    distinct_colors: usize,
    non_background_pixels: usize,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_cycles: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_master_ticks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_frame_phase_cycles: Option<u64>,
    video_phase: VideoPhaseSummary,
    trace_signature: SnapshotTraceSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_settle_seconds: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_trap_pc: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_trap_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison: Option<ImageComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vdg_trace: Option<VdgTraceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct VideoPhaseSummary {
    frame_master_tick: u64,
    physical_line: usize,
    line_master_tick: u64,
    visible_line: Option<usize>,
    active_y: Option<usize>,
    active_x: Option<usize>,
}

#[derive(Debug, Serialize)]
struct VdgTraceSummary {
    dropped_samples: usize,
    dropped_mode_writes: usize,
    dropped_device_accesses: usize,
    samples: Vec<VdgSampleSummary>,
    mode_writes: Vec<VdgModeWriteSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SnapshotTraceSignature {
    hash: String,
    trace_entries: usize,
    vdg_samples: usize,
    vdg_mode_writes: usize,
    framebuffer_words: usize,
}

#[derive(Debug, Serialize)]
struct VdgSampleSummary {
    cycle: u64,
    frame_master_tick: u64,
    line: usize,
    active_y: usize,
    byte_x: usize,
    display_base: u16,
    sam_video_mode: u8,
    sam_display_offset: u8,
    pia1_pb: u8,
    graphics: bool,
    css: bool,
    int_ext: bool,
    gm: u8,
}

#[derive(Debug, Serialize)]
struct VdgModeWriteSummary {
    cycle: u64,
    frame_master_tick: u64,
    line: Option<usize>,
    active_y: Option<usize>,
    active_x: Option<usize>,
    addr: u16,
    value: u8,
    graphics: bool,
    css: bool,
    int_ext: bool,
    gm: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SnapshotSmokeClassification {
    RunningVisible,
    RunningBlank,
    HaltedVisible,
    HaltedBlank,
    Error,
}

#[derive(Debug, Serialize)]
struct CasHeaderSummary {
    name: String,
    file_type: String,
    ascii_flag: u8,
    gap_flag: u8,
    first_address: u16,
    second_address: u16,
}

#[derive(Debug, Serialize)]
struct CasRuntimeSmoke {
    command: String,
    classification: RuntimeSmokeClassification,
    load_result: String,
    start_command: String,
    start_result: String,
    load_visible_change: bool,
    load_basic_error: bool,
    load_pc_before: u16,
    load_pc_after: u16,
    start_pc_after: u16,
    load_video: DragonVideoState,
    start_video: DragonVideoState,
    tape_position_bits: u64,
    tape_length_bits: u64,
    tape_finished: bool,
    visible_change_after_start: bool,
    start_video_changed: bool,
    start_settle_visible_change: bool,
    basic_error: bool,
    load_screen_text: Vec<String>,
    screen_text: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_audio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_audio: Option<String>,
    idle_after_start_frames: u32,
    idle_visible_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    idle_screen_text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idle_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    joystick_steps: Vec<SmokeJoystickStep>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    joystick_axis_steps: Vec<SmokeJoystickAxisStep>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    joystick_axis_sweeps: Vec<SmokeJoystickAxisSweep>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    joystick_axis_sweep_results: Vec<SmokeJoystickAxisSweepResult>,
    joystick_visible_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    joystick_screen_text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joystick_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_motoroff: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison: Option<ImageComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xroar_reference_comparison_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ImageComparison {
    emu_width: u32,
    emu_height: u32,
    reference_width: u32,
    reference_height: u32,
    dimensions_match: bool,
    compared_pixels: u64,
    differing_pixels: u64,
    differing_pixel_percent: f64,
    max_channel_delta: u8,
    mean_abs_channel_delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum RuntimeSmokeClassification {
    Error,
    LoadBasicError,
    LoadedNoVisibleChange,
    LoadedVisibleChange,
    MachineCodeRunningAfterLoad,
    StartedBasicError,
    StartedNoVisibleChange,
    StartedTextDrawing,
    StartedVideoControlChanged,
    StartedGraphicsBlank,
    StartedGraphicsDrawing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct DragonVideoState {
    sam_video_mode: u8,
    sam_display_offset: u8,
    display_base: u16,
    pia1_output_b: u8,
    pia1_ddr_b: u8,
    pia1_control_b: u8,
    pia1_cb2: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct XroarReferenceConfig {
    bin: PathBuf,
    output_dir: PathBuf,
    motoroff: Option<usize>,
    settle_seconds: f32,
    timeout_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeScreenshotFormat {
    Diagnostic,
    XroarZoomed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeScreenshotPhase {
    Immediate,
    CompletedFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenshotSource {
    Beam,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SmokeJoystickStep {
    port: u8,
    control: SmokeJoystickControl,
    frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SmokeJoystickAxisStep {
    port: u8,
    axis: SmokeJoystickAxis,
    value: i16,
    frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SmokeJoystickAxisSweep {
    port: u8,
    axis: SmokeJoystickAxis,
    start: i16,
    end: i16,
    steps: u32,
    frames: u32,
}

#[derive(Debug, Serialize)]
struct SmokeJoystickAxisSweepResult {
    port: u8,
    axis: SmokeJoystickAxis,
    step: u32,
    value: i16,
    frames: u32,
    visible_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SmokeJoystickControl {
    Up,
    Down,
    Left,
    Right,
    Fire,
    Idle,
}

impl SmokeJoystickControl {
    const fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Fire => "fire",
            Self::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SmokeJoystickAxis {
    X,
    Y,
}

impl SmokeJoystickAxis {
    const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RuntimeSmokeOptions<'a> {
    run_limit: usize,
    screenshot_stem: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    audio_stem: Option<&'a Path>,
    joystick: &'a [SmokeJoystickStep],
    joystick_axis: &'a [SmokeJoystickAxisStep],
    joystick_axis_sweep: &'a [SmokeJoystickAxisSweep],
    idle_after_start_frames: u32,
    xroar: Option<&'a XroarReferenceConfig>,
    xroar_stem: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug)]
struct SnapshotSmokeOptions<'a> {
    run_limit: usize,
    screenshot_path: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    screenshot_phase: SmokeScreenshotPhase,
    cycle_limit: u64,
    trace_limit: usize,
    xroar: Option<&'a XroarReferenceConfig>,
    xroar_stem: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug)]
struct BinSmokeOptions<'a> {
    run_limit: usize,
    screenshot_path: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    screenshot_phase: SmokeScreenshotPhase,
    cycle_limit: u64,
    trace_limit: usize,
}

#[derive(Clone, Copy, Debug)]
struct DiskSmokeOptions<'a> {
    run_limit: usize,
    screenshot_path: Option<&'a Path>,
    screenshot_format: SmokeScreenshotFormat,
    cycle_limit: u64,
}

/// Headless entry point: runs the bring-up and verification harness
/// (smoke matrices, typed-command runs, XRoar comparison, direct MC6809
/// harness). The dispatcher in `main.rs` routes here when a
/// headless-only flag is present.
pub fn run(args: Vec<String>) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }

    let cli = parse_cli(args)?;
    let firmware = load_dragon_firmware(&cli)?;
    let rom = &firmware.rom;
    let cart = cli
        .cart
        .as_ref()
        .map(|path| load_cartridge(path))
        .transpose()?;
    let disk = cli.disk.as_ref().map(|path| load_disk(path)).transpose()?;
    let bin = cli
        .bin
        .as_ref()
        .map(|path| load_binary_program(path))
        .transpose()?;
    let snapshot = cli
        .snapshot
        .as_ref()
        .map(|path| load_snapshot(path))
        .transpose()?;
    if let Some(path) = &cli.xroar_snapshot_out {
        ensure_dragon32_harness(&cli, "--xroar-snapshot-out")?;
        let snapshot = snapshot
            .as_ref()
            .ok_or_else(|| "--xroar-snapshot-out requires --snapshot".to_owned())?;
        write_xroar_snapshot_out(&cli, snapshot, path)?;
        println!("xroar snapshot: {}", path.display());
    }
    if cli.smoke_root.is_some() {
        let report = run_smoke_matrix(&cli, &firmware)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize smoke matrix report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }
    if cli.bin_smoke_root.is_some() {
        let report = run_bin_smoke_matrix(&cli, &firmware)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize BIN smoke report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }
    if cli.snapshot_smoke_root.is_some() {
        let report = run_snapshot_smoke_matrix(&cli, &firmware)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize snapshot smoke report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }
    if cli.disk_smoke_root.is_some() {
        let report = run_disk_smoke_matrix(&cli, &firmware)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to serialize disk smoke report: {err}"))?;
        if let Some(path) = &cli.smoke_report {
            fs::write(path, json.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }
    if let Some(command) = &cli.type_command {
        let report = run_typed_command(&cli, &firmware, command)?;
        print_typed_command_report(&report);
        if let Some(path) = &cli.screenshot {
            fs::write(path, &report.screenshot_png)
                .map_err(|err| format!("failed to write screenshot {}: {err}", path.display()))?;
        }
        if let Some(path) = &cli.disk_output {
            write_exported_disk(path, report.disk_vdk.as_deref())?;
            println!("disk: {}", path.display());
        }
        return Ok(());
    }

    ensure_dragon32_harness(&cli, "direct harness mode")?;
    let keyboard =
        DragonKeyboard::with_pressed_keys(&cli.pressed_keys).map_err(|err| err.to_string())?;
    let report = run_harness_with_keyboard(
        rom,
        keyboard,
        HarnessRunOptions {
            cartridge: cart.as_ref(),
            disk: disk.as_ref(),
            program: bin.as_ref(),
            snapshot: snapshot.as_ref(),
            cycle_limit: cli.cycles,
            trace_limit: cli.trace_limit,
            fetch_watch: cli.fetch_watch.clone(),
            write_watch: cli.write_watch.clone(),
            dump_text: cli.dump_text,
            dump_ram: cli.dump_ram.is_some(),
            export_disk: cli.disk_output.is_some(),
            dump_text_framebuffer: cli.dump_text_png.is_some(),
            capture_framebuffer: cli.screenshot.is_some(),
            capture_framebuffer_phase: cli.screenshot_phase,
            capture_framebuffer_source: cli.screenshot_source,
        },
    );
    print_report(&report);
    if let Some(path) = &cli.dump_ram {
        let ram = report
            .ram
            .as_deref()
            .ok_or_else(|| "RAM was not captured".to_owned())?;
        fs::write(path, ram).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        println!("ram: {}", path.display());
    }
    if let Some(path) = &cli.disk_output {
        write_exported_disk(path, report.disk_vdk.as_deref())?;
        println!("disk: {}", path.display());
    }
    if let Some(path) = &cli.dump_text_png {
        let framebuffer = report
            .text_framebuffer
            .as_deref()
            .ok_or_else(|| "text framebuffer was not captured".to_owned())?;
        write_text_png(path, framebuffer)?;
        println!("text png: {}", path.display());
    }
    if let Some(path) = &cli.screenshot {
        let framebuffer = report
            .framebuffer
            .as_deref()
            .ok_or_else(|| "framebuffer was not captured".to_owned())?;
        write_screenshot_png(
            path,
            framebuffer,
            cli.screenshot_format,
            report.framebuffer_cycles.unwrap_or(report.cycles),
        )?;
        println!("screenshot: {}", path.display());
    }
    Ok(())
}

fn parse_cli<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut model = Model::Dragon32Pal;
    let mut rom = None;
    let mut mode_rom = None;
    let mut cart = None;
    let mut disk = None;
    let mut bin = None;
    let mut snapshot = None;
    let mut cycles = DEFAULT_CYCLES;
    let mut type_command = None;
    let mut trace_limit = DEFAULT_TRACE_LIMIT;
    let mut fetch_watch = Vec::new();
    let mut write_watch = Vec::new();
    let mut pressed_keys = Vec::new();
    let mut dump_ram = None;
    let mut disk_output = None;
    let mut dump_text = false;
    let mut dump_text_png = None;
    let mut screenshot = None;
    let mut screenshot_format = SmokeScreenshotFormat::Diagnostic;
    let mut screenshot_phase = SmokeScreenshotPhase::Immediate;
    let mut screenshot_source = ScreenshotSource::Beam;
    let mut smoke_root = None;
    let mut bin_smoke_root = None;
    let mut snapshot_smoke_root = None;
    let mut disk_smoke_root = None;
    let mut disk_smoke_launch = false;
    let mut smoke_run_limit = DEFAULT_SMOKE_RUN_LIMIT;
    let mut smoke_report = None;
    let mut smoke_screenshot_dir = None;
    let mut smoke_screenshot_format = SmokeScreenshotFormat::Diagnostic;
    let mut smoke_audio_dir = None;
    let mut smoke_joystick = Vec::new();
    let mut smoke_joystick_axis = Vec::new();
    let mut smoke_joystick_axis_sweep = Vec::new();
    let mut smoke_idle_after_start = 0;
    let mut xroar_bin = None;
    let mut xroar_reference_dir = None;
    let mut xroar_snapshot_out = None;
    let mut xroar_motoroff = None;
    let mut xroar_settle_seconds = DEFAULT_XROAR_SETTLE_SECONDS;
    let mut xroar_timeout_seconds = DEFAULT_XROAR_TIMEOUT_SECONDS;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--model" => {
                model = parse_model(&next_value(&mut iter, "--model")?)?;
            }
            "--rom" => {
                rom = Some(PathBuf::from(next_value(&mut iter, "--rom")?));
            }
            "--rom64" => {
                mode_rom = Some(PathBuf::from(next_value(&mut iter, "--rom64")?));
            }
            "--cart" => {
                cart = Some(PathBuf::from(next_value(&mut iter, "--cart")?));
            }
            "--disk" => {
                disk = Some(PathBuf::from(next_value(&mut iter, "--disk")?));
            }
            "--bin" => {
                bin = Some(PathBuf::from(next_value(&mut iter, "--bin")?));
            }
            "--snapshot" => {
                snapshot = Some(PathBuf::from(next_value(&mut iter, "--snapshot")?));
            }
            "--cycles" => {
                cycles = parse_u64(&next_value(&mut iter, "--cycles")?, "--cycles")?;
            }
            "--type-command" => {
                type_command = Some(next_value(&mut iter, "--type-command")?);
            }
            "--trace-limit" => {
                trace_limit =
                    parse_usize(&next_value(&mut iter, "--trace-limit")?, "--trace-limit")?;
            }
            "--watch-fetch" => {
                fetch_watch.push(parse_address_range(
                    &next_value(&mut iter, "--watch-fetch")?,
                    "--watch-fetch",
                )?);
            }
            "--watch-write" => {
                write_watch.push(parse_address_range(
                    &next_value(&mut iter, "--watch-write")?,
                    "--watch-write",
                )?);
            }
            "--press" => {
                let key = parse_dragon_key(&next_value(&mut iter, "--press")?)?;
                pressed_keys.push(MatrixKey::from_dragon_key(key));
            }
            "--press-matrix" => {
                pressed_keys.push(parse_matrix_key(&next_value(&mut iter, "--press-matrix")?)?);
            }
            "--dump-ram" => {
                dump_ram = Some(PathBuf::from(next_value(&mut iter, "--dump-ram")?));
            }
            "--disk-output" => {
                disk_output = Some(PathBuf::from(next_value(&mut iter, "--disk-output")?));
            }
            "--dump-text" => {
                dump_text = true;
            }
            "--dump-text-png" => {
                dump_text_png = Some(PathBuf::from(next_value(&mut iter, "--dump-text-png")?));
            }
            "--screenshot" => {
                screenshot = Some(PathBuf::from(next_value(&mut iter, "--screenshot")?));
            }
            "--screenshot-format" => {
                screenshot_format = parse_screenshot_format(
                    &next_value(&mut iter, "--screenshot-format")?,
                    "--screenshot-format",
                )?;
            }
            "--screenshot-phase" => {
                screenshot_phase = parse_screenshot_phase(
                    &next_value(&mut iter, "--screenshot-phase")?,
                    "--screenshot-phase",
                )?;
            }
            "--screenshot-source" => {
                screenshot_source = parse_screenshot_source(
                    &next_value(&mut iter, "--screenshot-source")?,
                    "--screenshot-source",
                )?;
            }
            "--smoke-root" => {
                smoke_root = Some(PathBuf::from(next_value(&mut iter, "--smoke-root")?));
            }
            "--bin-smoke-root" => {
                bin_smoke_root = Some(PathBuf::from(next_value(&mut iter, "--bin-smoke-root")?));
            }
            "--snapshot-smoke-root" => {
                snapshot_smoke_root = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--snapshot-smoke-root",
                )?));
            }
            "--disk-smoke-root" => {
                disk_smoke_root = Some(PathBuf::from(next_value(&mut iter, "--disk-smoke-root")?));
            }
            "--disk-smoke-launch" => {
                disk_smoke_launch = true;
            }
            "--smoke-run-limit" => {
                smoke_run_limit = parse_usize(
                    &next_value(&mut iter, "--smoke-run-limit")?,
                    "--smoke-run-limit",
                )?;
            }
            "--smoke-report" => {
                smoke_report = Some(PathBuf::from(next_value(&mut iter, "--smoke-report")?));
            }
            "--smoke-screenshot-dir" => {
                smoke_screenshot_dir = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--smoke-screenshot-dir",
                )?));
            }
            "--smoke-screenshot-format" => {
                smoke_screenshot_format = parse_screenshot_format(
                    &next_value(&mut iter, "--smoke-screenshot-format")?,
                    "--smoke-screenshot-format",
                )?;
            }
            "--smoke-audio-dir" => {
                smoke_audio_dir = Some(PathBuf::from(next_value(&mut iter, "--smoke-audio-dir")?));
            }
            "--smoke-joystick" => {
                smoke_joystick.push(parse_smoke_joystick_step(&next_value(
                    &mut iter,
                    "--smoke-joystick",
                )?)?);
            }
            "--smoke-joystick-axis" => {
                smoke_joystick_axis.push(parse_smoke_joystick_axis_step(&next_value(
                    &mut iter,
                    "--smoke-joystick-axis",
                )?)?);
            }
            "--smoke-joystick-axis-sweep" => {
                smoke_joystick_axis_sweep.push(parse_smoke_joystick_axis_sweep(&next_value(
                    &mut iter,
                    "--smoke-joystick-axis-sweep",
                )?)?);
            }
            "--smoke-idle-after-start" => {
                smoke_idle_after_start = parse_u32(
                    &next_value(&mut iter, "--smoke-idle-after-start")?,
                    "--smoke-idle-after-start",
                )?;
            }
            "--xroar-bin" => {
                xroar_bin = Some(PathBuf::from(next_value(&mut iter, "--xroar-bin")?));
            }
            "--xroar-reference-dir" => {
                xroar_reference_dir = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--xroar-reference-dir",
                )?));
            }
            "--xroar-snapshot-out" => {
                xroar_snapshot_out = Some(PathBuf::from(next_value(
                    &mut iter,
                    "--xroar-snapshot-out",
                )?));
            }
            "--xroar-motoroff" => {
                xroar_motoroff = Some(parse_usize(
                    &next_value(&mut iter, "--xroar-motoroff")?,
                    "--xroar-motoroff",
                )?);
            }
            "--xroar-settle-seconds" => {
                xroar_settle_seconds = parse_f32(
                    &next_value(&mut iter, "--xroar-settle-seconds")?,
                    "--xroar-settle-seconds",
                )?;
            }
            "--xroar-timeout-seconds" => {
                xroar_timeout_seconds = parse_f32(
                    &next_value(&mut iter, "--xroar-timeout-seconds")?,
                    "--xroar-timeout-seconds",
                )?;
            }
            "--help" | "-h" => return Err(USAGE.to_owned()),
            "--headless" => {}
            _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
        }
    }

    let smoke_modes = [
        smoke_root.is_some(),
        bin_smoke_root.is_some(),
        snapshot_smoke_root.is_some(),
        disk_smoke_root.is_some(),
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    if smoke_modes > 1 {
        return Err(
            "--smoke-root, --bin-smoke-root, --snapshot-smoke-root, and --disk-smoke-root cannot be combined"
                .to_owned(),
        );
    }

    Ok(Cli {
        model,
        rom: rom.ok_or_else(|| format!("missing required --rom PATH\n\n{USAGE}"))?,
        mode_rom,
        cart,
        disk,
        bin,
        snapshot,
        cycles,
        type_command,
        trace_limit,
        fetch_watch,
        write_watch,
        pressed_keys,
        dump_ram,
        disk_output,
        dump_text,
        dump_text_png,
        screenshot,
        screenshot_format,
        screenshot_phase,
        screenshot_source,
        smoke_root,
        bin_smoke_root,
        snapshot_smoke_root,
        disk_smoke_root,
        disk_smoke_launch,
        smoke_run_limit,
        smoke_report,
        smoke_screenshot_dir,
        smoke_screenshot_format,
        smoke_audio_dir,
        smoke_joystick,
        smoke_joystick_axis,
        smoke_joystick_axis_sweep,
        smoke_idle_after_start,
        xroar_bin,
        xroar_reference_dir,
        xroar_snapshot_out,
        xroar_motoroff,
        xroar_settle_seconds,
        xroar_timeout_seconds,
    })
}

fn next_value<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value\n\n{USAGE}"))
}

fn parse_model(value: &str) -> Result<Model, String> {
    match value {
        "dragon32" | "dragon-32" | "dragon-32-pal" => Ok(Model::Dragon32Pal),
        "dragon64" | "dragon-64" | "dragon-64-pal" => Ok(Model::Dragon64Pal),
        _ => Err("--model expects dragon32 or dragon64".to_owned()),
    }
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|err| format!("invalid {flag} value {value}: {err}"))
    } else {
        value
            .parse()
            .map_err(|err| format!("invalid {flag} value {value}: {err}"))
    }
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    let parsed = parse_u64(value, flag)?;
    usize::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u8(value: &str, flag: &str) -> Result<u8, String> {
    let parsed = parse_u64(value, flag)?;
    u8::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u16(value: &str, flag: &str) -> Result<u16, String> {
    let parsed = parse_u64(value, flag)?;
    u16::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_u32(value: &str, flag: &str) -> Result<u32, String> {
    let parsed = parse_u64(value, flag)?;
    u32::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_address_range(value: &str, flag: &str) -> Result<AddressRange, String> {
    let (start, end) = value
        .split_once('-')
        .map_or((value, value), |(start, end)| (start, end));
    if start.is_empty() || end.is_empty() {
        return Err(format!("invalid {flag} value {value}; expected A or A-B"));
    }
    let start = parse_u16(start, flag)?;
    let end = parse_u16(end, flag)?;
    if start > end {
        return Err(format!(
            "invalid {flag} value {value}; start must be <= end"
        ));
    }
    Ok(AddressRange::new(start, end))
}

fn parse_f32(value: &str, flag: &str) -> Result<f32, String> {
    let parsed: f32 = value
        .parse()
        .map_err(|err| format!("invalid {flag} value {value}: {err}"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(format!(
            "{flag} value {value} must be a non-negative finite number"
        ));
    }
    Ok(parsed)
}

fn parse_screenshot_format(value: &str, flag: &str) -> Result<SmokeScreenshotFormat, String> {
    match value {
        "diagnostic" => Ok(SmokeScreenshotFormat::Diagnostic),
        "xroar-zoomed" => Ok(SmokeScreenshotFormat::XroarZoomed),
        _ => Err(format!(
            "invalid {flag} value {value}; expected diagnostic or xroar-zoomed"
        )),
    }
}

fn parse_screenshot_phase(value: &str, flag: &str) -> Result<SmokeScreenshotPhase, String> {
    match value {
        "immediate" => Ok(SmokeScreenshotPhase::Immediate),
        "completed-frame" => Ok(SmokeScreenshotPhase::CompletedFrame),
        _ => Err(format!(
            "invalid {flag} value {value}; expected immediate or completed-frame"
        )),
    }
}

fn parse_screenshot_source(value: &str, flag: &str) -> Result<ScreenshotSource, String> {
    match value {
        "beam" => Ok(ScreenshotSource::Beam),
        "static" => Ok(ScreenshotSource::Static),
        _ => Err(format!(
            "invalid {flag} value {value}; expected beam or static"
        )),
    }
}

fn parse_smoke_joystick_step(value: &str) -> Result<SmokeJoystickStep, String> {
    let mut parts = value.split(',');
    let port = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    let control = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    let frames = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_value(value))?;
    if parts.next().is_some() {
        return Err(invalid_smoke_joystick_value(value));
    }

    let port = parse_u8(port, "--smoke-joystick port")?;
    if !matches!(port, 1 | 2) {
        return Err(format!(
            "invalid --smoke-joystick port {port}; expected 1 or 2"
        ));
    }
    let frames = parse_u32(frames, "--smoke-joystick frames")?;
    if frames == 0 {
        return Err("--smoke-joystick frames must be greater than zero".to_owned());
    }
    Ok(SmokeJoystickStep {
        port,
        control: parse_smoke_joystick_control(control)?,
        frames,
    })
}

fn invalid_smoke_joystick_value(value: &str) -> String {
    format!("invalid --smoke-joystick value {value}; expected PORT,CONTROL,FRAMES")
}

fn parse_smoke_joystick_control(value: &str) -> Result<SmokeJoystickControl, String> {
    match value.to_ascii_lowercase().as_str() {
        "up" => Ok(SmokeJoystickControl::Up),
        "down" => Ok(SmokeJoystickControl::Down),
        "left" => Ok(SmokeJoystickControl::Left),
        "right" => Ok(SmokeJoystickControl::Right),
        "fire" => Ok(SmokeJoystickControl::Fire),
        "idle" => Ok(SmokeJoystickControl::Idle),
        _ => Err(format!(
            "invalid --smoke-joystick control {value}; expected up, down, left, right, fire, or idle"
        )),
    }
}

fn parse_smoke_joystick_axis_step(value: &str) -> Result<SmokeJoystickAxisStep, String> {
    let mut parts = value.split(',');
    let port = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_value(value))?;
    let axis = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_value(value))?;
    let axis_value = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_value(value))?;
    let frames = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_value(value))?;
    if parts.next().is_some() {
        return Err(invalid_smoke_joystick_axis_value(value));
    }

    let port = parse_u8(port, "--smoke-joystick-axis port")?;
    if !matches!(port, 1 | 2) {
        return Err(format!(
            "invalid --smoke-joystick-axis port {port}; expected 1 or 2"
        ));
    }
    let value = parse_smoke_axis_value(axis_value)?;
    let frames = parse_u32(frames, "--smoke-joystick-axis frames")?;
    if frames == 0 {
        return Err("--smoke-joystick-axis frames must be greater than zero".to_owned());
    }
    Ok(SmokeJoystickAxisStep {
        port,
        axis: parse_smoke_joystick_axis(axis)?,
        value,
        frames,
    })
}

fn parse_smoke_joystick_axis_sweep(value: &str) -> Result<SmokeJoystickAxisSweep, String> {
    let mut parts = value.split(',');
    let port = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    let axis = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    let start = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    let end = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    let steps = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    let frames = parts
        .next()
        .ok_or_else(|| invalid_smoke_joystick_axis_sweep_value(value))?;
    if parts.next().is_some() {
        return Err(invalid_smoke_joystick_axis_sweep_value(value));
    }

    let port = parse_u8(port, "--smoke-joystick-axis-sweep port")?;
    if !matches!(port, 1 | 2) {
        return Err(format!(
            "invalid --smoke-joystick-axis-sweep port {port}; expected 1 or 2"
        ));
    }
    let steps = parse_u32(steps, "--smoke-joystick-axis-sweep steps")?;
    if steps == 0 {
        return Err("--smoke-joystick-axis-sweep steps must be greater than zero".to_owned());
    }
    let frames = parse_u32(frames, "--smoke-joystick-axis-sweep frames")?;
    if frames == 0 {
        return Err("--smoke-joystick-axis-sweep frames must be greater than zero".to_owned());
    }

    Ok(SmokeJoystickAxisSweep {
        port,
        axis: parse_smoke_joystick_axis(axis)?,
        start: parse_smoke_axis_value(start)?,
        end: parse_smoke_axis_value(end)?,
        steps,
        frames,
    })
}

fn invalid_smoke_joystick_axis_value(value: &str) -> String {
    format!("invalid --smoke-joystick-axis value {value}; expected PORT,AXIS,VALUE,FRAMES")
}

fn invalid_smoke_joystick_axis_sweep_value(value: &str) -> String {
    format!(
        "invalid --smoke-joystick-axis-sweep value {value}; expected PORT,AXIS,START,END,STEPS,FRAMES"
    )
}

fn parse_smoke_joystick_axis(value: &str) -> Result<SmokeJoystickAxis, String> {
    match value.to_ascii_lowercase().as_str() {
        "x" => Ok(SmokeJoystickAxis::X),
        "y" => Ok(SmokeJoystickAxis::Y),
        _ => Err(format!(
            "invalid --smoke-joystick-axis axis {value}; expected x or y"
        )),
    }
}

fn parse_smoke_axis_value(value: &str) -> Result<i16, String> {
    let parsed: f32 = value
        .parse()
        .map_err(|err| format!("invalid --smoke-joystick-axis value {value}: {err}"))?;
    if !parsed.is_finite() || !(-1.0..=1.0).contains(&parsed) {
        return Err(format!(
            "--smoke-joystick-axis value {value} must be a finite number from -1.0 to 1.0"
        ));
    }
    Ok(normalize_axis_value(parsed))
}

fn normalize_axis_value(value: f32) -> i16 {
    let clamped = value.clamp(-1.0, 1.0);
    if clamped <= -1.0 {
        i16::MIN
    } else if clamped >= 1.0 {
        i16::MAX
    } else {
        (clamped * f32::from(i16::MAX)) as i16
    }
}

fn axis_sweep_value(sweep: SmokeJoystickAxisSweep, step: u32) -> i16 {
    if sweep.steps <= 1 {
        return sweep.start;
    }
    let fraction = f64::from(step) / f64::from(sweep.steps - 1);
    let value = f64::from(sweep.start) + (f64::from(sweep.end) - f64::from(sweep.start)) * fraction;
    (value + 0.5).floor() as i16
}

fn parse_matrix_key(value: &str) -> Result<MatrixKey, String> {
    let (row, column) = value
        .split_once(',')
        .ok_or_else(|| format!("invalid --press-matrix value {value}; expected R,C"))?;
    Ok(MatrixKey::new(
        parse_usize(row, "--press-matrix row")?,
        parse_usize(column, "--press-matrix column")?,
    ))
}

fn parse_dragon_key(value: &str) -> Result<DragonKey, String> {
    DragonKey::from_label(value).ok_or_else(|| {
        format!(
            "unknown Dragon key {value:?}; use a Dragon key label such as A, 1, @, enter, clear, break, shift, space, up, down, left, or right"
        )
    })
}

fn run_smoke_matrix(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
) -> Result<SmokeMatrixReport, String> {
    let root = cli
        .smoke_root
        .as_deref()
        .ok_or_else(|| "--smoke-root is required".to_owned())?;
    if cli.model == Model::Dragon64Pal
        && (cli.xroar_bin.is_some() || cli.xroar_reference_dir.is_some())
    {
        return Err(
            "Dragon 64 --smoke-root does not support XRoar references because the script only supplies one ROM to XRoar"
                .to_owned(),
        );
    }
    let xroar = xroar_reference_config(cli)?;
    let mut tapes = Vec::new();
    collect_tape_candidates(root, &mut tapes)?;
    tapes.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(dir) = &cli.smoke_audio_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(config) = &xroar {
        fs::create_dir_all(&config.output_dir).map_err(|err| {
            format!(
                "failed to create XRoar reference dir {}: {err}",
                config.output_dir.display()
            )
        })?;
    }

    let mut rows = Vec::with_capacity(tapes.len());
    let mut runtime_smokes = 0usize;
    for (index, tape_path) in tapes.iter().enumerate() {
        let screenshot_stem = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}", safe_stem(tape_path))));
        let xroar_stem = xroar.as_ref().map(|config| {
            config
                .output_dir
                .join(format!("{index:04}-{}", safe_stem(tape_path)))
        });
        let audio_stem = cli
            .smoke_audio_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}", safe_stem(tape_path))));
        let row = scan_tape_candidate(
            tape_path,
            firmware,
            &mut runtime_smokes,
            RuntimeSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_stem: screenshot_stem.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                audio_stem: audio_stem.as_deref(),
                joystick: &cli.smoke_joystick,
                joystick_axis: &cli.smoke_joystick_axis,
                joystick_axis_sweep: &cli.smoke_joystick_axis_sweep,
                idle_after_start_frames: cli.smoke_idle_after_start,
                xroar: xroar.as_ref(),
                xroar_stem: xroar_stem.as_deref(),
            },
        );
        rows.push(row);
    }

    Ok(SmokeMatrixReport {
        tape_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn run_snapshot_smoke_matrix(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
) -> Result<SnapshotSmokeMatrixReport, String> {
    let root = cli
        .snapshot_smoke_root
        .as_deref()
        .ok_or_else(|| "--snapshot-smoke-root is required".to_owned())?;
    if cli.model == Model::Dragon64Pal
        && (cli.xroar_bin.is_some() || cli.xroar_reference_dir.is_some())
    {
        return Err(
            "Dragon 64 --snapshot-smoke-root does not support XRoar references because the script only supplies one ROM to XRoar"
                .to_owned(),
        );
    }
    let xroar = xroar_reference_config(cli)?;
    let mut snapshots = Vec::new();
    collect_snapshot_candidates(root, &mut snapshots)?;
    snapshots.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }
    if let Some(config) = &xroar {
        fs::create_dir_all(&config.output_dir).map_err(|err| {
            format!(
                "failed to create XRoar reference dir {}: {err}",
                config.output_dir.display()
            )
        })?;
    }

    let mut rows = Vec::with_capacity(snapshots.len());
    let mut runtime_smokes = 0usize;
    for (index, snapshot_path) in snapshots.iter().enumerate() {
        let screenshot_path = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(snapshot_path))));
        let xroar_stem = xroar.as_ref().map(|config| {
            config
                .output_dir
                .join(format!("{index:04}-{}", safe_stem(snapshot_path)))
        });
        let row = scan_snapshot_candidate(
            snapshot_path,
            firmware,
            &mut runtime_smokes,
            SnapshotSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_path: screenshot_path.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                screenshot_phase: cli.screenshot_phase,
                cycle_limit: cli.cycles,
                trace_limit: cli.trace_limit,
                xroar: xroar.as_ref(),
                xroar_stem: xroar_stem.as_deref(),
            },
        );
        rows.push(row);
    }

    Ok(SnapshotSmokeMatrixReport {
        snapshot_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn run_bin_smoke_matrix(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
) -> Result<BinSmokeMatrixReport, String> {
    let root = cli
        .bin_smoke_root
        .as_deref()
        .ok_or_else(|| "--bin-smoke-root is required".to_owned())?;
    let mut programs = Vec::new();
    collect_bin_candidates(root, &mut programs)?;
    programs.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }

    let mut rows = Vec::with_capacity(programs.len());
    let mut runtime_smokes = 0usize;
    for (index, program_path) in programs.iter().enumerate() {
        let screenshot_path = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(program_path))));
        let row = scan_bin_candidate(
            program_path,
            firmware,
            &mut runtime_smokes,
            BinSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_path: screenshot_path.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                screenshot_phase: cli.screenshot_phase,
                cycle_limit: cli.cycles,
                trace_limit: cli.trace_limit,
            },
        );
        rows.push(row);
    }

    Ok(BinSmokeMatrixReport {
        program_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn run_disk_smoke_matrix(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
) -> Result<DiskSmokeMatrixReport, String> {
    let root = cli
        .disk_smoke_root
        .as_deref()
        .ok_or_else(|| "--disk-smoke-root is required".to_owned())?;
    let mut disks = Vec::new();
    collect_disk_candidates(root, &mut disks)?;
    disks.sort();
    if let Some(dir) = &cli.smoke_screenshot_dir {
        fs::create_dir_all(dir)
            .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    }

    let mut rows = Vec::with_capacity(disks.len());
    let mut runtime_smokes = 0usize;
    for (index, disk_path) in disks.iter().enumerate() {
        let screenshot_path = cli
            .smoke_screenshot_dir
            .as_ref()
            .map(|dir| dir.join(format!("{index:04}-{}.png", safe_stem(disk_path))));
        let row = scan_disk_candidate(
            disk_path,
            cli,
            firmware,
            &mut runtime_smokes,
            DiskSmokeOptions {
                run_limit: cli.smoke_run_limit,
                screenshot_path: screenshot_path.as_deref(),
                screenshot_format: cli.smoke_screenshot_format,
                cycle_limit: cli.cycles,
            },
        );
        rows.push(row);
    }

    Ok(DiskSmokeMatrixReport {
        disk_count: rows.len(),
        runtime_smokes,
        rows,
    })
}

fn xroar_reference_config(cli: &Cli) -> Result<Option<XroarReferenceConfig>, String> {
    match (&cli.xroar_bin, &cli.xroar_reference_dir) {
        (None, None) => Ok(None),
        (Some(bin), Some(output_dir)) => Ok(Some(XroarReferenceConfig {
            bin: bin.clone(),
            output_dir: output_dir.clone(),
            motoroff: cli.xroar_motoroff,
            settle_seconds: cli.xroar_settle_seconds,
            timeout_seconds: cli.xroar_timeout_seconds,
        })),
        (Some(_), None) => Err("--xroar-reference-dir is required with --xroar-bin".to_owned()),
        (None, Some(_)) => Err("--xroar-bin is required with --xroar-reference-dir".to_owned()),
    }
}

fn collect_snapshot_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_snapshot_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_snapshot_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn collect_tape_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_tape_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_tape_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn collect_bin_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_bin_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_bin_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn collect_disk_candidates(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_disk_candidate_path(path) {
            out.push(path.to_owned());
        }
        return Ok(());
    }

    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read entry under {}: {err}", path.display()))?;
        collect_disk_candidates(&entry.path(), out)?;
    }
    Ok(())
}

fn is_snapshot_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["pak", "zip"]))
}

fn is_tape_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["cas", "zip"]))
}

fn is_bin_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["bin", "zip"]))
}

fn is_disk_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches_ignore_ascii_case(ext, &["vdk", "zip"]))
}

fn scan_disk_candidate(
    path: &Path,
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
    runtime_smokes: &mut usize,
    smoke: DiskSmokeOptions<'_>,
) -> DiskSmokeRow {
    let loaded = match read_media_asset(path, MediaKind::Disk) {
        Ok(loaded) => loaded,
        Err(err) => {
            return DiskSmokeRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                tracks: None,
                sides: None,
                sectors_per_track: None,
                sector_size: None,
                directory_entries: None,
                directory: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let disk = match parse_vdk(&loaded.bytes) {
        Ok(disk) => disk,
        Err(err) => {
            return DiskSmokeRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                tracks: None,
                sides: None,
                sectors_per_track: None,
                sector_size: None,
                directory_entries: None,
                directory: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };
    let directory = dragon_dos_directory_entries(&disk);
    let launch_command = cli
        .disk_smoke_launch
        .then(|| choose_dragon_dos_launch_command(path, &directory));

    let runtime = if *runtime_smokes < smoke.run_limit {
        *runtime_smokes += 1;
        Some(match launch_command {
            Some(Some(command)) => {
                run_disk_command_smoke(cli, firmware, &loaded.bytes, &command, true, smoke)
            }
            Some(None) => failed_disk_runtime_smoke(
                "no BASIC or BIN DragonDOS launch candidate was found".to_owned(),
                "AUTO".to_owned(),
                DiskSmokeClassification::NoLaunchCandidate,
            ),
            None => run_disk_command_smoke(cli, firmware, &loaded.bytes, "DIR", false, smoke),
        })
    } else {
        None
    };

    DiskSmokeRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: "ok".to_owned(),
        tracks: Some(disk.tracks),
        sides: Some(disk.sides),
        sectors_per_track: Some(disk.sectors_per_track),
        sector_size: Some(disk.sector_size),
        directory_entries: Some(directory.len()),
        directory: Some(directory),
        runtime,
        error: None,
    }
}

fn run_disk_command_smoke(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
    disk_bytes: &[u8],
    command: &str,
    launch: bool,
    smoke: DiskSmokeOptions<'_>,
) -> DiskRuntimeSmoke {
    match run_disk_command_smoke_inner(cli, firmware, disk_bytes, command, launch, smoke) {
        Ok(smoke) => smoke,
        Err(error) => {
            failed_disk_runtime_smoke(error, command.to_owned(), DiskSmokeClassification::Error)
        }
    }
}

fn run_disk_command_smoke_inner(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
    disk_bytes: &[u8],
    command: &str,
    launch: bool,
    smoke: DiskSmokeOptions<'_>,
) -> Result<DiskRuntimeSmoke, String> {
    let cart_path = cli
        .cart
        .as_deref()
        .ok_or_else(|| "--disk-smoke-root requires --cart PATH for the DragonDOS ROM".to_owned())?;
    let cart_bytes = read_media_asset(cart_path, MediaKind::Cartridge)
        .map_err(|err| {
            format!(
                "failed to load DragonDOS cartridge {}: {err}",
                cart_path.display()
            )
        })?
        .bytes;

    let mut session = runtime_session(firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &cart_bytes,
    ));
    media.push(MediaImage::new("drive-1", MediaKind::Disk, disk_bytes));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load DragonDOS runtime media: {err}"))?;
    session
        .wait_for_boot(TYPED_COMMAND_BOOT_FRAME_BUDGET)
        .map_err(|err| format!("DragonDOS runtime did not report boot after media load: {err}"))?;
    session
        .run_frames(TYPED_COMMAND_POST_BOOT_SETTLE_FRAMES)
        .map_err(|err| format!("DragonDOS runtime did not idle after boot: {err}"))?;

    let mut trace_collector = RecentTraceCollector::new(64, 512);
    let mut result = None;
    let commands: Vec<_> = command
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if commands.is_empty() {
        return Err("DragonDOS smoke command is empty".to_owned());
    }
    for command in commands {
        type_basic_command_with_trace(&mut session, command, &mut trace_collector)?;
        result = Some(
            session
                .run_frames_with_trace_sink(
                    frames_for_cycles(smoke.cycle_limit),
                    &mut trace_collector,
                )
                .map_err(|err| {
                    format!("runtime failed after typing DragonDOS command {command:?}: {err}")
                })?,
        );
    }
    let result = result.ok_or_else(|| "DragonDOS smoke command did not run".to_owned())?;
    let state = runtime_smoke_state(&session)?;
    let screenshot = write_runtime_screenshot(
        smoke.screenshot_path,
        smoke.screenshot_format,
        &session,
        &state.diagnostic_png,
    )?;
    let disk_trace_count = trace_collector.disk_entries.len();
    let classification = classify_disk_smoke(
        launch,
        &state.screen_text,
        disk_trace_count,
        state.distinct_colors,
        state.non_background_pixels,
    );

    Ok(DiskRuntimeSmoke {
        classification,
        command: command.to_owned(),
        stop_reason: format_runtime_stop_reason(result.stop_reason),
        cycles: state.cycles,
        instructions: state.instructions,
        pc: state.pc,
        text_screen_base: state.text_screen_base,
        distinct_colors: state.distinct_colors,
        non_background_pixels: state.non_background_pixels,
        screen_text: state.screen_text,
        disk_trace_count,
        screenshot,
        trace_signature: runtime_trace_signature(
            state.cycles,
            state.pc,
            &state.framebuffer,
            &state.screen_text_for_hash,
        ),
        error: None,
    })
}

fn failed_disk_runtime_smoke(
    error: String,
    command: String,
    classification: DiskSmokeClassification,
) -> DiskRuntimeSmoke {
    DiskRuntimeSmoke {
        classification,
        command,
        stop_reason: "error".to_owned(),
        cycles: 0,
        instructions: 0,
        pc: 0,
        text_screen_base: 0,
        distinct_colors: 0,
        non_background_pixels: 0,
        screen_text: Vec::new(),
        disk_trace_count: 0,
        screenshot: None,
        trace_signature: runtime_trace_signature(0, 0, &[], &[]),
        error: Some(error),
    }
}

fn classify_disk_smoke(
    launch: bool,
    screen_text: &[String],
    disk_trace_count: usize,
    distinct_colors: usize,
    non_background_pixels: usize,
) -> DiskSmokeClassification {
    if launch && screen_text.iter().any(|line| line.contains("ERROR")) {
        return DiskSmokeClassification::LaunchError;
    }
    if disk_trace_count == 0 {
        return DiskSmokeClassification::NoDiskAccess;
    }
    if launch {
        let visible = distinct_colors > 1 && non_background_pixels > 0;
        return if visible {
            DiskSmokeClassification::LaunchVisible
        } else {
            DiskSmokeClassification::LaunchBlank
        };
    }
    if screen_text.iter().any(|line| line.contains("FREE BYTES")) {
        return DiskSmokeClassification::DirectoryVisible;
    }
    DiskSmokeClassification::DirectoryError
}

fn dragon_dos_directory_entries(disk: &DragonDiskImage) -> Vec<DragonDosDirectoryEntrySummary> {
    let sector_size = usize::from(disk.sector_size);
    let mut entries = Vec::new();
    for sector in disk.data().chunks_exact(sector_size) {
        for base in [0usize, 1, 11] {
            collect_directory_entries_in_sector_layout(sector, base, &mut entries);
        }
    }
    entries
}

fn collect_directory_entries_in_sector_layout(
    sector: &[u8],
    base: usize,
    entries: &mut Vec<DragonDosDirectoryEntrySummary>,
) {
    for entry in 0..10 {
        let offset = base + entry * 25;
        let Some(raw_entry) = sector.get(offset..offset + 25) else {
            continue;
        };
        let Some(entry) = dragon_dos_directory_entry(raw_entry) else {
            continue;
        };
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
}

fn dragon_dos_directory_entry(entry: &[u8]) -> Option<DragonDosDirectoryEntrySummary> {
    let name = entry.get(0..8)?;
    let extension = entry.get(8..11)?;
    if entry.get(11).copied()? > 0x01 || !is_plausible_dragon_dos_directory_entry(name, extension) {
        return None;
    }
    Some(DragonDosDirectoryEntrySummary {
        name: dragon_dos_field_to_string(name),
        extension: dragon_dos_field_to_string(extension),
    })
}

fn choose_dragon_dos_launch_command(
    media_path: &Path,
    entries: &[DragonDosDirectoryEntrySummary],
) -> Option<String> {
    choose_dragon_dos_media_matched_binary(media_path, entries)
        .map(|entry| format!("LOAD\"{}.BIN\":EXEC", entry.name))
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.extension == "BAS")
                .map(|entry| format!("RUN\"{}\"", entry.name))
        })
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.extension == "BIN")
                .map(|entry| format!("LOAD\"{}.BIN\":EXEC", entry.name))
        })
}

fn choose_dragon_dos_media_matched_binary<'a>(
    media_path: &Path,
    entries: &'a [DragonDosDirectoryEntrySummary],
) -> Option<&'a DragonDosDirectoryEntrySummary> {
    let media_tokens = dragon_dos_media_title_tokens(media_path);
    if media_tokens.is_empty() {
        return None;
    }
    entries.iter().find(|entry| {
        entry.extension == "BIN" && dragon_dos_name_matches_title(&entry.name, &media_tokens)
    })
}

fn dragon_dos_media_title_tokens(media_path: &Path) -> Vec<String> {
    let title = media_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .split_once(" (")
        .map_or_else(
            || {
                media_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
            },
            |(title, _)| title,
        );
    title
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 4)
        .map(|token| token.to_ascii_uppercase())
        .collect()
}

fn dragon_dos_name_matches_title(name: &str, media_tokens: &[String]) -> bool {
    let name = name.to_ascii_uppercase();
    media_tokens.iter().any(|token| {
        name == *token
            || name.contains(token)
            || token.contains(&name)
            || token.get(..4).is_some_and(|prefix| name.contains(prefix))
    })
}

fn dragon_dos_field_to_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|byte| !matches!(*byte, 0x00 | b' '))
        .map_or(0, |index| index + 1);
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

fn is_plausible_dragon_dos_directory_entry(name: &[u8], extension: &[u8]) -> bool {
    let Some((&first_name, rest_name)) = name.split_first() else {
        return false;
    };
    if matches!(first_name, 0x00 | 0xff) || !is_dragon_dos_name_byte(first_name) {
        return false;
    }
    rest_name.iter().all(|&byte| {
        matches!(byte, 0x00 | b' ')
            || (is_dragon_dos_name_byte(byte) && !matches!(byte, b'.' | b',' | b'/'))
    }) && extension
        .iter()
        .all(|&byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn is_dragon_dos_name_byte(byte: u8) -> bool {
    byte.is_ascii_uppercase()
        || byte.is_ascii_digit()
        || matches!(
            byte,
            b' ' | b'!' | b'"' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'(' | b')' | b'+' | b'-'
        )
}

fn scan_bin_candidate(
    path: &Path,
    firmware: &LoadedDragonFirmware,
    runtime_smokes: &mut usize,
    smoke: BinSmokeOptions<'_>,
) -> BinSmokeRow {
    let loaded = match read_media_asset(path, MediaKind::Program) {
        Ok(loaded) => loaded,
        Err(err) => {
            return BinSmokeRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                file_type: None,
                load_address: None,
                exec_address: None,
                len: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let program = match parse_dragon_bin(&loaded.bytes) {
        Ok(program) => program,
        Err(err) => {
            return BinSmokeRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                file_type: None,
                load_address: None,
                exec_address: None,
                len: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let runtime = if *runtime_smokes < smoke.run_limit {
        *runtime_smokes += 1;
        Some(run_bin_smoke(firmware, &loaded.bytes, &program, smoke))
    } else {
        None
    };

    BinSmokeRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: "ok".to_owned(),
        file_type: Some(program.file_type),
        load_address: Some(program.load_address),
        exec_address: Some(program.exec_address),
        len: Some(program.payload.len()),
        runtime,
        error: None,
    }
}

fn scan_snapshot_candidate(
    path: &Path,
    firmware: &LoadedDragonFirmware,
    runtime_smokes: &mut usize,
    smoke: SnapshotSmokeOptions<'_>,
) -> SnapshotSmokeRow {
    let loaded = match read_media_asset(path, MediaKind::Snapshot) {
        Ok(loaded) => loaded,
        Err(err) => {
            return SnapshotSmokeRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let snapshot = match parse_pcdragon_snapshot(&loaded.bytes) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return SnapshotSmokeRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let runtime = if *runtime_smokes < smoke.run_limit {
        *runtime_smokes += 1;
        Some(run_snapshot_smoke(
            firmware,
            &loaded.bytes,
            &snapshot,
            smoke,
        ))
    } else {
        None
    };

    SnapshotSmokeRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: "ok".to_owned(),
        runtime,
        error: None,
    }
}

fn run_snapshot_smoke(
    firmware: &LoadedDragonFirmware,
    snapshot_bytes: &[u8],
    snapshot: &PcDragonSnapshot,
    smoke: SnapshotSmokeOptions<'_>,
) -> SnapshotRuntimeSmoke {
    if firmware.model == Model::Dragon64Pal {
        return run_snapshot_smoke_runtime(firmware, snapshot_bytes, snapshot, smoke);
    }

    let report = run_harness_with_keyboard(
        &firmware.rom,
        DragonKeyboard::new(),
        HarnessRunOptions {
            cartridge: None,
            disk: None,
            program: None,
            snapshot: Some(snapshot),
            cycle_limit: smoke.cycle_limit,
            trace_limit: smoke.trace_limit,
            fetch_watch: Vec::new(),
            write_watch: Vec::new(),
            dump_text: true,
            dump_ram: false,
            export_disk: false,
            dump_text_framebuffer: false,
            capture_framebuffer: true,
            capture_framebuffer_phase: smoke.screenshot_phase,
            capture_framebuffer_source: ScreenshotSource::Beam,
        },
    );
    let framebuffer = report.framebuffer.as_deref().unwrap_or(&[]);
    let (distinct_colors, non_background_pixels) = framebuffer_stats(framebuffer);
    let screenshot = match (smoke.screenshot_path, report.framebuffer.as_deref()) {
        (Some(path), Some(framebuffer)) => {
            if let Err(err) = write_screenshot_png(
                path,
                framebuffer,
                smoke.screenshot_format,
                report.framebuffer_cycles.unwrap_or(report.cycles),
            ) {
                return failed_snapshot_smoke(
                    snapshot,
                    report,
                    distinct_colors,
                    non_background_pixels,
                    err,
                );
            }
            Some(path.display().to_string())
        }
        _ => None,
    };
    let classification =
        classify_snapshot_smoke(report.stop_reason, distinct_colors, non_background_pixels);
    let comparison_screenshot =
        if matches!(smoke.screenshot_format, SmokeScreenshotFormat::XroarZoomed) {
            screenshot.as_deref().map(Path::new)
        } else {
            None
        };
    let screenshot_cycles = report.framebuffer_cycles.unwrap_or(report.cycles);
    let screenshot_master_ticks = report
        .framebuffer_master_ticks
        .unwrap_or(report.master_ticks);
    let use_master_timed_reference =
        matches!(smoke.screenshot_phase, SmokeScreenshotPhase::CompletedFrame)
            && report.framebuffer_master_ticks.is_some();
    let xroar_reference_settle_seconds =
        xroar_snapshot_settle_seconds(screenshot_cycles, screenshot_master_ticks);
    let xroar_reference_trap =
        if smoke.xroar.is_some() && smoke.xroar_stem.is_some() && !use_master_timed_reference {
            xroar_snapshot_reference_trap(
                &firmware.rom,
                snapshot,
                screenshot_cycles,
                smoke.trace_limit,
            )
        } else {
            None
        };
    let (xroar_reference_screenshot, xroar_reference_error, xroar_reference_comparison) =
        match (smoke.xroar, smoke.xroar_stem) {
            (Some(config), Some(stem)) => match capture_xroar_snapshot_reference(
                config,
                &firmware.rom,
                snapshot,
                comparison_screenshot,
                stem,
                XroarSnapshotReferenceTiming {
                    trap: xroar_reference_trap,
                    settle_seconds: xroar_reference_settle_seconds,
                    start_immediate: use_master_timed_reference,
                },
            ) {
                Ok(reference) => (
                    Some(reference.path.display().to_string()),
                    None,
                    reference.comparison,
                ),
                Err(err) => (None, Some(err), None),
            },
            _ => (None, None, None),
        };

    SnapshotRuntimeSmoke {
        classification,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_master_ticks: report.framebuffer_master_ticks,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|_| report.video_phase.frame_master_tick),
        video_phase: video_phase_summary(report.video_phase),
        trace_signature: snapshot_trace_signature(&report, framebuffer),
        xroar_reference_screenshot,
        xroar_reference_settle_seconds: smoke.xroar.map(|_| xroar_reference_settle_seconds),
        xroar_reference_trap_pc: xroar_reference_trap.map(|trap| trap.pc),
        xroar_reference_trap_count: xroar_reference_trap.map(|trap| trap.count),
        xroar_reference_error,
        xroar_reference_comparison,
        xroar_reference_comparison_error: None,
        vdg_trace: vdg_trace_summary(&report),
        error: None,
    }
}

fn failed_snapshot_smoke(
    snapshot: &PcDragonSnapshot,
    report: HarnessReport,
    distinct_colors: usize,
    non_background_pixels: usize,
    error: String,
) -> SnapshotRuntimeSmoke {
    SnapshotRuntimeSmoke {
        classification: SnapshotSmokeClassification::Error,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot: None,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_master_ticks: report.framebuffer_master_ticks,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|_| report.video_phase.frame_master_tick),
        video_phase: video_phase_summary(report.video_phase),
        trace_signature: snapshot_trace_signature(&report, &[]),
        xroar_reference_screenshot: None,
        xroar_reference_settle_seconds: None,
        xroar_reference_trap_pc: None,
        xroar_reference_trap_count: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
        vdg_trace: vdg_trace_summary(&report),
        error: Some(error),
    }
}

fn run_snapshot_smoke_runtime(
    firmware: &LoadedDragonFirmware,
    snapshot_bytes: &[u8],
    snapshot: &PcDragonSnapshot,
    smoke: SnapshotSmokeOptions<'_>,
) -> SnapshotRuntimeSmoke {
    match run_snapshot_smoke_runtime_inner(firmware, snapshot_bytes, snapshot, smoke) {
        Ok(smoke) => smoke,
        Err(error) => failed_snapshot_runtime_smoke(snapshot, error),
    }
}

fn run_snapshot_smoke_runtime_inner(
    firmware: &LoadedDragonFirmware,
    snapshot_bytes: &[u8],
    snapshot: &PcDragonSnapshot,
    smoke: SnapshotSmokeOptions<'_>,
) -> Result<SnapshotRuntimeSmoke, String> {
    let mut session = runtime_session(firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "snapshot-1",
        MediaKind::Snapshot,
        snapshot_bytes,
    ));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load snapshot into runtime: {err}"))?;
    let target = session.time().saturating_add(smoke.cycle_limit);
    let result = session
        .run_until(target)
        .map_err(|err| format!("snapshot runtime failed: {err}"))?;
    runtime_snapshot_smoke_from_session(&session, result.stop_reason, snapshot, smoke)
}

fn failed_snapshot_runtime_smoke(
    snapshot: &PcDragonSnapshot,
    error: String,
) -> SnapshotRuntimeSmoke {
    SnapshotRuntimeSmoke {
        classification: SnapshotSmokeClassification::Error,
        stop_reason: "error".to_owned(),
        cycles: 0,
        instructions: 0,
        pc: snapshot.registers.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: 0,
        distinct_colors: 0,
        non_background_pixels: 0,
        screen_text: Vec::new(),
        screenshot: None,
        screenshot_cycles: None,
        screenshot_master_ticks: None,
        screenshot_frame_phase_cycles: None,
        video_phase: runtime_video_phase_summary(),
        trace_signature: runtime_trace_signature(0, 0, &[], &[]),
        xroar_reference_screenshot: None,
        xroar_reference_settle_seconds: None,
        xroar_reference_trap_pc: None,
        xroar_reference_trap_count: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
        vdg_trace: None,
        error: Some(error),
    }
}

fn run_bin_smoke(
    firmware: &LoadedDragonFirmware,
    program_bytes: &[u8],
    program: &DragonBinImage,
    smoke: BinSmokeOptions<'_>,
) -> BinRuntimeSmoke {
    if firmware.model == Model::Dragon64Pal {
        return run_bin_smoke_runtime(firmware, program_bytes, smoke);
    }

    let report = run_harness_with_keyboard(
        &firmware.rom,
        DragonKeyboard::new(),
        HarnessRunOptions {
            cartridge: None,
            disk: None,
            program: Some(program),
            snapshot: None,
            cycle_limit: smoke.cycle_limit,
            trace_limit: smoke.trace_limit,
            fetch_watch: Vec::new(),
            write_watch: Vec::new(),
            dump_text: true,
            dump_ram: false,
            export_disk: false,
            dump_text_framebuffer: false,
            capture_framebuffer: true,
            capture_framebuffer_phase: smoke.screenshot_phase,
            capture_framebuffer_source: ScreenshotSource::Beam,
        },
    );
    let framebuffer = report.framebuffer.as_deref().unwrap_or(&[]);
    let (distinct_colors, non_background_pixels) = framebuffer_stats(framebuffer);
    let screenshot = match (smoke.screenshot_path, report.framebuffer.as_deref()) {
        (Some(path), Some(framebuffer)) => {
            if let Err(err) = write_screenshot_png(
                path,
                framebuffer,
                smoke.screenshot_format,
                report.framebuffer_cycles.unwrap_or(report.cycles),
            ) {
                return failed_bin_smoke(report, distinct_colors, non_background_pixels, err);
            }
            Some(path.display().to_string())
        }
        _ => None,
    };
    let classification =
        classify_snapshot_smoke(report.stop_reason, distinct_colors, non_background_pixels);

    BinRuntimeSmoke {
        classification,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_master_ticks: report.framebuffer_master_ticks,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|_| report.video_phase.frame_master_tick),
        video_phase: video_phase_summary(report.video_phase),
        trace_signature: snapshot_trace_signature(&report, framebuffer),
        vdg_trace: vdg_trace_summary(&report),
        error: None,
    }
}

fn run_bin_smoke_runtime(
    firmware: &LoadedDragonFirmware,
    program_bytes: &[u8],
    smoke: BinSmokeOptions<'_>,
) -> BinRuntimeSmoke {
    match run_bin_smoke_runtime_inner(firmware, program_bytes, smoke) {
        Ok(smoke) => smoke,
        Err(error) => failed_bin_runtime_smoke(error),
    }
}

fn run_bin_smoke_runtime_inner(
    firmware: &LoadedDragonFirmware,
    program_bytes: &[u8],
    smoke: BinSmokeOptions<'_>,
) -> Result<BinRuntimeSmoke, String> {
    let mut session = runtime_session(firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "program-1",
        MediaKind::Program,
        program_bytes,
    ));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load program into runtime: {err}"))?;
    let target = session.time().saturating_add(smoke.cycle_limit);
    let result = session
        .run_until(target)
        .map_err(|err| format!("program runtime failed: {err}"))?;
    runtime_bin_smoke_from_session(&session, result.stop_reason, smoke)
}

fn failed_bin_runtime_smoke(error: String) -> BinRuntimeSmoke {
    BinRuntimeSmoke {
        classification: SnapshotSmokeClassification::Error,
        stop_reason: "error".to_owned(),
        cycles: 0,
        instructions: 0,
        pc: 0,
        text_screen_base: 0,
        distinct_colors: 0,
        non_background_pixels: 0,
        screen_text: Vec::new(),
        screenshot: None,
        screenshot_cycles: None,
        screenshot_master_ticks: None,
        screenshot_frame_phase_cycles: None,
        video_phase: runtime_video_phase_summary(),
        trace_signature: runtime_trace_signature(0, 0, &[], &[]),
        vdg_trace: None,
        error: Some(error),
    }
}

fn runtime_snapshot_smoke_from_session(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    stop_reason: RuntimeStopReason,
    snapshot: &PcDragonSnapshot,
    smoke: SnapshotSmokeOptions<'_>,
) -> Result<SnapshotRuntimeSmoke, String> {
    let state = runtime_smoke_state(session)?;
    let screenshot = write_runtime_screenshot(
        smoke.screenshot_path,
        smoke.screenshot_format,
        session,
        &state.diagnostic_png,
    )?;
    let classification = classify_snapshot_smoke(
        runtime_stop_reason(stop_reason),
        state.distinct_colors,
        state.non_background_pixels,
    );

    Ok(SnapshotRuntimeSmoke {
        classification,
        stop_reason: format_runtime_stop_reason(stop_reason),
        cycles: state.cycles,
        instructions: state.instructions,
        pc: state.pc,
        load_address: snapshot.load_address,
        ram_len: snapshot.ram.len(),
        text_screen_base: state.text_screen_base,
        distinct_colors: state.distinct_colors,
        non_background_pixels: state.non_background_pixels,
        screen_text: state.screen_text,
        screenshot,
        screenshot_cycles: Some(state.cycles),
        screenshot_master_ticks: None,
        screenshot_frame_phase_cycles: None,
        video_phase: runtime_video_phase_summary(),
        trace_signature: runtime_trace_signature(
            state.cycles,
            state.pc,
            &state.framebuffer,
            &state.screen_text_for_hash,
        ),
        xroar_reference_screenshot: None,
        xroar_reference_settle_seconds: None,
        xroar_reference_trap_pc: None,
        xroar_reference_trap_count: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
        vdg_trace: None,
        error: None,
    })
}

fn runtime_bin_smoke_from_session(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    stop_reason: RuntimeStopReason,
    smoke: BinSmokeOptions<'_>,
) -> Result<BinRuntimeSmoke, String> {
    let state = runtime_smoke_state(session)?;
    let screenshot = write_runtime_screenshot(
        smoke.screenshot_path,
        smoke.screenshot_format,
        session,
        &state.diagnostic_png,
    )?;
    let classification = classify_snapshot_smoke(
        runtime_stop_reason(stop_reason),
        state.distinct_colors,
        state.non_background_pixels,
    );

    Ok(BinRuntimeSmoke {
        classification,
        stop_reason: format_runtime_stop_reason(stop_reason),
        cycles: state.cycles,
        instructions: state.instructions,
        pc: state.pc,
        text_screen_base: state.text_screen_base,
        distinct_colors: state.distinct_colors,
        non_background_pixels: state.non_background_pixels,
        screen_text: state.screen_text,
        screenshot,
        screenshot_cycles: Some(state.cycles),
        screenshot_master_ticks: None,
        screenshot_frame_phase_cycles: None,
        video_phase: runtime_video_phase_summary(),
        trace_signature: runtime_trace_signature(
            state.cycles,
            state.pc,
            &state.framebuffer,
            &state.screen_text_for_hash,
        ),
        vdg_trace: None,
        error: None,
    })
}

struct RuntimeSmokeState {
    cycles: u64,
    instructions: u64,
    pc: u16,
    text_screen_base: u16,
    distinct_colors: usize,
    non_background_pixels: usize,
    screen_text: Vec<String>,
    screen_text_for_hash: Vec<String>,
    framebuffer: Vec<u32>,
    diagnostic_png: Vec<u8>,
}

struct TypedCommandReport {
    command: String,
    boot_frames: u32,
    boot_reason: String,
    frames_after_command: u32,
    cycles: u64,
    instructions: u64,
    pc: u16,
    text_screen_base: u16,
    screen_text: Vec<String>,
    screenshot_png: Vec<u8>,
    disk_vdk: Option<Vec<u8>>,
    device_traces: Vec<String>,
    disk_traces: Vec<String>,
    interrupt_traces: Vec<String>,
}

#[derive(Default)]
struct RecentTraceCollector {
    entries: Vec<String>,
    disk_entries: Vec<String>,
    interrupt_entries: Vec<String>,
    limit: usize,
    disk_limit: usize,
}

impl RecentTraceCollector {
    fn new(limit: usize, disk_limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            disk_entries: Vec::new(),
            interrupt_entries: Vec::new(),
            limit,
            disk_limit,
        }
    }
}

impl TraceSink for RecentTraceCollector {
    fn push_trace(&mut self, event: TraceEvent<'_>) -> Result<(), MachineError> {
        match event.kind.as_ref() {
            "dragon.device_access" => {
                let payload = String::from_utf8_lossy(event.payload);
                self.entries
                    .push(format!("{} {}", event.timestamp.0, payload));
                if self.entries.len() > self.limit {
                    self.entries.remove(0);
                }
                if payload.contains(r#""device":"DiskController""#) {
                    self.disk_entries
                        .push(format!("{} {}", event.timestamp.0, payload));
                    if self.disk_entries.len() > self.disk_limit {
                        self.disk_entries.remove(0);
                    }
                }
            }
            "dragon.interrupt_accept" | "dragon.interrupt_line" => {
                let payload = String::from_utf8_lossy(event.payload);
                self.interrupt_entries
                    .push(format!("{} {}", event.timestamp.0, payload));
                if self.interrupt_entries.len() > self.limit {
                    self.interrupt_entries.remove(0);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn run_typed_command(
    cli: &Cli,
    firmware: &LoadedDragonFirmware,
    command: &str,
) -> Result<TypedCommandReport, String> {
    let mut session = runtime_session(firmware)?;
    let mut cart_bytes = None;
    let mut disk_bytes = None;
    let mut program_bytes = None;
    let mut snapshot_bytes = None;

    if let Some(path) = &cli.cart {
        cart_bytes = Some(
            read_media_asset(path, MediaKind::Cartridge)
                .map_err(|err| {
                    format!("failed to load Dragon cartridge {}: {err}", path.display())
                })?
                .bytes,
        );
    }
    if let Some(path) = &cli.disk {
        disk_bytes = Some(
            read_media_asset(path, MediaKind::Disk)
                .map_err(|err| format!("failed to load Dragon disk {}: {err}", path.display()))?
                .bytes,
        );
    }
    if let Some(path) = &cli.bin {
        program_bytes = Some(
            read_media_asset(path, MediaKind::Program)
                .map_err(|err| format!("failed to load Dragon program {}: {err}", path.display()))?
                .bytes,
        );
    }
    if let Some(path) = &cli.snapshot {
        snapshot_bytes = Some(
            read_media_asset(path, MediaKind::Snapshot)
                .map_err(|err| format!("failed to load Dragon snapshot {}: {err}", path.display()))?
                .bytes,
        );
    }

    let mut media = MediaSet::new();
    if let Some(bytes) = &cart_bytes {
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    }
    if let Some(bytes) = &disk_bytes {
        media.push(MediaImage::new("drive-1", MediaKind::Disk, bytes));
    }
    if let Some(bytes) = &program_bytes {
        media.push(MediaImage::new("program-1", MediaKind::Program, bytes));
    }
    if let Some(bytes) = &snapshot_bytes {
        media.push(MediaImage::new("snapshot-1", MediaKind::Snapshot, bytes));
    }
    if !media.is_empty() {
        session
            .load_media(&media)
            .map_err(|err| format!("failed to load runtime media: {err}"))?;
    }

    let boot = session
        .wait_for_boot(TYPED_COMMAND_BOOT_FRAME_BUDGET)
        .map_err(|err| format!("Dragon runtime did not report boot after media load: {err}"))?;
    session
        .run_frames(TYPED_COMMAND_POST_BOOT_SETTLE_FRAMES)
        .map_err(|err| format!("Dragon runtime did not idle after boot: {err}"))?;
    let frames_after_command = frames_for_cycles(cli.cycles);
    let mut trace_collector = RecentTraceCollector::new(64, 512);
    type_basic_command_with_trace(&mut session, command, &mut trace_collector)?;
    session
        .run_frames_with_trace_sink(frames_after_command, &mut trace_collector)
        .map_err(|err| format!("runtime failed after typing command {command:?}: {err}"))?;
    let screenshot_png = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture typed-command screenshot: {err}"))?;
    let disk_vdk = cli
        .disk_output
        .is_some()
        .then(|| session.machine().export_drive_vdk(0))
        .flatten();

    Ok(TypedCommandReport {
        command: command.to_owned(),
        boot_frames: boot.frames,
        boot_reason: boot.reason,
        frames_after_command,
        cycles: query_u64(&session, "dragon.cpu.cycles")?,
        instructions: query_u64(&session, "dragon.cpu.instructions")?,
        pc: query_u16(&session, "dragon.cpu.pc")?,
        text_screen_base: query_u16(&session, "dragon.text.base")?,
        screen_text: screen_text_lines(&session)?,
        screenshot_png,
        disk_vdk,
        device_traces: trace_collector.entries,
        disk_traces: trace_collector.disk_entries,
        interrupt_traces: trace_collector.interrupt_entries,
    })
}

fn print_typed_command_report(report: &TypedCommandReport) {
    println!("dragon typed-command summary");
    println!("command: {}", report.command);
    println!(
        "boot: reason={} frames={}",
        report.boot_reason, report.boot_frames
    );
    println!("frames after command: {}", report.frames_after_command);
    println!("cycles: {}", report.cycles);
    println!("instructions: {}", report.instructions);
    println!("pc: ${:04X}", report.pc);
    println!("text screen base: ${:04X}", report.text_screen_base);
    println!("text screen:");
    for line in &report.screen_text {
        println!("  |{line}|");
    }
    if !report.device_traces.is_empty() {
        println!("device traces:");
        for trace in &report.device_traces {
            println!("  {trace}");
        }
    }
    if !report.disk_traces.is_empty() {
        println!("disk traces:");
        for trace in &report.disk_traces {
            println!("  {trace}");
        }
    }
    if !report.interrupt_traces.is_empty() {
        println!("interrupt traces:");
        for trace in &report.interrupt_traces {
            println!("  {trace}");
        }
    }
}

fn frames_for_cycles(cycles: u64) -> u32 {
    let frames = cycles.div_ceil(DRAGON_FRAME_CYCLES);
    u32::try_from(frames.max(1)).unwrap_or(u32::MAX)
}

fn runtime_smoke_state(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<RuntimeSmokeState, String> {
    let framebuffer = runtime_framebuffer_argb(session)?;
    let (distinct_colors, non_background_pixels) = framebuffer_stats(&framebuffer);
    let screen_text = screen_text_lines(session)?;
    let diagnostic_png = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture runtime smoke screenshot: {err}"))?;

    Ok(RuntimeSmokeState {
        cycles: query_u64(session, "dragon.cpu.cycles")?,
        instructions: query_u64(session, "dragon.cpu.instructions")?,
        pc: query_u16(session, "dragon.cpu.pc")?,
        text_screen_base: query_u16(session, "dragon.text.base")?,
        distinct_colors,
        non_background_pixels,
        screen_text_for_hash: screen_text.clone(),
        screen_text,
        framebuffer,
        diagnostic_png,
    })
}

fn runtime_framebuffer_argb(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<Vec<u32>, String> {
    let frame = session
        .latest_frame()
        .ok_or_else(|| "runtime smoke did not emit a frame".to_owned())?;
    let rgba = frame
        .rgba_pixels()
        .map_err(|err| format!("failed to read runtime frame pixels: {err}"))?;
    let mut pixels = Vec::with_capacity(rgba.len() / 4);
    for chunk in rgba.chunks_exact(4) {
        let [r, g, b, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        pixels
            .push((u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
    }
    Ok(pixels)
}

fn write_runtime_screenshot(
    path: Option<&Path>,
    format: SmokeScreenshotFormat,
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    diagnostic_png: &[u8],
) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let png = match format {
        SmokeScreenshotFormat::Diagnostic => diagnostic_png.to_vec(),
        SmokeScreenshotFormat::XroarZoomed => {
            xroar_zoomed_png_bytes(session.latest_frame().ok_or_else(|| {
                "cannot write xroar-zoomed runtime screenshot before a frame has been captured"
                    .to_owned()
            })?)?
        }
    };
    fs::write(path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn runtime_video_phase_summary() -> VideoPhaseSummary {
    VideoPhaseSummary {
        frame_master_tick: 0,
        physical_line: 0,
        line_master_tick: 0,
        visible_line: None,
        active_y: None,
        active_x: None,
    }
}

fn runtime_stop_reason(reason: RuntimeStopReason) -> StopReason {
    match reason {
        RuntimeStopReason::Halted => StopReason::CpuHalted,
        RuntimeStopReason::ReachedTarget
        | RuntimeStopReason::WaitingForInput
        | RuntimeStopReason::Breakpoint => StopReason::CycleLimit,
        _ => StopReason::CycleLimit,
    }
}

fn format_runtime_stop_reason(reason: RuntimeStopReason) -> String {
    match reason {
        RuntimeStopReason::ReachedTarget => "reached-target",
        RuntimeStopReason::WaitingForInput => "waiting-for-input",
        RuntimeStopReason::Breakpoint => "breakpoint",
        RuntimeStopReason::Halted => "halted",
        _ => "unknown",
    }
    .to_owned()
}

fn runtime_trace_signature(
    cycles: u64,
    pc: u16,
    framebuffer: &[u32],
    screen_text: &[String],
) -> SnapshotTraceSignature {
    let mut hasher = StableTraceHasher::new();
    hasher.write_u64(cycles);
    hasher.write_u16(pc);
    hasher.write_usize(framebuffer.len());
    for &pixel in framebuffer {
        hasher.write_u32(pixel);
    }
    for line in screen_text {
        hasher.write_str(line);
    }
    SnapshotTraceSignature {
        hash: hasher.finish_hex(),
        trace_entries: 0,
        vdg_samples: 0,
        vdg_mode_writes: 0,
        framebuffer_words: framebuffer.len(),
    }
}

fn failed_bin_smoke(
    report: HarnessReport,
    distinct_colors: usize,
    non_background_pixels: usize,
    error: String,
) -> BinRuntimeSmoke {
    BinRuntimeSmoke {
        classification: SnapshotSmokeClassification::Error,
        stop_reason: format_stop_reason(report.stop_reason),
        cycles: report.cycles,
        instructions: report.instructions,
        pc: report.pc,
        text_screen_base: report.text_screen_base,
        distinct_colors,
        non_background_pixels,
        screen_text: report
            .text_screen
            .as_deref()
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default(),
        screenshot: None,
        screenshot_cycles: report.framebuffer_cycles,
        screenshot_master_ticks: report.framebuffer_master_ticks,
        screenshot_frame_phase_cycles: report
            .framebuffer_cycles
            .map(|_| report.video_phase.frame_master_tick),
        video_phase: video_phase_summary(report.video_phase),
        trace_signature: snapshot_trace_signature(&report, &[]),
        vdg_trace: vdg_trace_summary(&report),
        error: Some(error),
    }
}

fn xroar_snapshot_reference_trap(
    rom: &[u8; ROM_SIZE],
    snapshot: &PcDragonSnapshot,
    target_cycles: u64,
    trace_limit: usize,
) -> Option<XroarSnapshotTrap> {
    let probe = run_harness_with_keyboard(
        rom,
        DragonKeyboard::new(),
        HarnessRunOptions {
            cartridge: None,
            disk: None,
            program: None,
            snapshot: Some(snapshot),
            cycle_limit: target_cycles,
            trace_limit: 0,
            fetch_watch: Vec::new(),
            write_watch: Vec::new(),
            dump_text: false,
            dump_ram: false,
            export_disk: false,
            dump_text_framebuffer: false,
            capture_framebuffer: false,
            capture_framebuffer_phase: SmokeScreenshotPhase::Immediate,
            capture_framebuffer_source: ScreenshotSource::Beam,
        },
    );
    let fetch = probe.last_fetch?;
    let report = run_harness_with_keyboard(
        rom,
        DragonKeyboard::new(),
        HarnessRunOptions {
            cartridge: None,
            disk: None,
            program: None,
            snapshot: Some(snapshot),
            cycle_limit: target_cycles,
            trace_limit,
            fetch_watch: vec![AddressRange::new(fetch.pc, fetch.pc)],
            write_watch: Vec::new(),
            dump_text: false,
            dump_ram: false,
            export_disk: false,
            dump_text_framebuffer: false,
            capture_framebuffer: false,
            capture_framebuffer_phase: SmokeScreenshotPhase::Immediate,
            capture_framebuffer_source: ScreenshotSource::Beam,
        },
    );
    let count = report
        .dropped_watched_fetches
        .saturating_add(report.watched_fetches.len());
    (count != 0).then_some(XroarSnapshotTrap {
        pc: fetch.pc,
        count,
    })
}

const fn video_phase_summary(phase: DragonVideoPhase) -> VideoPhaseSummary {
    VideoPhaseSummary {
        frame_master_tick: phase.frame_master_tick,
        physical_line: phase.physical_line,
        line_master_tick: phase.line_master_tick,
        visible_line: phase.visible_line,
        active_y: phase.active_y,
        active_x: phase.active_x,
    }
}

fn snapshot_trace_signature(report: &HarnessReport, framebuffer: &[u32]) -> SnapshotTraceSignature {
    let mut hasher = StableTraceHasher::new();
    hasher.write_u8(match report.stop_reason {
        StopReason::CycleLimit => 0,
        StopReason::CpuHalted => 1,
    });
    hasher.write_u64(report.cycles);
    hasher.write_u64(report.master_ticks);
    hasher.write_u64(report.instructions);
    hasher.write_u16(report.pc);
    hasher.write_u16(report.addr);
    hasher.write_bool(report.rw);
    hasher.write_u16(report.text_screen_base);
    write_video_phase_signature(&mut hasher, report.video_phase);
    hasher.write_usize(report.dropped_trace);
    for fetch in &report.trace {
        hasher.write_u64(fetch.cycle);
        hasher.write_u64(fetch.master_tick);
        hasher.write_u16(fetch.pc);
        hasher.write_u8(fetch.opcode);
    }
    hasher.write_usize(report.dropped_vdg_samples);
    for sample in &report.vdg_samples {
        hasher.write_u64(sample.cycle);
        hasher.write_u64(sample.frame_master_tick);
        hasher.write_u64(sample.fetch_frame_master_tick);
        hasher.write_usize(sample.line);
        hasher.write_usize(sample.active_y);
        hasher.write_usize(sample.byte_x);
        hasher.write_usize(sample.display_offset);
        hasher.write_u8(sample.raw);
        hasher.write_u16(sample.display_base);
        hasher.write_u8(sample.sam_video_mode);
        hasher.write_u8(sample.sam_display_offset);
        hasher.write_u8(sample.pia1_pb);
        hasher.write_bool(sample.graphics);
        hasher.write_bool(sample.css);
        hasher.write_bool(sample.int_ext);
        hasher.write_u8(sample.gm);
    }
    hasher.write_usize(report.dropped_vdg_mode_writes);
    for write in &report.vdg_mode_writes {
        hasher.write_u64(write.cycle);
        hasher.write_u64(write.frame_master_tick);
        hasher.write_option_usize(write.line);
        hasher.write_option_usize(write.active_y);
        hasher.write_option_usize(write.active_x);
        hasher.write_u16(write.addr);
        hasher.write_u8(write.value);
        hasher.write_bool(write.graphics);
        hasher.write_bool(write.css);
        hasher.write_bool(write.int_ext);
        hasher.write_u8(write.gm);
    }
    hasher.write_usize(framebuffer.len());
    for &pixel in framebuffer {
        hasher.write_u32(pixel);
    }
    if let Some(text) = &report.text_screen {
        hasher.write_str(text);
    }

    SnapshotTraceSignature {
        hash: hasher.finish_hex(),
        trace_entries: report.trace.len(),
        vdg_samples: report.vdg_samples.len(),
        vdg_mode_writes: report.vdg_mode_writes.len(),
        framebuffer_words: framebuffer.len(),
    }
}

fn write_video_phase_signature(hasher: &mut StableTraceHasher, phase: DragonVideoPhase) {
    hasher.write_u64(phase.frame_master_tick);
    hasher.write_usize(phase.physical_line);
    hasher.write_u64(phase.line_master_tick);
    hasher.write_option_usize(phase.visible_line);
    hasher.write_option_usize(phase.active_y);
    hasher.write_option_usize(phase.active_x);
}

#[derive(Debug, Clone, Copy)]
struct StableTraceHasher {
    hash: u64,
}

impl StableTraceHasher {
    const OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;

    const fn new() -> Self {
        Self { hash: Self::OFFSET }
    }

    fn write_u8(&mut self, value: u8) {
        self.hash ^= u64::from(value);
        self.hash = self.hash.wrapping_mul(Self::PRIME);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_option_usize(&mut self, value: Option<usize>) {
        match value {
            Some(value) => {
                self.write_bool(true);
                self.write_usize(value);
            }
            None => self.write_bool(false),
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        for &byte in value.as_bytes() {
            self.write_u8(byte);
        }
    }

    fn finish_hex(self) -> String {
        format!("{:016x}", self.hash)
    }
}

fn vdg_trace_summary(report: &HarnessReport) -> Option<VdgTraceSummary> {
    if report.vdg_samples.is_empty()
        && report.dropped_vdg_samples == 0
        && report.vdg_mode_writes.is_empty()
        && report.dropped_vdg_mode_writes == 0
    {
        return None;
    }

    let mode_writes: Vec<_> = report
        .vdg_mode_writes
        .iter()
        .map(|write| VdgModeWriteSummary {
            cycle: write.cycle,
            frame_master_tick: write.frame_master_tick,
            line: write.line,
            active_y: write.active_y,
            active_x: write.active_x,
            addr: write.addr,
            value: write.value,
            graphics: write.graphics,
            css: write.css,
            int_ext: write.int_ext,
            gm: write.gm,
        })
        .collect();

    Some(VdgTraceSummary {
        dropped_samples: report.dropped_vdg_samples,
        dropped_mode_writes: report.dropped_vdg_mode_writes,
        dropped_device_accesses: report.dropped_device_accesses,
        samples: report
            .vdg_samples
            .iter()
            .map(|sample| VdgSampleSummary {
                cycle: sample.cycle,
                frame_master_tick: sample.frame_master_tick,
                line: sample.line,
                active_y: sample.active_y,
                byte_x: sample.byte_x,
                display_base: sample.display_base,
                sam_video_mode: sample.sam_video_mode,
                sam_display_offset: sample.sam_display_offset,
                pia1_pb: sample.pia1_pb,
                graphics: sample.graphics,
                css: sample.css,
                int_ext: sample.int_ext,
                gm: sample.gm,
            })
            .collect(),
        mode_writes,
    })
}

fn classify_snapshot_smoke(
    stop_reason: StopReason,
    distinct_colors: usize,
    non_background_pixels: usize,
) -> SnapshotSmokeClassification {
    let visible = distinct_colors > 1 && non_background_pixels > 0;
    match (stop_reason, visible) {
        (StopReason::CycleLimit, true) => SnapshotSmokeClassification::RunningVisible,
        (StopReason::CycleLimit, false) => SnapshotSmokeClassification::RunningBlank,
        (StopReason::CpuHalted, true) => SnapshotSmokeClassification::HaltedVisible,
        (StopReason::CpuHalted, false) => SnapshotSmokeClassification::HaltedBlank,
    }
}

fn framebuffer_stats(framebuffer: &[u32]) -> (usize, usize) {
    let Some(&background) = framebuffer.first() else {
        return (0, 0);
    };
    let mut colors = Vec::new();
    let mut non_background_pixels = 0usize;
    for &pixel in framebuffer {
        if pixel != background {
            non_background_pixels += 1;
        }
        if !colors.contains(&pixel) {
            colors.push(pixel);
        }
    }
    (colors.len(), non_background_pixels)
}

fn scan_tape_candidate(
    path: &Path,
    firmware: &LoadedDragonFirmware,
    runtime_smokes: &mut usize,
    smoke: RuntimeSmokeOptions<'_>,
) -> SmokeMatrixRow {
    let loaded = match read_media_asset(path, MediaKind::Tape) {
        Ok(loaded) => loaded,
        Err(err) => {
            return SmokeMatrixRow {
                path: path.display().to_string(),
                archive_member: None,
                parse_status: "read-error".to_owned(),
                blocks: None,
                checksums_valid: None,
                ignored_segments: None,
                ignored_bytes: None,
                header: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let parsed = match parse_cas_tolerant(&loaded.bytes) {
        Ok(image) => image,
        Err(err) => {
            return SmokeMatrixRow {
                path: path.display().to_string(),
                archive_member: loaded.archive_member,
                parse_status: "parse-error".to_owned(),
                blocks: None,
                checksums_valid: None,
                ignored_segments: None,
                ignored_bytes: None,
                header: None,
                runtime: None,
                error: Some(err.to_string()),
            };
        }
    };

    let header = parsed.first_header().map(CasHeaderSummary::from);
    let should_smoke = parsed.first_header().is_some_and(|header| {
        matches!(
            header.file_type,
            CasFileType::Basic | CasFileType::MachineCode
        )
    }) && !parsed.has_ignored_bytes()
        && parsed.checksums_valid()
        && *runtime_smokes < smoke.run_limit;
    let runtime = if should_smoke {
        *runtime_smokes += 1;
        Some(run_runtime_smoke(firmware, &loaded.bytes, &parsed, smoke))
    } else {
        None
    };

    SmokeMatrixRow {
        path: path.display().to_string(),
        archive_member: loaded.archive_member,
        parse_status: if parsed.has_ignored_bytes() {
            "ok-with-ignored-bytes".to_owned()
        } else {
            "ok".to_owned()
        },
        blocks: Some(parsed.blocks.len()),
        checksums_valid: Some(parsed.checksums_valid()),
        ignored_segments: Some(parsed.ignored_ranges.len()),
        ignored_bytes: Some(parsed.ignored_byte_count()),
        header,
        runtime,
        error: None,
    }
}

fn run_runtime_smoke(
    firmware: &LoadedDragonFirmware,
    tape_bytes: &[u8],
    tape: &CasImage,
    smoke_options: RuntimeSmokeOptions<'_>,
) -> CasRuntimeSmoke {
    let command = tape
        .first_header()
        .map(|header| match header.file_type {
            CasFileType::Basic => "CLOAD",
            CasFileType::MachineCode => "CLOADM",
            CasFileType::Data | CasFileType::Unknown(_) => "",
        })
        .unwrap_or("");
    if command.is_empty() {
        return failed_runtime_smoke("", "unsupported tape file type");
    }

    match run_runtime_smoke_inner(firmware, tape_bytes, command, smoke_options) {
        Ok(mut smoke) => {
            if let (Some(config), Some(stem)) = (smoke_options.xroar, smoke_options.xroar_stem) {
                let comparison_screenshot = smoke
                    .start_screenshot
                    .as_ref()
                    .or(smoke.load_screenshot.as_ref());
                match capture_best_xroar_reference(
                    config,
                    &firmware.rom,
                    tape_bytes,
                    command,
                    xroar_start_command(&smoke),
                    comparison_screenshot.map(Path::new),
                    stem,
                ) {
                    Ok(reference) => {
                        smoke.xroar_reference_screenshot =
                            Some(reference.path.display().to_string());
                        smoke.xroar_reference_motoroff = Some(reference.motoroff);
                        smoke.xroar_reference_comparison = reference.comparison;
                    }
                    Err(err) => smoke.xroar_reference_error = Some(err),
                }
            }
            smoke
        }
        Err(error) => failed_runtime_smoke(command, &error),
    }
}

fn run_runtime_smoke_inner(
    firmware: &LoadedDragonFirmware,
    tape_bytes: &[u8],
    command: &str,
    smoke_options: RuntimeSmokeOptions<'_>,
) -> Result<CasRuntimeSmoke, String> {
    let screenshot_stem = smoke_options.screenshot_stem;
    let screenshot_format = smoke_options.screenshot_format;
    let audio_stem = smoke_options.audio_stem;
    let joystick_steps = smoke_options.joystick;
    let joystick_axis_steps = smoke_options.joystick_axis;
    let joystick_axis_sweeps = smoke_options.joystick_axis_sweep;
    let idle_after_start_frames = smoke_options.idle_after_start_frames;
    let mut session = boot_runtime_session(firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, tape_bytes));
    session
        .load_media(&media)
        .map_err(|err| format!("failed to load tape into runtime: {err}"))?;
    let tape_length_bits = query_u64(&session, "dragon.tape.length_bits")?;

    session.clear_audio_capture();
    let load_pc_before = query_u16(&session, "dragon.cpu.pc")?;
    type_basic_command(&mut session, command)?;
    let before_load = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture submitted-load frame: {err}"))?;
    let moved_to = wait_for_tape_position_above(&mut session, 0, 180)?;
    if moved_to == 0 {
        return Err("ROM did not start consuming cassette bits".to_owned());
    }
    let load_wait_frames = load_wait_frame_budget(tape_length_bits);
    let load_stop = wait_for_tape_load_stop(&mut session, load_wait_frames)?;
    if load_stop == TapeLoadStop::FrameLimit {
        return Err(format!(
            "cassette load did not stop within {load_wait_frames} frames for {tape_length_bits} tape bits"
        ));
    }
    let lines_after_load = screen_text_lines(&session)?;
    let after_load = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture post-load frame: {err}"))?;
    let load_visible_change = after_load != before_load;
    let load_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let load_video = video_state(&session)?;
    let load_screenshot = write_smoke_screenshot(
        screenshot_stem,
        "load",
        screenshot_format,
        &session,
        &after_load,
    )?;
    let load_audio = write_smoke_audio(audio_stem, "load", &session)?;
    let load_error = lines_after_load.iter().any(|line| line.contains("ERROR"));
    let load_result = if load_error { "basic-error" } else { "ok" }.to_owned();
    let start_command = if command == "CLOAD" { "RUN" } else { "EXEC" };
    let should_start =
        should_issue_start_command(command, load_error, load_visible_change, load_pc_after);

    let mut start_settle_visible_change = false;
    let (start_result, visible_change_after_start, screen_text, start_audio) = if should_start {
        session.clear_audio_capture();
        type_basic_command(&mut session, start_command)?;
        let changed_frame = wait_for_screenshot_change(&mut session, &after_load, 500)?;
        if let Some(changed_frame) = &changed_frame {
            session
                .run_frames(SMOKE_START_SETTLE_FRAMES)
                .map_err(|err| format!("runtime failed while settling after start: {err}"))?;
            let settled_frame = session
                .screenshot_png_bytes()
                .map_err(|err| format!("failed to capture settled start frame: {err}"))?;
            start_settle_visible_change = settled_frame != *changed_frame;
        }
        let visible_change = changed_frame.is_some();
        let screen_text = screen_text_lines(&session)?;
        let basic_error = screen_text.iter().any(|line| line.contains("ERROR"));
        let start_result = if basic_error {
            "basic-error"
        } else if visible_change {
            "visible-change"
        } else {
            "no-visible-change"
        }
        .to_owned();
        let start_audio = write_smoke_audio(audio_stem, "start", &session)?;
        (start_result, visible_change, screen_text, start_audio)
    } else {
        (
            skipped_start_result(command, load_visible_change, load_pc_after).to_owned(),
            false,
            lines_after_load.clone(),
            None,
        )
    };
    let basic_error = screen_text.iter().any(|line| line.contains("ERROR"));
    let start_pc_after = query_u16(&session, "dragon.cpu.pc")?;
    let start_video = video_state(&session)?;
    let start_screenshot_frame = session
        .screenshot_png_bytes()
        .map_err(|err| format!("failed to capture post-start frame: {err}"))?;
    let start_screenshot = write_smoke_screenshot(
        screenshot_stem,
        "start",
        screenshot_format,
        &session,
        &start_screenshot_frame,
    )?;
    let (idle_visible_change, idle_screen_text, idle_screenshot) = if idle_after_start_frames == 0 {
        (false, None, None)
    } else {
        session.run_frames(idle_after_start_frames).map_err(|err| {
            format!("runtime failed while idling after start for {idle_after_start_frames} frames: {err}")
        })?;
        let idle_frame = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture post-idle frame: {err}"))?;
        let idle_screen_text = screen_text_lines(&session)?;
        let idle_screenshot = write_smoke_screenshot(
            screenshot_stem,
            "idle",
            screenshot_format,
            &session,
            &idle_frame,
        )?;
        (
            idle_frame != start_screenshot_frame,
            Some(idle_screen_text),
            idle_screenshot,
        )
    };
    let mut joystick_axis_sweep_results = Vec::new();
    let (joystick_visible_change, joystick_screen_text, joystick_screenshot) = if joystick_steps
        .is_empty()
        && joystick_axis_steps.is_empty()
        && joystick_axis_sweeps.is_empty()
    {
        (false, None, None)
    } else {
        apply_smoke_joystick_steps(&mut session, joystick_steps)?;
        apply_smoke_joystick_axis_steps(&mut session, joystick_axis_steps)?;
        joystick_axis_sweep_results = apply_smoke_joystick_axis_sweeps(
            &mut session,
            joystick_axis_sweeps,
            &start_screenshot_frame,
            screenshot_stem,
            screenshot_format,
        )?;
        let joystick_frame = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture post-joystick frame: {err}"))?;
        let joystick_screen_text = screen_text_lines(&session)?;
        let joystick_screenshot = write_smoke_screenshot(
            screenshot_stem,
            "joystick",
            screenshot_format,
            &session,
            &joystick_frame,
        )?;
        (
            joystick_frame != start_screenshot_frame,
            Some(joystick_screen_text),
            joystick_screenshot,
        )
    };
    let start_video_changed = load_video != start_video;
    let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
        command,
        load_result: &load_result,
        start_result: &start_result,
        load_visible_change,
        visible_change_after_start,
        start_video_changed,
        start_settle_visible_change,
        basic_error,
        load_video,
        start_video,
    });

    Ok(CasRuntimeSmoke {
        command: command.to_owned(),
        classification,
        load_result,
        start_command: if should_start {
            start_command.to_owned()
        } else {
            String::new()
        },
        start_result,
        load_visible_change,
        load_basic_error: load_error,
        load_pc_before,
        load_pc_after,
        start_pc_after,
        load_video,
        start_video,
        tape_position_bits: query_u64(&session, "dragon.tape.position_bits")?,
        tape_length_bits,
        tape_finished: query_bool(&session, "dragon.tape.finished")?,
        visible_change_after_start,
        start_video_changed,
        start_settle_visible_change,
        basic_error,
        load_screen_text: lines_after_load,
        screen_text,
        error: None,
        load_screenshot,
        load_audio,
        start_screenshot,
        start_audio,
        idle_after_start_frames,
        idle_visible_change,
        idle_screen_text,
        idle_screenshot,
        joystick_steps: joystick_steps.to_vec(),
        joystick_axis_steps: joystick_axis_steps.to_vec(),
        joystick_axis_sweeps: joystick_axis_sweeps.to_vec(),
        joystick_axis_sweep_results,
        joystick_visible_change,
        joystick_screen_text,
        joystick_screenshot,
        xroar_reference_screenshot: None,
        xroar_reference_motoroff: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
    })
}

fn should_issue_start_command(
    command: &str,
    load_error: bool,
    load_visible_change: bool,
    _load_pc_after: u16,
) -> bool {
    !load_error && (matches!(command, "CLOAD" | "CLOADM") || !load_visible_change)
}

fn skipped_start_result(
    command: &str,
    load_visible_change: bool,
    load_pc_after: u16,
) -> &'static str {
    if command == "CLOADM" && load_visible_change && load_pc_after < 0x8000 {
        "already-running-after-load"
    } else if command == "CLOADM" && load_visible_change {
        "start-skipped-load-screen-changed"
    } else {
        "start-skipped"
    }
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSmokeClassificationInput<'a> {
    command: &'a str,
    load_result: &'a str,
    start_result: &'a str,
    load_visible_change: bool,
    visible_change_after_start: bool,
    start_video_changed: bool,
    start_settle_visible_change: bool,
    basic_error: bool,
    load_video: DragonVideoState,
    start_video: DragonVideoState,
}

fn classify_runtime_smoke(
    input: RuntimeSmokeClassificationInput<'_>,
) -> RuntimeSmokeClassification {
    if input.load_result == "error" {
        return RuntimeSmokeClassification::Error;
    }
    if input.load_result == "basic-error" {
        return RuntimeSmokeClassification::LoadBasicError;
    }
    if input.basic_error || input.start_result == "basic-error" {
        return RuntimeSmokeClassification::StartedBasicError;
    }

    let start_uses_graphics = video_state_uses_graphics(input.start_video);
    if input.visible_change_after_start && start_uses_graphics {
        return if input.start_settle_visible_change {
            RuntimeSmokeClassification::StartedGraphicsDrawing
        } else {
            RuntimeSmokeClassification::StartedGraphicsBlank
        };
    }
    if input.start_video_changed {
        return RuntimeSmokeClassification::StartedVideoControlChanged;
    }
    if input.visible_change_after_start {
        return RuntimeSmokeClassification::StartedTextDrawing;
    }
    if input.start_result == "no-visible-change" {
        return RuntimeSmokeClassification::StartedNoVisibleChange;
    }
    if input.command == "CLOADM"
        && input.start_result == "already-running-after-load"
        && input.load_visible_change
    {
        return RuntimeSmokeClassification::MachineCodeRunningAfterLoad;
    }
    if video_state_uses_graphics(input.load_video) {
        return RuntimeSmokeClassification::StartedGraphicsBlank;
    }
    if input.load_visible_change {
        RuntimeSmokeClassification::LoadedVisibleChange
    } else {
        RuntimeSmokeClassification::LoadedNoVisibleChange
    }
}

fn video_state_uses_graphics(video: DragonVideoState) -> bool {
    video.sam_video_mode != 0 || video.pia1_output_b & 0x80 != 0
}

fn video_state(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<DragonVideoState, String> {
    Ok(DragonVideoState {
        sam_video_mode: query_u8(session, "dragon.sam.video_mode")?,
        sam_display_offset: query_u8(session, "dragon.sam.display_offset")?,
        display_base: query_u16(session, "dragon.video.display_base")?,
        pia1_output_b: query_u8(session, "dragon.pia1.output_b")?,
        pia1_ddr_b: query_u8(session, "dragon.pia1.ddr_b")?,
        pia1_control_b: query_u8(session, "dragon.pia1.control_b")?,
        pia1_cb2: query_bool(session, "dragon.pia1.cb2")?,
    })
}

fn write_smoke_screenshot(
    stem: Option<&Path>,
    suffix: &str,
    format: SmokeScreenshotFormat,
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    diagnostic_png: &[u8],
) -> Result<Option<String>, String> {
    let Some(stem) = stem else {
        return Ok(None);
    };
    let path = stem.with_extension(format!("{suffix}.png"));
    let png = match format {
        SmokeScreenshotFormat::Diagnostic => diagnostic_png.to_vec(),
        SmokeScreenshotFormat::XroarZoomed => {
            xroar_zoomed_png_bytes(session.latest_frame().ok_or_else(|| {
                "cannot write xroar-zoomed smoke screenshot before a frame has been captured"
                    .to_owned()
            })?)?
        }
    };
    fs::write(&path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn write_smoke_audio(
    stem: Option<&Path>,
    suffix: &str,
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<Option<String>, String> {
    let Some(stem) = stem else {
        return Ok(None);
    };
    let path = stem.with_extension(format!("{suffix}.wav"));
    session
        .save_audio_capture(&path)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn xroar_zoomed_png_bytes(frame: &CapturedFrame) -> Result<Vec<u8>, String> {
    if frame.format != PixelFormat::Rgba8888 {
        return Err(format!(
            "xroar-zoomed smoke screenshots require RGBA frames; got {:?}",
            frame.format
        ));
    }
    let source = XroarZoomSource::from_frame(frame)?;

    let mut rgba = Vec::with_capacity((XROAR_ZOOMED_WIDTH * XROAR_ZOOMED_HEIGHT * 4) as usize);
    for y in 0..motorola_vdg_6847::TEXT_FRAMEBUFFER_HEIGHT {
        for _ in 0..2 {
            for x in 0..source.active_width {
                let offset = source.pixel_offset(x, y);
                let pixel = &frame.pixels[offset..offset + 4];
                rgba.extend_from_slice(pixel);
                if source.duplicate_x {
                    rgba.extend_from_slice(pixel);
                }
            }
        }
    }

    encode_rgba_png(XROAR_ZOOMED_WIDTH, XROAR_ZOOMED_HEIGHT, &rgba)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct XroarZoomSource {
    frame_width: usize,
    active_x: usize,
    active_y: usize,
    active_width: usize,
    duplicate_x: bool,
}

impl XroarZoomSource {
    fn from_frame(frame: &CapturedFrame) -> Result<Self, String> {
        match (frame.width as usize, frame.height as usize) {
            (TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TEXT_VISIBLE_FRAMEBUFFER_HEIGHT) => Ok(Self {
                frame_width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
                active_x: motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS,
                active_y: motorola_vdg_6847::TEXT_TOP_BORDER_LINES,
                active_width: motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH,
                duplicate_x: true,
            }),
            (VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH, VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT) => Ok(Self {
                frame_width: VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH,
                active_x: VDG_PAL_OVERSCAN_VISIBLE_X
                    + motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS * 2,
                active_y: VDG_PAL_OVERSCAN_VISIBLE_Y + motorola_vdg_6847::TEXT_TOP_BORDER_LINES,
                active_width: motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH * 2,
                duplicate_x: false,
            }),
            (width, height) => Err(format!(
                "xroar-zoomed smoke screenshots require RGBA frames of {}x{} or {}x{}; got {:?} {}x{}",
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
                TEXT_VISIBLE_FRAMEBUFFER_HEIGHT,
                VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH,
                VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT,
                frame.format,
                width,
                height
            )),
        }
    }

    fn pixel_offset(self, x: usize, y: usize) -> usize {
        ((self.active_y + y) * self.frame_width + self.active_x + x) * 4
    }
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(format!(
            "RGBA buffer has {} bytes; expected {expected} for {width}x{height}",
            rgba.len()
        ));
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| format!("failed to write PNG header: {err}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|err| format!("failed to write PNG data: {err}"))?;
        writer
            .finish()
            .map_err(|err| format!("failed to finish PNG: {err}"))?;
    }
    Ok(png)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn compare_png_files(emu_path: &Path, reference_path: &Path) -> Result<ImageComparison, String> {
    let emu_bytes = fs::read(emu_path)
        .map_err(|err| format!("failed to read {}: {err}", emu_path.display()))?;
    let reference_bytes = fs::read(reference_path)
        .map_err(|err| format!("failed to read {}: {err}", reference_path.display()))?;
    let emu = decode_png_rgba(&emu_bytes)?;
    let reference = decode_png_rgba(&reference_bytes)?;
    Ok(compare_images(&emu, &reference))
}

fn decode_png_rgba(bytes: &[u8]) -> Result<DecodedImage, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("failed to read PNG header: {err}"))?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG output buffer size overflows usize".to_owned())?;
    let mut buffer = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| format!("failed to read PNG frame: {err}"))?;
    let data = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in data.chunks_exact(3) {
                out.extend_from_slice(chunk);
                out.push(0xFF);
            }
            out
        }
        png::ColorType::Rgba => data.to_vec(),
        other => {
            return Err(format!(
                "unsupported PNG colour type {other:?}; expected RGB or RGBA"
            ));
        }
    };
    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn compare_images(emu: &DecodedImage, reference: &DecodedImage) -> ImageComparison {
    let dimensions_match = emu.width == reference.width && emu.height == reference.height;
    let compared_pixels = if dimensions_match {
        u64::from(emu.width) * u64::from(emu.height)
    } else {
        0
    };
    if !dimensions_match {
        return ImageComparison {
            emu_width: emu.width,
            emu_height: emu.height,
            reference_width: reference.width,
            reference_height: reference.height,
            dimensions_match,
            compared_pixels,
            differing_pixels: 0,
            differing_pixel_percent: 0.0,
            max_channel_delta: 0,
            mean_abs_channel_delta: 0.0,
        };
    }

    let mut differing_pixels = 0u64;
    let mut max_channel_delta = 0u8;
    let mut total_abs_channel_delta = 0u64;
    for (emu_pixel, reference_pixel) in emu.rgba.chunks_exact(4).zip(reference.rgba.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for channel in 0..3 {
            let delta = emu_pixel[channel].abs_diff(reference_pixel[channel]);
            pixel_differs |= delta != 0;
            max_channel_delta = max_channel_delta.max(delta);
            total_abs_channel_delta += u64::from(delta);
        }
        if pixel_differs {
            differing_pixels += 1;
        }
    }

    let compared_channels = compared_pixels * 3;
    ImageComparison {
        emu_width: emu.width,
        emu_height: emu.height,
        reference_width: reference.width,
        reference_height: reference.height,
        dimensions_match,
        compared_pixels,
        differing_pixels,
        differing_pixel_percent: (differing_pixels as f64 / compared_pixels as f64) * 100.0,
        max_channel_delta,
        mean_abs_channel_delta: total_abs_channel_delta as f64 / compared_channels as f64,
    }
}

#[derive(Debug)]
struct XroarReferenceCapture {
    path: PathBuf,
    motoroff: usize,
    comparison: Option<ImageComparison>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct XroarSnapshotTrap {
    pc: u16,
    count: usize,
}

#[derive(Clone, Copy, Debug)]
struct XroarSnapshotReferenceTiming {
    trap: Option<XroarSnapshotTrap>,
    settle_seconds: f32,
    start_immediate: bool,
}

fn capture_best_xroar_reference(
    config: &XroarReferenceConfig,
    rom: &[u8; ROM_SIZE],
    tape_bytes: &[u8],
    command: &str,
    start_command: Option<&str>,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
) -> Result<XroarReferenceCapture, String> {
    let rom_path = write_temp_bytes("rom", "rom", rom)?;
    let tape_path = write_temp_bytes("tape", "cas", tape_bytes)?;
    let result = capture_best_xroar_reference_inner(
        config,
        command,
        start_command,
        comparison_screenshot,
        stem,
        &rom_path,
        &tape_path,
    );
    let _ = fs::remove_file(&rom_path);
    let _ = fs::remove_file(&tape_path);
    result
}

fn capture_xroar_snapshot_reference(
    config: &XroarReferenceConfig,
    rom: &[u8; ROM_SIZE],
    snapshot: &PcDragonSnapshot,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
    timing: XroarSnapshotReferenceTiming,
) -> Result<XroarReferenceCapture, String> {
    let rom_path = write_temp_bytes("rom", "rom", rom)?;
    let template_path = write_temp_path("snapshot-template", "sna")?;
    let template_result = run_xroar_snapshot_template_command(config, &rom_path, &template_path)
        .and_then(|()| {
            fs::read(&template_path)
                .map_err(|err| format!("failed to read {}: {err}", template_path.display()))
        });
    let snapshot_path = template_result
        .and_then(|template| xroar_v2_snapshot_bytes(&template, snapshot))
        .and_then(|bytes| write_temp_bytes("snapshot", "sna", &bytes));
    let output_path = stem.with_extension("xroar.png");
    let result = snapshot_path.and_then(|snapshot_path| {
        let result = (if let Some(trap) = timing.trap {
            run_xroar_snapshot_trap_reference_command(
                config,
                &rom_path,
                &snapshot_path,
                trap,
                &output_path,
            )
        } else {
            let trap_condition = if timing.start_immediate {
                "immediate".to_owned()
            } else {
                xroar_snapshot_trap_condition(snapshot)
            };
            run_xroar_snapshot_reference_command(
                config,
                &rom_path,
                &snapshot_path,
                &trap_condition,
                &output_path,
                timing.settle_seconds,
            )
        })
        .and_then(|path| {
            let comparison = comparison_screenshot
                .map(|screenshot| compare_png_files(screenshot, &path))
                .transpose()?;
            Ok(XroarReferenceCapture {
                path,
                motoroff: 0,
                comparison,
            })
        });
        let _ = fs::remove_file(&snapshot_path);
        result
    });
    let _ = fs::remove_file(&rom_path);
    let _ = fs::remove_file(&template_path);
    result
}

fn write_xroar_snapshot_out(
    cli: &Cli,
    snapshot: &PcDragonSnapshot,
    output_path: &Path,
) -> Result<(), String> {
    let xroar_bin = cli
        .xroar_bin
        .as_ref()
        .ok_or_else(|| "--xroar-snapshot-out requires --xroar-bin".to_owned())?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let config = XroarReferenceConfig {
        bin: xroar_bin.clone(),
        output_dir: output_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        motoroff: None,
        settle_seconds: DEFAULT_XROAR_SETTLE_SECONDS,
        timeout_seconds: DEFAULT_XROAR_TIMEOUT_SECONDS,
    };

    let rom_path = output_path.with_extension("rom");
    fs::write(&rom_path, load_rom(&cli.rom)?)
        .map_err(|err| format!("failed to write {}: {err}", rom_path.display()))?;
    let template_path = write_temp_path("snapshot-template", "sna")?;
    let template_result = run_xroar_snapshot_template_command(&config, &rom_path, &template_path)
        .and_then(|()| {
            fs::read(&template_path)
                .map_err(|err| format!("failed to read {}: {err}", template_path.display()))
        });
    let result = template_result
        .and_then(|template| xroar_v2_snapshot_bytes(&template, snapshot))
        .and_then(|bytes| {
            fs::write(output_path, bytes)
                .map_err(|err| format!("failed to write {}: {err}", output_path.display()))
        });
    let _ = fs::remove_file(&template_path);
    result
}

fn capture_best_xroar_reference_inner(
    config: &XroarReferenceConfig,
    command: &str,
    start_command: Option<&str>,
    comparison_screenshot: Option<&Path>,
    stem: &Path,
    rom_path: &Path,
    tape_path: &Path,
) -> Result<XroarReferenceCapture, String> {
    let candidates = xroar_motoroff_candidates(config.motoroff, command, start_command);
    let mut best: Option<XroarReferenceCapture> = None;
    let mut errors = Vec::new();

    for motoroff in candidates {
        let output_path = if config.motoroff.is_some() {
            stem.with_extension("xroar.png")
        } else {
            stem.with_extension(format!("xroar-{motoroff}.png"))
        };
        match run_xroar_reference_command(
            config,
            command,
            start_command,
            motoroff,
            rom_path,
            tape_path,
            &output_path,
        ) {
            Ok(path) => {
                let comparison = comparison_screenshot
                    .map(|screenshot| compare_png_files(screenshot, &path))
                    .transpose()?;
                let candidate = XroarReferenceCapture {
                    path,
                    motoroff,
                    comparison,
                };
                if best
                    .as_ref()
                    .is_none_or(|current| xroar_capture_is_better(&candidate, current))
                {
                    best = Some(candidate);
                }
            }
            Err(err) => errors.push(format!("motoroff {motoroff}: {err}")),
        }
    }

    best.ok_or_else(|| {
        if errors.is_empty() {
            "XRoar reference capture had no motor-off candidates".to_owned()
        } else {
            errors.join("; ")
        }
    })
}

fn run_xroar_snapshot_reference_command(
    config: &XroarReferenceConfig,
    rom_path: &Path,
    snapshot_path: &Path,
    trap_condition: &str,
    output_path: &Path,
    settle_seconds: f32,
) -> Result<PathBuf, String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = fs::remove_file(output_path);

    let mut command = Command::new(&config.bin);
    command
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-vo-picture")
        .arg("zoomed")
        .arg("-machine")
        .arg("dragon32");
    append_xroar_rom_args(&mut command, rom_path)?;
    let output = command
        .arg("-load")
        .arg(snapshot_path)
        .arg("-trap")
        .arg(trap_condition)
        .arg("-trap-timeout")
        .arg(format_seconds(settle_seconds))
        .arg("-trap-timeout-screenshot")
        .arg(output_path)
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write reference screenshot {}",
            output_path.display()
        ));
    }
    Ok(output_path.to_owned())
}

fn run_xroar_snapshot_trap_reference_command(
    config: &XroarReferenceConfig,
    rom_path: &Path,
    snapshot_path: &Path,
    trap: XroarSnapshotTrap,
    output_path: &Path,
) -> Result<PathBuf, String> {
    if trap.count == 0 {
        return Err("XRoar snapshot trap count must be greater than zero".to_owned());
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = fs::remove_file(output_path);

    let trap_condition = format!("pc=0x{:04x}", trap.pc);
    let trap_range = format!("{}-{}", trap.count, trap.count);
    let mut command = Command::new(&config.bin);
    command
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-vo-picture")
        .arg("zoomed")
        .arg("-machine")
        .arg("dragon32");
    append_xroar_rom_args(&mut command, rom_path)?;
    let output = command
        .arg("-load")
        .arg(snapshot_path)
        .arg("-trap")
        .arg(trap_condition)
        .arg("-trap-range")
        .arg(trap_range)
        .arg("-trap-screenshot")
        .arg(output_path)
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write reference screenshot {}",
            output_path.display()
        ));
    }
    Ok(output_path.to_owned())
}

fn run_xroar_snapshot_template_command(
    config: &XroarReferenceConfig,
    rom_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let _ = fs::remove_file(output_path);

    let mut command = Command::new(&config.bin);
    command
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-machine")
        .arg("dragon32");
    append_xroar_rom_args(&mut command, rom_path)?;
    let output = command
        .arg("-trap")
        .arg("immediate")
        .arg("-trap-snap")
        .arg(output_path)
        .arg("-trap-timeout")
        .arg("0.01")
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {} while creating v2 snapshot template: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write v2 snapshot template {}",
            output_path.display()
        ));
    }
    Ok(())
}

fn append_xroar_rom_args(command: &mut Command, rom_path: &Path) -> Result<(), String> {
    let parent = rom_path
        .parent()
        .ok_or_else(|| format!("XRoar ROM path {} has no parent", rom_path.display()))?;
    let filename = rom_path
        .file_name()
        .ok_or_else(|| format!("XRoar ROM path {} has no filename", rom_path.display()))?;
    command
        .arg("-rompath")
        .arg(parent)
        .arg("-extbas")
        .arg(filename);
    Ok(())
}

fn xroar_snapshot_trap_condition(snapshot: &PcDragonSnapshot) -> String {
    format!("pc=0x{:04x}", snapshot.registers.pc)
}

fn run_xroar_reference_command(
    config: &XroarReferenceConfig,
    command: &str,
    start_command: Option<&str>,
    motoroff: usize,
    rom_path: &Path,
    tape_path: &Path,
    output_path: &Path,
) -> Result<PathBuf, String> {
    if motoroff == 0 {
        return Err("--xroar-motoroff must be greater than zero".to_owned());
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = fs::remove_file(output_path);

    let trap_range = format!("{motoroff}-{motoroff}");
    let type_text = xroar_type_text(command, start_command)?;
    let output = Command::new(&config.bin)
        .arg("-ui")
        .arg("null")
        .arg("-ao")
        .arg("null")
        .arg("-no-ratelimit")
        .arg("-vo-picture")
        .arg("zoomed")
        .arg("-machine")
        .arg("dragon32")
        .arg("-extbas")
        .arg(rom_path)
        .arg("-load-tape")
        .arg(tape_path)
        .arg("-type")
        .arg(type_text)
        .arg("-trap")
        .arg("tape-motor-off")
        .arg("-trap-range")
        .arg(trap_range)
        .arg("-trap-timeout")
        .arg(format_seconds(config.settle_seconds))
        .arg("-trap-timeout-screenshot")
        .arg(output_path)
        .arg("-timeout")
        .arg(format_seconds(config.timeout_seconds))
        .arg("-q")
        .output()
        .map_err(|err| format!("failed to run {}: {err}", config.bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "XRoar exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if !output_path.is_file() {
        return Err(format!(
            "XRoar did not write reference screenshot {}",
            output_path.display()
        ));
    }
    Ok(output_path.to_owned())
}

fn write_temp_bytes(kind: &str, extension: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = write_temp_path(kind, extension)?;
    fs::write(&path, bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn write_temp_path(kind: &str, extension: &str) -> Result<PathBuf, String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock was before UNIX epoch: {err}"))?
        .as_nanos();
    let path = env::temp_dir().join(format!(
        "emu198x-xroar-{}-{unique}-{kind}.{extension}",
        process::id()
    ));
    Ok(path)
}

fn xroar_v2_snapshot_bytes(
    template: &[u8],
    snapshot: &PcDragonSnapshot,
) -> Result<Vec<u8>, String> {
    let mut bytes = template.to_vec();
    let ram = xroar_snapshot_ram(snapshot);
    patch_xroar_v2_ram(&mut bytes, &ram)?;
    patch_xroar_v2_pias(&mut bytes, snapshot)?;
    patch_xroar_v2_vdg(&mut bytes, snapshot)?;
    patch_xroar_v2_cpu(&mut bytes, snapshot)?;
    patch_xroar_v2_sam(&mut bytes, snapshot)?;
    Ok(bytes)
}

fn patch_xroar_v2_ram(bytes: &mut [u8], ram: &[u8]) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"RAM", Some(b"VDG"))?;
    let bank = xroar_v2_find_tag_payload(bytes, range.start, range.end, 7)
        .ok_or_else(|| "XRoar v2 RAM bank tag not found".to_owned())?;
    let data = xroar_v2_find_tag_payload(bytes, bank.payload_start, range.end, 1)
        .ok_or_else(|| "XRoar v2 RAM payload tag not found".to_owned())?;
    if data.payload_end - data.payload_start != ram.len() {
        return Err(format!(
            "XRoar v2 RAM payload size mismatch: template has {}, snapshot has {}",
            data.payload_end - data.payload_start,
            ram.len()
        ));
    }
    bytes[data.payload_start..data.payload_end].copy_from_slice(ram);
    Ok(())
}

fn patch_xroar_v2_pias(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let peripherals = snapshot.peripherals.unwrap_or(PcDragonPeripherals {
        ff02: 0,
        ff03: 0,
        ff22: 0,
    });

    patch_xroar_v2_pia(
        bytes,
        b"PIA0",
        Some(b"CPU"),
        XroarV2PiaPortB {
            control: peripherals.ff03,
            ddr: 0xff,
            output: peripherals.ff02,
            irq1: peripherals.ff03 & 0x80,
            irq2: peripherals.ff03 & 0x40,
        },
    )?;
    patch_xroar_v2_pia(
        bytes,
        b"PIA1",
        Some(b"PIA0"),
        XroarV2PiaPortB {
            control: 0x04,
            ddr: 0xff,
            output: peripherals.ff22,
            irq1: 0,
            irq2: 0,
        },
    )
}

#[derive(Clone, Copy)]
struct XroarV2PiaPortB {
    control: u8,
    ddr: u8,
    output: u8,
    irq1: u8,
    irq2: u8,
}

fn patch_xroar_v2_pia(
    bytes: &mut Vec<u8>,
    component: &[u8],
    next_component: Option<&[u8]>,
    port_b: XroarV2PiaPortB,
) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, component, next_component)?;
    let part_start = find_bytes_from_until(bytes, b"MC6821", range.start, range.end)
        .map(|offset| offset + b"MC6821".len())
        .ok_or_else(|| {
            format!(
                "XRoar v2 {} MC6821 part payload not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_a_marker =
        find_bytes_from_until(bytes, &[1, 0], part_start, range.end).ok_or_else(|| {
            format!(
                "XRoar v2 {} port A tag not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_b_marker = find_bytes_from_until(bytes, &[2, 0], side_a_marker + 2, range.end)
        .ok_or_else(|| {
            format!(
                "XRoar v2 {} port B tag not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let side_a = side_a_marker + 2;
    let mut side_a_end = side_b_marker;
    patch_xroar_v2_pia_side_a(bytes, side_a, &mut side_a_end)?;
    let side_b_delta = side_a_end
        .checked_sub(side_b_marker)
        .ok_or_else(|| "XRoar v2 PIA side range underflowed".to_owned())?;
    let side_b = side_b_marker + side_b_delta + 2;
    let mut side_b_end = range.end + side_b_delta;
    patch_xroar_v2_pia_side_b(bytes, side_b, &mut side_b_end, port_b)
}

fn patch_xroar_v2_pia_side_a(
    bytes: &mut Vec<u8>,
    side_a: usize,
    side_end: &mut usize,
) -> Result<(), String> {
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 2, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 3, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 5, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 13, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 6, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 8, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 9, 0xff)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 10, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_a, side_end, 11, 0xff)?;
    Ok(())
}

fn patch_xroar_v2_pia_side_b(
    bytes: &mut Vec<u8>,
    side_b: usize,
    side_end: &mut usize,
    port_b: XroarV2PiaPortB,
) -> Result<(), String> {
    if side_b >= *side_end {
        return Err(format!(
            "XRoar v2 PIA port B range is empty: {side_b}..{}",
            *side_end
        ));
    }
    let out_source = port_b.output & port_b.ddr;
    let out_sink = port_b.output | !port_b.ddr;
    let irq = xroar_v2_pia_irq(port_b.control, port_b.irq1, port_b.irq2);

    patch_xroar_v2_vuint_field_in_range(
        bytes,
        side_b,
        side_end,
        1,
        u32::from(port_b.control & 0x3f),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 2, u32::from(port_b.ddr))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 3, u32::from(port_b.output))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 5, u32::from(port_b.irq1))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 13, u32::from(port_b.irq2))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 6, u32::from(irq))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 8, u32::from(out_source))?;
    patch_xroar_v2_vuint_field_in_range(bytes, side_b, side_end, 9, u32::from(out_sink))?;
    Ok(())
}

fn xroar_v2_pia_irq(control: u8, irq1: u8, irq2: u8) -> u8 {
    let irq1_active = if control & 0x01 != 0 { irq1 } else { 0 };
    let irq2_active = if control & 0x28 == 0x08 { irq2 } else { 0 };
    irq1_active | irq2_active
}

fn patch_xroar_v2_vdg(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    const VDG_GREEN: u32 = 0;
    const VDG_WHITE: u32 = 4;
    const VDG_BLACK: u32 = 8;
    const VDG_DARK_GREEN: u32 = 9;
    const VDG_RENDER_CG: u32 = 1;
    const VDG_RENDER_RG: u32 = 2;
    const VDG_TLB: u32 = 120;
    const VDG_TRB: u32 = 112;
    const VDG_HS_RISING_EDGE_DELTA: u32 = 64;
    const VDG_LINE_DURATION: u32 = 912;
    const VDG_LEFT_BORDER_START: u32 = 134;
    const VDG_INITIAL_SCANLINE: u32 = 0;
    const GM_NLPR: [u32; 8] = [3, 3, 3, 2, 2, 1, 1, 1];

    let range = xroar_v2_component_range(bytes, b"VDG", Some(b"PIA1"))?;
    let start = find_bytes_from_until(bytes, b"MC6847", range.start, range.end)
        .map(|offset| offset + b"MC6847".len())
        .ok_or_else(|| "XRoar v2 VDG part payload not found".to_owned())?;
    let mut end = range.end;
    let mode = snapshot
        .peripherals
        .map(|peripherals| peripherals.ff22 & 0xf8)
        .unwrap_or(0);
    let gm = u32::from((mode >> 4) & 0x07);
    let gm0 = gm & 1;
    let css = u32::from(mode & 0x08 != 0);
    let graphics = u32::from(mode & 0x80 != 0);
    let is_32byte = u32::from(graphics == 0 || !(gm == 0 || (gm0 != 0 && gm != 7)));
    let cg_colours = if css != 0 { VDG_WHITE } else { VDG_GREEN };
    let fg_colour = if css != 0 { VDG_WHITE } else { VDG_GREEN };
    let bg_colour = if css != 0 { VDG_BLACK } else { VDG_DARK_GREEN };
    let render_mode = if gm0 != 0 {
        VDG_RENDER_RG
    } else {
        VDG_RENDER_CG
    };

    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, gm)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 8, graphics)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 9, gm0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 10, css)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 11, css)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 12, css)?;
    upsert_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, VDG_LINE_DURATION)?;
    upsert_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 15, VDG_HS_RISING_EDGE_DELTA)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, VDG_LEFT_BORDER_START)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, VDG_INITIAL_SCANLINE)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 21, is_32byte)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 22, gm0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 25, fg_colour)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 26, bg_colour)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 27, cg_colours)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 28, cg_colours)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 30, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 31, render_mode)?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        33,
        u32::from(!(graphics != 0 && css != 0 && gm0 != 0)),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 35, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 36, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 37, VDG_TLB)?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        38,
        if is_32byte != 0 { 32 } else { 16 },
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 39, VDG_TRB)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 40, GM_NLPR[gm as usize])
}

fn patch_xroar_v2_cpu(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"CPU", Some(b"SAM"))?;
    let start = find_bytes_from_until(bytes, b"MC6809", range.start, range.end)
        .map(|offset| offset + b"MC6809".len())
        .ok_or_else(|| "XRoar v2 CPU part payload not found".to_owned())?;
    let mut end = range.end;
    let registers = &snapshot.registers;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 2, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 3, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 4, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 5, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 7, 1)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 8, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 9, u32::from(registers.cc))?;
    patch_xroar_v2_vuint_field_in_range(
        bytes,
        start,
        &mut end,
        10,
        u32::from(u16::from_be_bytes([registers.a, registers.b])),
    )?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 11, u32::from(registers.dp))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 12, u32::from(registers.x))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 13, u32::from(registers.y))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, u32::from(registers.u))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 15, u32::from(registers.s))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, u32::from(registers.pc))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 19, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 20, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 21, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 22, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 23, 0)
}

fn patch_xroar_v2_sam(bytes: &mut Vec<u8>, snapshot: &PcDragonSnapshot) -> Result<(), String> {
    let range = xroar_v2_component_range(bytes, b"SAM", None)?;
    let start = find_bytes_from_until(bytes, b"SN74LS783", range.start, range.end)
        .map(|offset| offset + b"SN74LS783".len())
        .ok_or_else(|| "XRoar v2 SAM part payload not found".to_owned())?;
    let mut end = range.end;
    let sam = u16::from_be_bytes(xroar_snapshot_sam(snapshot));
    upsert_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 5, u32::from(sam))?;
    let v = u32::from(sam & 0x0007);
    let f = u32::from((sam << 6) & 0xfe00);
    let p = u32::from((sam >> 10) & 0x0001);
    let r = u32::from((sam >> 11) & 0x0003);
    let m = u32::from((sam >> 13) & 0x0003);
    let ty = u32::from((sam >> 15) & 0x0001);
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 6, ty)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 13, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 14, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 15, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 16, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 17, v)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 18, f)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 19, xroar_v2_sam_clr_mode(v))?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 28, p)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 29, r)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 30, m)?;
    patch_xroar_v2_vuint_field_in_range(bytes, start, &mut end, 36, v)?;
    patch_xroar_v2_sam_vcounters(bytes, start, end, f >> 5)
}

fn patch_xroar_v2_sam_vcounters(
    bytes: &mut Vec<u8>,
    start: usize,
    end: usize,
    b15_5: u32,
) -> Result<(), String> {
    for tag in 20..=27 {
        let value = if tag == 20 { b15_5 } else { 0 };
        patch_xroar_v2_sam_vcounter(bytes, start, end, tag, value)?;
    }
    Ok(())
}

fn patch_xroar_v2_sam_vcounter(
    bytes: &mut Vec<u8>,
    start: usize,
    component_end: usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    let vcounter = xroar_v2_find_tag_payload(bytes, start, component_end, tag)
        .ok_or_else(|| format!("XRoar v2 SAM video counter {tag} not found"))?;
    let mut end = xroar_v2_find_tag_payload(bytes, vcounter.payload_end, component_end, tag + 1)
        .map_or(component_end, |next| next.tag_start);
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 1, 0)?;
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 2, value)?;
    patch_xroar_v2_vuint_field_in_range(bytes, vcounter.payload_end, &mut end, 3, 0)
}

fn xroar_v2_sam_clr_mode(video_mode: u32) -> u32 {
    const CLR_N: u32 = 0;
    const CLR_3: u32 = 1;
    const CLR_4: u32 = 2;
    match video_mode {
        1 | 3 | 5 => CLR_3,
        7 => CLR_N,
        _ => CLR_4,
    }
}

fn patch_xroar_v2_vuint_field_in_range(
    bytes: &mut Vec<u8>,
    start: usize,
    end: &mut usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    let delta = patch_xroar_v2_vuint_field(bytes, start, *end, tag, value)?;
    if delta.is_negative() {
        *end = end
            .checked_sub(delta.unsigned_abs())
            .ok_or_else(|| "XRoar v2 component range underflowed while patching".to_owned())?;
    } else {
        *end = end
            .checked_add(delta as usize)
            .ok_or_else(|| "XRoar v2 component range overflowed while patching".to_owned())?;
    }
    Ok(())
}

fn upsert_xroar_v2_vuint_field_in_range(
    bytes: &mut Vec<u8>,
    start: usize,
    end: &mut usize,
    tag: u32,
    value: u32,
) -> Result<(), String> {
    if xroar_v2_find_tag_payload(bytes, start, *end, tag).is_some() {
        patch_xroar_v2_vuint_field_in_range(bytes, start, end, tag, value)
    } else {
        let delta = insert_xroar_v2_vuint_field(bytes, start, tag, value);
        *end = end
            .checked_add(delta)
            .ok_or_else(|| "XRoar v2 component range overflowed while inserting".to_owned())?;
        Ok(())
    }
}

fn insert_xroar_v2_vuint_field(bytes: &mut Vec<u8>, offset: usize, tag: u32, value: u32) -> usize {
    let mut field = xroar_v2_vuint(tag);
    field.extend_from_slice(&xroar_v2_vuint(xroar_v2_vuint(value).len() as u32));
    field.extend_from_slice(&xroar_v2_vuint(value));
    field.push(0);
    let len = field.len();
    bytes.splice(offset..offset, field);
    len
}

fn patch_xroar_v2_vuint_field(
    bytes: &mut Vec<u8>,
    start: usize,
    end: usize,
    tag: u32,
    value: u32,
) -> Result<isize, String> {
    let field = xroar_v2_find_tag_payload(bytes, start, end, tag)
        .ok_or_else(|| format!("XRoar v2 tag {tag} not found"))?;
    let original_len = field.payload_end - field.tag_start;
    let mut replacement = xroar_v2_vuint(tag);
    replacement.extend_from_slice(&xroar_v2_vuint(xroar_v2_vuint(value).len() as u32));
    replacement.extend_from_slice(&xroar_v2_vuint(value));
    let replacement_len = replacement.len();
    bytes.splice(field.tag_start..field.payload_end, replacement);
    Ok(replacement_len as isize - original_len as isize)
}

#[derive(Clone, Copy)]
struct XroarV2Field {
    tag_start: usize,
    payload_start: usize,
    payload_end: usize,
}

fn xroar_v2_find_tag_payload(
    bytes: &[u8],
    start: usize,
    end: usize,
    expected_tag: u32,
) -> Option<XroarV2Field> {
    let mut offset = start;
    while offset < end {
        if bytes[offset] == 0 {
            offset += 1;
            continue;
        }
        let tag_start = offset;
        let (tag, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let (len, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let len = usize::try_from(len).ok()?;
        let payload_end = offset.checked_add(len)?;
        if payload_end > end || payload_end > bytes.len() {
            return None;
        }
        if tag == expected_tag {
            return Some(XroarV2Field {
                tag_start,
                payload_start: offset,
                payload_end,
            });
        }
        offset = payload_end;
    }
    None
}

fn xroar_v2_read_vuint(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let mut offset = start;
    let byte0 = *bytes.get(offset)?;
    offset += 1;
    let mut value = u32::from(byte0);
    let mut mask = 0x7f;
    let mut marker = byte0;
    for _ in 1..5 {
        if marker & 0x80 == 0 {
            break;
        }
        marker <<= 1;
        let byte = *bytes.get(offset)?;
        offset += 1;
        mask = (mask << 7) | 0x7f;
        value = (value << 8) | u32::from(byte);
    }
    Some((value & mask, offset))
}

fn xroar_v2_vuint(value: u32) -> Vec<u8> {
    if value <= 0x7f {
        vec![value as u8]
    } else if value <= 0x3fff {
        ((value | 0x8000) as u16).to_be_bytes().to_vec()
    } else if value <= 0x1f_ffff {
        let mut bytes = Vec::with_capacity(3);
        bytes.push(0xc0 | ((value >> 16) as u8));
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    } else if value <= 0x0fff_ffff {
        let mut bytes = Vec::with_capacity(4);
        bytes.extend_from_slice(&((0xe000 | (value >> 16)) as u16).to_be_bytes());
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    } else {
        let mut bytes = Vec::with_capacity(5);
        bytes.push(0xf0);
        bytes.extend_from_slice(&((value >> 16) as u16).to_be_bytes());
        bytes.extend_from_slice(&(value as u16).to_be_bytes());
        bytes
    }
}

fn xroar_v2_component_range(
    bytes: &[u8],
    component: &[u8],
    next_component: Option<&[u8]>,
) -> Result<std::ops::Range<usize>, String> {
    let marker = xroar_v2_component_marker(component);
    let start = find_bytes(bytes, &marker).ok_or_else(|| {
        format!(
            "XRoar v2 component {} not found",
            String::from_utf8_lossy(component)
        )
    })?;
    let structural_end = xroar_v2_open_tag_end(bytes, start + marker.len(), bytes.len())
        .ok_or_else(|| {
            format!(
                "XRoar v2 component {} terminator not found",
                String::from_utf8_lossy(component)
            )
        })?;
    let end = match next_component {
        Some(next) => {
            let next_marker = xroar_v2_component_marker(next);
            let next_start = find_bytes_from(bytes, &next_marker, start + marker.len())
                .ok_or_else(|| {
                    format!(
                        "XRoar v2 component {} terminator {} not found",
                        String::from_utf8_lossy(component),
                        String::from_utf8_lossy(next)
                    )
                })?;
            if next_start > structural_end {
                return Err(format!(
                    "XRoar v2 component {} terminator {} is outside the component tree",
                    String::from_utf8_lossy(component),
                    String::from_utf8_lossy(next)
                ));
            }
            next_start
        }
        None => structural_end,
    };
    Ok(start..end)
}

fn xroar_v2_open_tag_end(bytes: &[u8], mut offset: usize, end: usize) -> Option<usize> {
    let mut open_tags = 1usize;
    let mut tag_open = false;
    while offset < end {
        let (tag, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        if tag == 0 {
            if tag_open {
                tag_open = false;
                continue;
            }
            open_tags = open_tags.checked_sub(1)?;
            if open_tags == 0 {
                return Some(offset);
            }
            continue;
        }
        if tag_open {
            open_tags = open_tags.checked_add(1)?;
        }
        tag_open = true;
        let (len, next) = xroar_v2_read_vuint(bytes, offset)?;
        offset = next;
        let len = usize::try_from(len).ok()?;
        offset = offset.checked_add(len)?;
        if offset > end || offset > bytes.len() {
            return None;
        }
    }
    None
}

fn xroar_v2_component_marker(component: &[u8]) -> Vec<u8> {
    let mut marker = xroar_v2_vuint(1);
    marker.extend_from_slice(&xroar_v2_vuint(component.len() as u32));
    marker.extend_from_slice(component);
    marker
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    find_bytes_from_until(haystack, needle, start, haystack.len())
}

fn find_bytes_from_until(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
    end: usize,
) -> Option<usize> {
    if needle.is_empty() || start >= end || end > haystack.len() {
        return None;
    }
    haystack[start..end]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

#[cfg(test)]
fn xroar_v1_snapshot_bytes(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    const ID_RAM_PAGE0: u8 = 1;
    const ID_PIA_REGISTERS: u8 = 2;
    const ID_SAM_REGISTERS: u8 = 3;
    const ID_MC6809_STATE: u8 = 4;
    const ID_MACHINECONFIG: u8 = 8;
    const ID_SNAPVERSION: u8 = 9;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"XRoar snapshot.\n\0");
    push_xroar_v1_chunk(&mut bytes, ID_SNAPVERSION, &[1, 0, 8]);
    push_xroar_v1_chunk(&mut bytes, ID_MACHINECONFIG, &[0, 0, 0, 0, 0, 32, 0, 0]);
    push_xroar_v1_chunk(&mut bytes, ID_RAM_PAGE0, &xroar_snapshot_ram(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_PIA_REGISTERS, &xroar_snapshot_pias(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_SAM_REGISTERS, &xroar_snapshot_sam(snapshot));
    push_xroar_v1_chunk(&mut bytes, ID_MC6809_STATE, &xroar_snapshot_cpu(snapshot));
    bytes
}

#[cfg(test)]
fn push_xroar_v1_chunk(bytes: &mut Vec<u8>, section: u8, payload: &[u8]) {
    bytes.push(section);
    push_be_u16(bytes, payload.len() as u16);
    bytes.extend_from_slice(payload);
}

fn xroar_snapshot_ram(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    let mut ram = vec![0; 0x8000];
    let start = usize::from(snapshot.load_address);
    if start < ram.len() {
        let len = snapshot.ram.len().min(ram.len() - start);
        ram[start..start + len].copy_from_slice(&snapshot.ram[..len]);
    }
    ram
}

#[cfg(test)]
fn xroar_snapshot_pias(snapshot: &PcDragonSnapshot) -> [u8; 12] {
    let peripherals = snapshot.peripherals.unwrap_or(PcDragonPeripherals {
        ff02: 0,
        ff03: 0,
        ff22: 0,
    });
    [
        0,
        0,
        0,
        0xff,
        peripherals.ff02,
        peripherals.ff03,
        0,
        0,
        0,
        0xff,
        peripherals.ff22,
        0x04,
    ]
}

fn xroar_snapshot_sam(snapshot: &PcDragonSnapshot) -> [u8; 2] {
    const DRAGON32_MEMORY_SIZE_BITS: u16 = 2 << 13;

    let display_register = snapshot
        .display_base
        .map(|base| (base >> 6) & 0x03f8)
        .unwrap_or(0);
    let video_mode = xroar_snapshot_sam_video_mode(
        snapshot
            .peripherals
            .map(|peripherals| peripherals.ff22 & 0xf8)
            .unwrap_or(0),
    );
    let register = DRAGON32_MEMORY_SIZE_BITS | display_register | video_mode;
    register.to_be_bytes()
}

fn xroar_snapshot_sam_video_mode(vdg_mode: u8) -> u16 {
    if vdg_mode & 0x80 == 0 {
        return 0;
    }

    match (vdg_mode >> 4) & 0x07 {
        0 | 1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 | 7 => 6,
        _ => unreachable!("masked VDG GM bits are always 0..=7"),
    }
}

#[cfg(test)]
fn xroar_snapshot_cpu(snapshot: &PcDragonSnapshot) -> Vec<u8> {
    let mut cpu = Vec::with_capacity(20);
    cpu.push(snapshot.registers.cc);
    cpu.push(snapshot.registers.a);
    cpu.push(snapshot.registers.b);
    cpu.push(snapshot.registers.dp);
    push_be_u16(&mut cpu, snapshot.registers.x);
    push_be_u16(&mut cpu, snapshot.registers.y);
    push_be_u16(&mut cpu, snapshot.registers.u);
    push_be_u16(&mut cpu, snapshot.registers.s);
    push_be_u16(&mut cpu, snapshot.registers.pc);
    cpu.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    cpu
}

#[cfg(test)]
fn push_be_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn xroar_start_command(smoke: &CasRuntimeSmoke) -> Option<&str> {
    if smoke.start_command.is_empty() {
        None
    } else {
        Some(smoke.start_command.as_str())
    }
}

fn xroar_type_text(command: &str, start_command: Option<&str>) -> Result<String, String> {
    match (command, start_command) {
        ("CLOAD", Some("RUN")) => Ok("CLOAD\rRUN\r".to_owned()),
        ("CLOAD", None) => Ok("CLOAD\r".to_owned()),
        ("CLOADM", Some("EXEC")) => Ok("CLOADM\rEXEC\r".to_owned()),
        ("CLOADM", None) => Ok("CLOADM\r".to_owned()),
        _ => Err(format!(
            "unsupported XRoar reference command {command:?} with start {start_command:?}"
        )),
    }
}

fn default_xroar_motoroff(command: &str, start_command: Option<&str>) -> usize {
    match (command, start_command) {
        ("CLOAD", None) => 3,
        _ => 2,
    }
}

fn xroar_motoroff_candidates(
    configured: Option<usize>,
    command: &str,
    start_command: Option<&str>,
) -> Vec<usize> {
    if let Some(motoroff) = configured {
        return vec![motoroff];
    }

    let mut candidates = vec![default_xroar_motoroff(command, start_command), 2, 3];
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn xroar_capture_is_better(
    candidate: &XroarReferenceCapture,
    current: &XroarReferenceCapture,
) -> bool {
    match (&candidate.comparison, &current.comparison) {
        (Some(candidate), Some(current)) => {
            (
                candidate.differing_pixels,
                candidate.max_channel_delta,
                candidate.mean_abs_channel_delta.to_bits(),
            ) < (
                current.differing_pixels,
                current.max_channel_delta,
                current.mean_abs_channel_delta.to_bits(),
            )
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => false,
    }
}

fn format_seconds(seconds: f32) -> String {
    if seconds.fract() == 0.0 {
        format!("{seconds:.0}")
    } else {
        seconds.to_string()
    }
}

fn xroar_snapshot_settle_seconds(cycles: u64, master_ticks: u64) -> f32 {
    if master_ticks != 0 {
        master_ticks as f32 / DRAGON_MASTER_HZ as f32
    } else {
        cycles as f32 / DRAGON_CPU_HZ as f32
    }
}

fn failed_runtime_smoke(command: &str, error: &str) -> CasRuntimeSmoke {
    let start_command = if command == "CLOAD" {
        "RUN"
    } else if command == "CLOADM" {
        "EXEC"
    } else {
        ""
    };
    CasRuntimeSmoke {
        command: command.to_owned(),
        classification: RuntimeSmokeClassification::Error,
        load_result: "error".to_owned(),
        start_command: start_command.to_owned(),
        start_result: "not-run".to_owned(),
        load_visible_change: false,
        load_basic_error: false,
        load_pc_before: 0,
        load_pc_after: 0,
        start_pc_after: 0,
        load_video: DragonVideoState::default(),
        start_video: DragonVideoState::default(),
        tape_position_bits: 0,
        tape_length_bits: 0,
        tape_finished: false,
        visible_change_after_start: false,
        start_video_changed: false,
        start_settle_visible_change: false,
        basic_error: false,
        load_screen_text: Vec::new(),
        screen_text: Vec::new(),
        error: Some(error.to_owned()),
        load_screenshot: None,
        load_audio: None,
        start_screenshot: None,
        start_audio: None,
        idle_after_start_frames: 0,
        idle_visible_change: false,
        idle_screen_text: None,
        idle_screenshot: None,
        joystick_steps: Vec::new(),
        joystick_axis_steps: Vec::new(),
        joystick_axis_sweeps: Vec::new(),
        joystick_axis_sweep_results: Vec::new(),
        joystick_visible_change: false,
        joystick_screen_text: None,
        joystick_screenshot: None,
        xroar_reference_screenshot: None,
        xroar_reference_motoroff: None,
        xroar_reference_error: None,
        xroar_reference_comparison: None,
        xroar_reference_comparison_error: None,
    }
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).map_or(20_000, |frames| frames)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapeLoadStop {
    MotorOff,
    TapeFinished,
    FrameLimit,
}

fn wait_for_tape_load_stop(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> Result<TapeLoadStop, String> {
    for _ in 0..=max_frames {
        if !query_bool(session, "dragon.tape.motor_on")? {
            return Ok(TapeLoadStop::MotorOff);
        }
        if query_bool(session, "dragon.tape.finished")? {
            return Ok(TapeLoadStop::TapeFinished);
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for tape load stop: {err}"))?;
    }
    Ok(TapeLoadStop::FrameLimit)
}

fn runtime_session(
    firmware: &LoadedDragonFirmware,
) -> Result<HeadlessSession<DragonRuntime, DragonSessionQueryProvider>, String> {
    let mut firmware_set = FirmwareSet::new();
    match firmware.model {
        Model::Dragon32Pal => {
            firmware_set.push(FirmwareImage::new("dragon32-basic-rom", &firmware.rom));
        }
        Model::Dragon64Pal => {
            let mode_rom = firmware
                .mode_rom
                .as_ref()
                .ok_or_else(|| "Dragon 64 runtime requires --rom64 PATH".to_owned())?;
            firmware_set.push(FirmwareImage::new("dragon64-compatible-rom", &firmware.rom));
            firmware_set.push(FirmwareImage::new("dragon64-basic-rom", mode_rom));
        }
    }
    let runtime = DragonRuntime::from_firmware(firmware.model, &firmware_set)
        .map_err(|err| format!("failed to build Dragon runtime: {err}"))?;
    Ok(HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    ))
}

fn boot_runtime_session(
    firmware: &LoadedDragonFirmware,
) -> Result<HeadlessSession<DragonRuntime, DragonSessionQueryProvider>, String> {
    let mut session = runtime_session(firmware)?;
    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .map_err(|err| format!("Dragon BASIC boot did not complete: {err}"))?;
    if boot.reason != "basic-ok-prompt" {
        return Err(format!("unexpected boot reason {}", boot.reason));
    }
    session
        .run_frames(30)
        .map_err(|err| format!("Dragon runtime did not idle after boot: {err}"))?;
    Ok(session)
}

fn type_basic_command(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
) -> Result<(), String> {
    for ch in command.chars() {
        tap_basic_char(session, ch)?;
    }
    tap_key(session, "enter")
}

fn type_basic_command_with_trace(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
    trace_sink: &mut impl TraceSink,
) -> Result<(), String> {
    for ch in command.chars() {
        tap_basic_char_with_trace(session, ch, trace_sink)?;
    }
    tap_key_with_trace(session, "enter", trace_sink)
}

fn tap_basic_char(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    ch: char,
) -> Result<(), String> {
    if let Some(combo) = dragon_basic_key_combo(ch) {
        return tap_key_combo(session, combo);
    }
    tap_key(session, &dragon_basic_key_name(ch))
}

fn tap_basic_char_with_trace(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    ch: char,
    trace_sink: &mut impl TraceSink,
) -> Result<(), String> {
    if let Some(combo) = dragon_basic_key_combo(ch) {
        return tap_key_combo_with_trace(session, combo, trace_sink);
    }
    tap_key_with_trace(session, &dragon_basic_key_name(ch), trace_sink)
}

fn dragon_basic_key_combo(ch: char) -> Option<&'static [&'static str]> {
    match ch {
        '"' => Some(&["shift", "2"]),
        _ => None,
    }
}

fn dragon_basic_key_name(ch: char) -> String {
    match ch {
        ' ' => "space".to_owned(),
        _ => ch.to_ascii_lowercase().to_string(),
    }
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
) -> Result<(), String> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key press {name} failed: {err}"))?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key release {name} failed: {err}"))?;
    Ok(())
}

fn tap_key_with_trace(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
    trace_sink: &mut impl TraceSink,
) -> Result<(), String> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames_with_trace_sink(KEY_EDGE_FRAMES, trace_sink)
        .map_err(|err| format!("key press {name} failed: {err}"))?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session
        .run_frames_with_trace_sink(KEY_EDGE_FRAMES, trace_sink)
        .map_err(|err| format!("key release {name} failed: {err}"))?;
    Ok(())
}

fn tap_key_combo(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    names: &[&str],
) -> Result<(), String> {
    for name in names {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: true,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key combo press {names:?} failed: {err}"))?;
    for name in names.iter().rev() {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: false,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .map_err(|err| format!("key combo release {names:?} failed: {err}"))?;
    Ok(())
}

fn tap_key_combo_with_trace(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    names: &[&str],
    trace_sink: &mut impl TraceSink,
) -> Result<(), String> {
    for name in names {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: true,
        });
    }
    session
        .run_frames_with_trace_sink(KEY_EDGE_FRAMES, trace_sink)
        .map_err(|err| format!("key combo press {names:?} failed: {err}"))?;
    for name in names.iter().rev() {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: false,
        });
    }
    session
        .run_frames_with_trace_sink(KEY_EDGE_FRAMES, trace_sink)
        .map_err(|err| format!("key combo release {names:?} failed: {err}"))?;
    Ok(())
}

fn apply_smoke_joystick_steps(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    steps: &[SmokeJoystickStep],
) -> Result<(), String> {
    for step in steps {
        let name = step.control.name();
        if step.control == SmokeJoystickControl::Idle {
            session
                .run_frames(step.frames)
                .map_err(|err| format!("joystick idle for {} frames failed: {err}", step.frames))?;
            continue;
        }
        session.queue_input(InputEvent::Button {
            port: step.port,
            name: name.into(),
            pressed: true,
        });
        session.run_frames(step.frames).map_err(|err| {
            format!(
                "joystick press port {} {name} for {} frames failed: {err}",
                step.port, step.frames
            )
        })?;
        session.queue_input(InputEvent::Button {
            port: step.port,
            name: name.into(),
            pressed: false,
        });
        session
            .run_frames(KEY_EDGE_FRAMES)
            .map_err(|err| format!("joystick release port {} {name} failed: {err}", step.port))?;
    }
    Ok(())
}

fn apply_smoke_joystick_axis_steps(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    steps: &[SmokeJoystickAxisStep],
) -> Result<(), String> {
    for step in steps {
        let name = step.axis.name();
        session.queue_input(InputEvent::Axis {
            port: step.port,
            name: name.into(),
            value: step.value,
        });
        session.run_frames(step.frames).map_err(|err| {
            format!(
                "joystick axis port {} {name}={} for {} frames failed: {err}",
                step.port, step.value, step.frames
            )
        })?;
        session.queue_input(InputEvent::Axis {
            port: step.port,
            name: name.into(),
            value: 0,
        });
        session.run_frames(KEY_EDGE_FRAMES).map_err(|err| {
            format!(
                "joystick axis reset port {} {name} failed: {err}",
                step.port
            )
        })?;
    }
    Ok(())
}

fn apply_smoke_joystick_axis_sweeps(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    sweeps: &[SmokeJoystickAxisSweep],
    baseline_frame: &[u8],
    screenshot_stem: Option<&Path>,
    screenshot_format: SmokeScreenshotFormat,
) -> Result<Vec<SmokeJoystickAxisSweepResult>, String> {
    let mut results = Vec::new();
    for sweep in sweeps {
        let name = sweep.axis.name();
        for step in 0..sweep.steps {
            let value = axis_sweep_value(*sweep, step);
            session.queue_input(InputEvent::Axis {
                port: sweep.port,
                name: name.into(),
                value,
            });
            session.run_frames(sweep.frames).map_err(|err| {
                format!(
                    "joystick axis sweep port {} {name}={} for {} frames failed: {err}",
                    sweep.port, value, sweep.frames
                )
            })?;
            let frame = session.screenshot_png_bytes().map_err(|err| {
                format!(
                    "failed to capture joystick axis sweep port {} {name} step {}: {err}",
                    sweep.port, step
                )
            })?;
            let screenshot = write_smoke_screenshot(
                screenshot_stem,
                &format!("joystick-axis-{}-{}-{step:02}", sweep.port, name),
                screenshot_format,
                session,
                &frame,
            )?;
            results.push(SmokeJoystickAxisSweepResult {
                port: sweep.port,
                axis: sweep.axis,
                step,
                value,
                frames: sweep.frames,
                visible_change: frame != baseline_frame,
                screenshot,
            });
        }
        session.queue_input(InputEvent::Axis {
            port: sweep.port,
            name: name.into(),
            value: 0,
        });
        session.run_frames(KEY_EDGE_FRAMES).map_err(|err| {
            format!(
                "joystick axis sweep reset port {} {name} failed: {err}",
                sweep.port
            )
        })?;
    }
    Ok(results)
}

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    threshold: u64,
    max_frames: u32,
) -> Result<u64, String> {
    for _ in 0..=max_frames {
        let position = query_u64(session, "dragon.tape.position_bits")?;
        if position > threshold {
            return Ok(position);
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for tape movement: {err}"))?;
    }
    query_u64(session, "dragon.tape.position_bits")
}

fn wait_for_screenshot_change(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    before: &[u8],
    max_frames: u32,
) -> Result<Option<Vec<u8>>, String> {
    for _ in 0..=max_frames {
        let current = session
            .screenshot_png_bytes()
            .map_err(|err| format!("failed to capture frame: {err}"))?;
        if current != before {
            return Ok(Some(current));
        }
        session
            .run_frames(1)
            .map_err(|err| format!("runtime failed while waiting for frame change: {err}"))?;
    }
    Ok(None)
}

fn screen_text_lines(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<Vec<String>, String> {
    let result = session
        .query("screen.text.lines")
        .map_err(|err| format!("screen.text.lines query failed: {err}"))?;
    result
        .value
        .as_array()
        .ok_or_else(|| "screen.text.lines was not an array".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "screen.text.lines entry was not a string".to_owned())
        })
        .collect()
}

fn query_u64(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u64, String> {
    session
        .query(path)
        .map_err(|err| format!("{path} query failed: {err}"))?
        .value
        .as_u64()
        .ok_or_else(|| format!("{path} query was not an unsigned integer"))
}

fn query_u16(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u16, String> {
    let value = query_u64(session, path)?;
    u16::try_from(value).map_err(|err| format!("{path} query value {value} is not u16: {err}"))
}

fn query_u8(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<u8, String> {
    let value = query_u64(session, path)?;
    u8::try_from(value).map_err(|err| format!("{path} query value {value} is not u8: {err}"))
}

fn query_bool(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    session
        .query(path)
        .map_err(|err| format!("{path} query failed: {err}"))?
        .value
        .as_bool()
        .ok_or_else(|| format!("{path} query was not a boolean"))
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("tape")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn matches_ignore_ascii_case(value: &str, expected: &[&str]) -> bool {
    expected
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

impl From<&CasHeader> for CasHeaderSummary {
    fn from(header: &CasHeader) -> Self {
        Self {
            name: header.name.clone(),
            file_type: cas_file_type_label(header.file_type).to_owned(),
            ascii_flag: header.ascii_flag,
            gap_flag: header.gap_flag,
            first_address: header.first_address,
            second_address: header.second_address,
        }
    }
}

fn cas_file_type_label(file_type: CasFileType) -> &'static str {
    match file_type {
        CasFileType::Basic => "basic",
        CasFileType::Data => "data",
        CasFileType::MachineCode => "machine-code",
        CasFileType::Unknown(_) => "unknown",
    }
}

fn load_rom(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return load_rom_from_zip(path);
    }

    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    exact_rom_from_bytes(path, bytes)
}

fn load_dragon_firmware(cli: &Cli) -> Result<LoadedDragonFirmware, String> {
    if cli.model == Model::Dragon32Pal && cli.mode_rom.is_some() {
        return Err("--rom64 requires --model dragon64".to_owned());
    }

    let mode_rom_path = match cli.model {
        Model::Dragon32Pal => None,
        Model::Dragon64Pal => Some(
            cli.mode_rom
                .as_deref()
                .ok_or_else(|| "--model dragon64 requires --rom64 PATH".to_owned())?,
        ),
    };
    let rom = load_rom(&cli.rom)?;
    let mode_rom = mode_rom_path.map(load_rom).transpose()?;

    Ok(LoadedDragonFirmware {
        model: cli.model,
        rom,
        mode_rom,
    })
}

fn ensure_dragon32_harness(cli: &Cli, feature: &str) -> Result<(), String> {
    if cli.model == Model::Dragon32Pal {
        return Ok(());
    }

    Err(format!(
        "{feature} currently uses the low-level Dragon 32 harness; use --model dragon64 with --smoke-root for runtime-backed CAS smoke"
    ))
}

fn load_rom_from_zip(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    let file =
        fs::File::open(path).map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut candidate = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            format!(
                "failed to read zip entry {index} in {}: {err}",
                path.display()
            )
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("failed to read {entry_name} from {}: {err}", path.display()))?;

        if bytes.len() == ROM_SIZE {
            if candidate.is_some() {
                return Err(format!(
                    "{} contains multiple {ROM_SIZE}-byte ROM candidates",
                    path.display()
                ));
            }
            candidate = Some(
                bytes
                    .try_into()
                    .map_err(|_| format!("{entry_name} was not exactly {ROM_SIZE} bytes"))?,
            );
        }
    }

    candidate.ok_or_else(|| {
        format!(
            "{} did not contain a {ROM_SIZE}-byte Dragon ROM",
            path.display()
        )
    })
}

fn exact_rom_from_bytes(path: &Path, bytes: Vec<u8>) -> Result<[u8; ROM_SIZE], String> {
    let actual_len = bytes.len();
    bytes.try_into().map_err(|_| {
        format!(
            "{} must be exactly {ROM_SIZE} bytes; got {actual_len}",
            path.display()
        )
    })
}

fn load_cartridge(path: &Path) -> Result<DragonPakImage, String> {
    let loaded = read_media_asset(path, MediaKind::Cartridge)
        .map_err(|err| format!("failed to load Dragon cartridge {}: {err}", path.display()))?;
    parse_dragon_pak(&loaded.bytes)
        .map_err(|err| format!("failed to parse Dragon cartridge {}: {err}", path.display()))
}

fn load_disk(path: &Path) -> Result<DragonDiskImage, String> {
    let loaded = read_media_asset(path, MediaKind::Disk)
        .map_err(|err| format!("failed to load Dragon disk {}: {err}", path.display()))?;
    parse_vdk(&loaded.bytes)
        .map_err(|err| format!("failed to parse Dragon disk {}: {err}", path.display()))
}

fn write_exported_disk(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    let bytes = bytes.ok_or_else(|| {
        format!(
            "cannot write {}; no DragonDOS disk is mounted in drive 1",
            path.display()
        )
    })?;
    fs::write(path, bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn load_binary_program(path: &Path) -> Result<DragonBinImage, String> {
    let loaded = read_media_asset(path, MediaKind::Program).map_err(|err| {
        format!(
            "failed to load Dragon binary program {}: {err}",
            path.display()
        )
    })?;
    parse_dragon_bin(&loaded.bytes).map_err(|err| {
        format!(
            "failed to parse Dragon binary program {}: {err}",
            path.display()
        )
    })
}

fn load_snapshot(path: &Path) -> Result<PcDragonSnapshot, String> {
    let loaded = read_media_asset(path, MediaKind::Snapshot)
        .map_err(|err| format!("failed to load Dragon snapshot {}: {err}", path.display()))?;
    parse_pcdragon_snapshot(&loaded.bytes)
        .map_err(|err| format!("failed to parse Dragon snapshot {}: {err}", path.display()))
}

const fn machine_cartridge_kind(kind: ParsedDragonCartridgeKind) -> DragonCartridgeKind {
    match kind {
        ParsedDragonCartridgeKind::Rom => DragonCartridgeKind::Rom,
        ParsedDragonCartridgeKind::GamesMaster => DragonCartridgeKind::GamesMaster,
    }
}

#[derive(Clone, Debug)]
struct HarnessRunOptions<'a> {
    cartridge: Option<&'a DragonPakImage>,
    disk: Option<&'a DragonDiskImage>,
    program: Option<&'a DragonBinImage>,
    snapshot: Option<&'a PcDragonSnapshot>,
    cycle_limit: u64,
    trace_limit: usize,
    fetch_watch: Vec<AddressRange>,
    write_watch: Vec<AddressRange>,
    dump_text: bool,
    dump_ram: bool,
    export_disk: bool,
    dump_text_framebuffer: bool,
    capture_framebuffer: bool,
    capture_framebuffer_phase: SmokeScreenshotPhase,
    capture_framebuffer_source: ScreenshotSource,
}

struct HarnessCaptures {
    text_screen: Option<String>,
    ram: Option<Vec<u8>>,
    text_framebuffer: Option<Vec<u32>>,
    framebuffer: Option<Vec<u32>>,
    framebuffer_cycles: Option<u64>,
    framebuffer_master_ticks: Option<u64>,
    video_phase: DragonVideoPhase,
    disk_vdk: Option<Vec<u8>>,
}

fn run_harness_with_keyboard(
    rom: &[u8; ROM_SIZE],
    keyboard: DragonKeyboard,
    options: HarnessRunOptions<'_>,
) -> HarnessReport {
    let mut machine = Dragon32::new_with_keyboard(rom, keyboard);
    if let Some(cartridge) = options.cartridge {
        machine.load_cartridge(machine_cartridge_kind(cartridge.kind), &cartridge.rom, true);
    }
    if let Some(disk) = options.disk {
        let result = machine.insert_disk(0, disk.clone());
        debug_assert!(result.is_ok());
    }
    if let Some(snapshot) = options.snapshot {
        machine.load_pcdragon_snapshot(
            snapshot.load_address,
            &snapshot.ram,
            machine_dragon_32::DragonSnapshotRegisters {
                pc: snapshot.registers.pc,
                x: snapshot.registers.x,
                y: snapshot.registers.y,
                u: snapshot.registers.u,
                s: snapshot.registers.s,
                dp: snapshot.registers.dp,
                b: snapshot.registers.b,
                a: snapshot.registers.a,
                cc: snapshot.registers.cc,
            },
            snapshot
                .peripherals
                .map(|peripherals| machine_dragon_32::DragonSnapshotPeripherals {
                    ff02: peripherals.ff02,
                    ff03: peripherals.ff03,
                    ff22: peripherals.ff22,
                }),
            snapshot.display_base,
        );
    }
    if let Some(program) = options.program {
        if options.snapshot.is_none() && options.cartridge.is_none() {
            boot_machine_to_basic_idle(&mut machine);
        }
        let result = machine.load_binary_program(
            program.load_address,
            &program.payload,
            program.exec_address,
            true,
        );
        debug_assert!(result.is_ok());
    }
    let mut run_options = RunOptions::new(options.trace_limit);
    run_options.fetch_watch = options.fetch_watch;
    run_options.write_watch = options.write_watch;
    let report = machine.run_cycles_with_options(options.cycle_limit, run_options);
    let mut framebuffer_cycles = None;
    let mut framebuffer_master_ticks = None;
    if options.capture_framebuffer
        && matches!(report.stop_reason, StopReason::CycleLimit)
        && matches!(
            options.capture_framebuffer_phase,
            SmokeScreenshotPhase::CompletedFrame
        )
    {
        run_to_completed_video_frame(&mut machine);
    }
    let text_screen =
        (options.dump_text || options.dump_text_framebuffer).then(|| machine.capture_text_screen());
    let text_screen_text = text_screen
        .as_ref()
        .filter(|_| options.dump_text)
        .map(|screen| screen.to_plain_text());
    let text_framebuffer = options
        .dump_text_framebuffer
        .then(|| machine.render_visible_text_argb(TextPalette::default()));
    let ram = options.dump_ram.then(|| machine.ram().to_vec());
    let disk_vdk = options
        .export_disk
        .then(|| machine.disk_image(0).map(DragonDiskImage::to_vdk_bytes))
        .flatten();
    let framebuffer = options.capture_framebuffer.then(|| {
        framebuffer_cycles = Some(machine.cycles());
        framebuffer_master_ticks = Some(machine.master_ticks());
        match options.capture_framebuffer_source {
            ScreenshotSource::Beam => machine.beam_visible_argb().to_vec(),
            ScreenshotSource::Static => machine.render_visible_argb(VdgPalette::default()),
        }
    });

    report.into_harness_report(HarnessCaptures {
        text_screen: text_screen_text,
        ram,
        text_framebuffer,
        framebuffer,
        framebuffer_cycles,
        framebuffer_master_ticks,
        video_phase: machine.video_phase(),
        disk_vdk,
    })
}

fn boot_machine_to_basic_idle(machine: &mut Dragon32) {
    if screen_has_basic_prompt(machine) {
        return;
    }

    for _ in 0..BOOT_FRAME_BUDGET {
        let report = machine.run_cycles(DRAGON_FRAME_CYCLES, 0);
        if matches!(report.stop_reason, StopReason::CpuHalted) {
            return;
        }
        if screen_has_basic_prompt(machine) {
            let _ = machine.run_cycles(
                DRAGON_FRAME_CYCLES.saturating_mul(DIRECT_PROGRAM_BOOT_SETTLE_FRAMES),
                0,
            );
            return;
        }
    }
}

fn screen_has_basic_prompt(machine: &Dragon32) -> bool {
    machine
        .capture_text_screen()
        .to_plain_text()
        .lines()
        .any(|line| line.trim() == "OK")
}

fn run_to_completed_video_frame(machine: &mut Dragon32) {
    if machine.video_phase().frame_master_tick == 0 {
        return;
    }

    let max_cycles = DRAGON_FRAME_CYCLES.saturating_mul(3);
    for _ in 0..max_cycles {
        let previous = machine.video_phase().frame_master_tick;
        let _ = machine.run_cycles(1, 0);
        let current = machine.video_phase().frame_master_tick;
        if current == 0 || current < previous {
            return;
        }
    }
}

trait IntoHarnessReport {
    fn into_harness_report(self, captures: HarnessCaptures) -> HarnessReport;
}

impl IntoHarnessReport for RunReport {
    fn into_harness_report(self, captures: HarnessCaptures) -> HarnessReport {
        HarnessReport {
            stop_reason: self.stop_reason,
            cycles: self.cycles,
            master_ticks: self.master_ticks,
            instructions: self.instructions,
            pc: self.pc,
            addr: self.addr,
            rw: self.rw,
            last_fetch: self.last_fetch,
            trace: self.trace,
            dropped_trace: self.dropped_trace,
            device_accesses: self.device_accesses,
            dropped_device_accesses: self.dropped_device_accesses,
            readonly_writes: self.readonly_writes,
            dropped_readonly_writes: self.dropped_readonly_writes,
            watched_fetches: self.watched_fetches,
            dropped_watched_fetches: self.dropped_watched_fetches,
            watched_writes: self.watched_writes,
            dropped_watched_writes: self.dropped_watched_writes,
            pia_signals: self.pia_signals,
            dropped_pia_signals: self.dropped_pia_signals,
            interrupt_lines: self.interrupt_lines,
            dropped_interrupt_lines: self.dropped_interrupt_lines,
            interrupt_accepts: self.interrupt_accepts,
            dropped_interrupt_accepts: self.dropped_interrupt_accepts,
            vdg_samples: self.vdg_samples,
            dropped_vdg_samples: self.dropped_vdg_samples,
            vdg_mode_writes: self.vdg_mode_writes,
            dropped_vdg_mode_writes: self.dropped_vdg_mode_writes,
            text_screen_base: self.text_screen_base,
            text_screen: captures.text_screen,
            ram: captures.ram,
            text_framebuffer: captures.text_framebuffer,
            framebuffer: captures.framebuffer,
            framebuffer_cycles: captures.framebuffer_cycles,
            framebuffer_master_ticks: captures.framebuffer_master_ticks,
            video_phase: captures.video_phase,
            disk_vdk: captures.disk_vdk,
        }
    }
}

fn print_report(report: &HarnessReport) {
    println!("dragon harness summary");
    println!("status: {}", format_stop_reason(report.stop_reason));
    println!("cycles: {}", report.cycles);
    println!("master ticks: {}", report.master_ticks);
    println!("instructions: {}", report.instructions);
    println!("pc: ${:04X}", report.pc);
    println!("text screen base: ${:04X}", report.text_screen_base);
    println!(
        "video phase: frame_tick={} physical_line={} line_tick={} visible_line={:?} active_y={:?} active_x={:?}",
        report.video_phase.frame_master_tick,
        report.video_phase.physical_line,
        report.video_phase.line_master_tick,
        report.video_phase.visible_line,
        report.video_phase.active_y,
        report.video_phase.active_x
    );
    println!(
        "bus: addr=${:04X} rw={}",
        report.addr,
        if report.rw { "read" } else { "write" }
    );
    if let Some(fetch) = report.last_fetch {
        println!(
            "last fetch: cycle={} master_tick={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.master_tick, fetch.pc, fetch.opcode
        );
    }
    if report.dropped_trace != 0 {
        println!("trace dropped: {}", report.dropped_trace);
    }
    if report.dropped_device_accesses != 0 {
        println!(
            "device accesses dropped: {}",
            report.dropped_device_accesses
        );
    }
    println!("device accesses:");
    for access in &report.device_accesses {
        println!(
            "  cycle={} {} {} addr=${:04X} value=${:02X}",
            access.cycle,
            if access.rw { "read" } else { "write" },
            format_device_region(access.device),
            access.addr,
            access.value
        );
    }
    if report.dropped_readonly_writes != 0 {
        println!(
            "readonly writes dropped: {}",
            report.dropped_readonly_writes
        );
    }
    println!("readonly writes:");
    for write in &report.readonly_writes {
        println!(
            "  cycle={} addr=${:04X} value=${:02X}",
            write.cycle, write.addr, write.value
        );
    }
    if report.dropped_watched_fetches != 0 {
        println!(
            "watched fetches dropped: {}",
            report.dropped_watched_fetches
        );
    }
    println!("watched fetches:");
    for fetch in &report.watched_fetches {
        println!(
            "  cycle={} master_tick={} pc=${:04X} opcode=${:02X} {}",
            fetch.cycle,
            fetch.master_tick,
            fetch.pc,
            fetch.opcode,
            format_cpu_registers(fetch.regs)
        );
    }
    if report.dropped_watched_writes != 0 {
        println!("watched writes dropped: {}", report.dropped_watched_writes);
    }
    println!("watched writes:");
    for write in &report.watched_writes {
        let instruction_pc = write
            .instruction_pc
            .map_or("????".to_owned(), |pc| format!("{pc:04X}"));
        println!(
            "  cycle={} frame_tick={} line={:?} active_y={:?} active_x={:?} instr=${} addr=${:04X} value=${:02X} {}",
            write.cycle,
            write.frame_master_tick,
            write.line,
            write.active_y,
            write.active_x,
            instruction_pc,
            write.addr,
            write.value,
            format_cpu_registers(write.regs)
        );
    }
    if report.dropped_pia_signals != 0 {
        println!("pia signals dropped: {}", report.dropped_pia_signals);
    }
    println!("pia signals:");
    for signal in &report.pia_signals {
        println!(
            "  cycle={} {} {:?} level={} cra=${:02X} crb=${:02X} irq_a={} irq_b={}",
            signal.cycle,
            format_device_region(signal.device),
            signal.signal,
            signal.level,
            signal.control_a,
            signal.control_b,
            signal.irq_a,
            signal.irq_b
        );
    }
    if report.dropped_interrupt_lines != 0 {
        println!(
            "interrupt lines dropped: {}",
            report.dropped_interrupt_lines
        );
    }
    println!("interrupt lines:");
    for line in &report.interrupt_lines {
        println!(
            "  cycle={} {} level={} pc=${:04X} cc=${:02X}",
            line.cycle,
            format_interrupt_kind(line.kind),
            line.level,
            line.pc,
            line.cc
        );
    }
    if report.dropped_interrupt_accepts != 0 {
        println!(
            "interrupt accepts dropped: {}",
            report.dropped_interrupt_accepts
        );
    }
    println!("interrupt accepts:");
    for accept in &report.interrupt_accepts {
        println!(
            "  cycle={} {} pc=${:04X} cc=${:02X}",
            accept.cycle,
            format_interrupt_kind(accept.kind),
            accept.pc,
            accept.cc
        );
    }
    if report.dropped_vdg_samples != 0 {
        println!("vdg samples dropped: {}", report.dropped_vdg_samples);
    }
    println!("vdg samples:");
    for sample in &report.vdg_samples {
        println!(
            "  cycle={} frame_tick={} fetch_tick={} line={} active_y={} byte={} offset=${:04X} raw=${:02X} base=${:04X} sam_mode=${:02X} sam_offset=${:02X} pb=${:02X} ag={} css={} int_ext={} gm={}",
            sample.cycle,
            sample.frame_master_tick,
            sample.fetch_frame_master_tick,
            sample.line,
            sample.active_y,
            sample.byte_x,
            sample.display_offset,
            sample.raw,
            sample.display_base,
            sample.sam_video_mode,
            sample.sam_display_offset,
            sample.pia1_pb,
            sample.graphics,
            sample.css,
            sample.int_ext,
            sample.gm
        );
    }
    if report.dropped_vdg_mode_writes != 0 {
        println!(
            "vdg mode writes dropped: {}",
            report.dropped_vdg_mode_writes
        );
    }
    println!("vdg mode writes:");
    for write in &report.vdg_mode_writes {
        println!(
            "  cycle={} frame_tick={} line={:?} active_y={:?} active_x={:?} addr=${:04X} value=${:02X} ag={} css={} int_ext={} gm={}",
            write.cycle,
            write.frame_master_tick,
            write.line,
            write.active_y,
            write.active_x,
            write.addr,
            write.value,
            write.graphics,
            write.css,
            write.int_ext,
            write.gm
        );
    }
    if let Some(text_screen) = &report.text_screen {
        println!("text screen:");
        for line in text_screen.lines() {
            println!("  |{line}|");
        }
    }
    if let Some(framebuffer) = &report.text_framebuffer {
        let foreground_pixels = framebuffer
            .iter()
            .filter(|&&pixel| pixel == TextPalette::default().foreground)
            .count();
        println!(
            "text framebuffer: {}x{} foreground-pixels={}",
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, foreground_pixels
        );
    }
    if let Some(framebuffer) = &report.framebuffer {
        println!(
            "framebuffer: {}x{} pixels={}",
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
            TEXT_VISIBLE_FRAMEBUFFER_HEIGHT,
            framebuffer.len()
        );
        if let Some(master_ticks) = report.framebuffer_master_ticks {
            println!("framebuffer master ticks: {master_ticks}");
        }
    }
    println!("trace:");
    for fetch in &report.trace {
        println!(
            "  cycle={} master_tick={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.master_tick, fetch.pc, fetch.opcode
        );
    }
}

fn format_interrupt_kind(kind: CpuInterruptKind) -> &'static str {
    match kind {
        CpuInterruptKind::Nmi => "nmi",
        CpuInterruptKind::Irq => "irq",
        CpuInterruptKind::Firq => "firq",
    }
}

fn format_cpu_registers(regs: CpuRegisterTrace) -> String {
    format!(
        "pc=${:04X} a=${:02X} b=${:02X} dp=${:02X} x=${:04X} y=${:04X} u=${:04X} s=${:04X} cc=${:02X}",
        regs.pc, regs.a, regs.b, regs.dp, regs.x, regs.y, regs.u, regs.s, regs.cc
    )
}

fn write_text_png(path: &Path, framebuffer: &[u32]) -> Result<(), String> {
    if framebuffer.len() != TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT {
        return Err(format!(
            "text framebuffer has {} pixels; expected {}",
            framebuffer.len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        ));
    }

    let file = fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(
        writer,
        TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
        TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|err| format!("failed to write PNG header for {}: {err}", path.display()))?;

    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &argb in framebuffer {
        rgba.push(((argb >> 16) & 0xFF) as u8);
        rgba.push(((argb >> 8) & 0xFF) as u8);
        rgba.push((argb & 0xFF) as u8);
        rgba.push(((argb >> 24) & 0xFF) as u8);
    }
    png_writer
        .write_image_data(&rgba)
        .map_err(|err| format!("failed to write PNG data for {}: {err}", path.display()))?;
    png_writer
        .finish()
        .map_err(|err| format!("failed to finish PNG {}: {err}", path.display()))?;
    Ok(())
}

fn write_screenshot_png(
    path: &Path,
    framebuffer: &[u32],
    format: SmokeScreenshotFormat,
    cycles: u64,
) -> Result<(), String> {
    let rgba = argb_to_rgba(framebuffer)?;
    let png = match format {
        SmokeScreenshotFormat::Diagnostic => encode_rgba_png(
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            &rgba,
        )?,
        SmokeScreenshotFormat::XroarZoomed => {
            let frame = CapturedFrame {
                timestamp: MachineTime::new(cycles),
                format: PixelFormat::Rgba8888,
                width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
                height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
                palette: None,
                pixels: rgba,
            };
            xroar_zoomed_png_bytes(&frame)?
        }
    };
    fs::write(path, png).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn argb_to_rgba(framebuffer: &[u32]) -> Result<Vec<u8>, String> {
    if framebuffer.len() != TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT {
        return Err(format!(
            "framebuffer has {} pixels; expected {}",
            framebuffer.len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        ));
    }

    let mut rgba = Vec::with_capacity(framebuffer.len() * 4);
    for &argb in framebuffer {
        rgba.push(((argb >> 16) & 0xFF) as u8);
        rgba.push(((argb >> 8) & 0xFF) as u8);
        rgba.push((argb & 0xFF) as u8);
        rgba.push(((argb >> 24) & 0xFF) as u8);
    }
    Ok(rgba)
}

fn format_stop_reason(reason: StopReason) -> String {
    match reason {
        StopReason::CycleLimit => "cycle-limit".to_owned(),
        StopReason::CpuHalted => "cpu-halted".to_owned(),
    }
}

fn format_device_region(device: DeviceRegion) -> &'static str {
    match device {
        DeviceRegion::Pia0 => "pia0",
        DeviceRegion::Pia1 => "pia1",
        DeviceRegion::Sam => "sam",
        DeviceRegion::Acia => "acia",
        DeviceRegion::Cartridge => "cartridge",
        DeviceRegion::DiskController => "disk-controller",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn harness_can_dump_sam_selected_text_screen() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$01
        rom[0x0001] = 0x01;
        rom[0x0002] = 0xB7; // STA $FFC9: set SAM F1, selecting text base $0400.
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0xC9;
        rom[0x0005] = 0xB7; // STA $0400: MC6847 diagnostic 'A'.
        rom[0x0006] = 0x04;
        rom[0x0007] = 0x00;
        rom[0x0008] = 0x86; // LDA #$02
        rom[0x0009] = 0x02;
        rom[0x000A] = 0xB7; // STA $0401: MC6847 diagnostic 'B'.
        rom[0x000B] = 0x04;
        rom[0x000C] = 0x01;
        rom[0x000D] = 0x01;

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                disk: None,
                program: None,
                snapshot: None,
                cycle_limit: 128,
                trace_limit: 8,
                fetch_watch: Vec::new(),
                write_watch: Vec::new(),
                dump_text: true,
                dump_ram: false,
                export_disk: false,
                dump_text_framebuffer: true,
                capture_framebuffer: true,
                capture_framebuffer_phase: SmokeScreenshotPhase::Immediate,
                capture_framebuffer_source: ScreenshotSource::Beam,
            },
        );

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.text_screen_base, 0x0400);
        assert_eq!(
            report
                .text_screen
                .as_deref()
                .expect("text dump should be captured")
                .lines()
                .next(),
            Some("AB@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@")
        );
        assert_eq!(
            report
                .text_framebuffer
                .as_ref()
                .expect("text framebuffer should be captured")
                .len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        );
        assert_eq!(
            report
                .framebuffer
                .as_ref()
                .expect("framebuffer should be captured")
                .len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        );
    }

    #[test]
    fn completed_frame_screenshot_phase_advances_to_video_frame_boundary() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x20; // BRA -2.
        rom[0x0001] = 0xFE;

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                disk: None,
                program: None,
                snapshot: None,
                cycle_limit: 1,
                trace_limit: 0,
                fetch_watch: Vec::new(),
                write_watch: Vec::new(),
                dump_text: false,
                dump_ram: false,
                export_disk: false,
                dump_text_framebuffer: false,
                capture_framebuffer: true,
                capture_framebuffer_phase: SmokeScreenshotPhase::CompletedFrame,
                capture_framebuffer_source: ScreenshotSource::Beam,
            },
        );

        assert_eq!(report.stop_reason, StopReason::CycleLimit);
        assert_eq!(report.cycles, 1);
        assert_eq!(report.framebuffer_cycles, Some(DRAGON_FRAME_CYCLES));
        assert_eq!(report.video_phase.frame_master_tick, 0);
    }

    #[test]
    fn completed_frame_screenshot_phase_uses_actual_video_phase_for_fast_sam_cycles() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$00.
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xB7; // STA $FFD9: set SAM R1, selecting fast CPU cycles.
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0xD9;
        rom[0x0005] = 0x20; // BRA -2.
        rom[0x0006] = 0xFE;

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                disk: None,
                program: None,
                snapshot: None,
                cycle_limit: 16,
                trace_limit: 0,
                fetch_watch: Vec::new(),
                write_watch: Vec::new(),
                dump_text: false,
                dump_ram: false,
                export_disk: false,
                dump_text_framebuffer: false,
                capture_framebuffer: true,
                capture_framebuffer_phase: SmokeScreenshotPhase::CompletedFrame,
                capture_framebuffer_source: ScreenshotSource::Beam,
            },
        );

        let framebuffer_cycles = report
            .framebuffer_cycles
            .expect("completed-frame capture should report its cycle");
        assert_eq!(report.stop_reason, StopReason::CycleLimit);
        assert!(
            report.video_phase.frame_master_tick < MAX_SAM_BUS_CYCLE_MASTER_TICKS,
            "completed-frame capture should stop at the first bus-cycle boundary after frame wrap, got frame tick {}",
            report.video_phase.frame_master_tick
        );
        assert_ne!(
            framebuffer_cycles % DRAGON_FRAME_CYCLES,
            0,
            "fast SAM timing must not use nominal slow-cycle frame modulo"
        );
    }

    #[test]
    fn harness_exports_mounted_disk_as_vdk() {
        let rom = rom_with_reset_vector(0x8000);
        let mut disk_bytes = vec![0; 12 + 40 * 18 * 256];
        disk_bytes[0] = b'd';
        disk_bytes[1] = b'k';
        disk_bytes[2] = 12;
        disk_bytes[8] = 40;
        disk_bytes[9] = 1;
        disk_bytes[12] = 0x5a;
        let disk = parse_vdk(&disk_bytes).expect("test VDK should parse");

        let report = run_harness_with_keyboard(
            &rom,
            DragonKeyboard::new(),
            HarnessRunOptions {
                cartridge: None,
                disk: Some(&disk),
                program: None,
                snapshot: None,
                cycle_limit: 0,
                trace_limit: 0,
                fetch_watch: Vec::new(),
                write_watch: Vec::new(),
                dump_text: false,
                dump_ram: false,
                export_disk: true,
                dump_text_framebuffer: false,
                capture_framebuffer: false,
                capture_framebuffer_phase: SmokeScreenshotPhase::Immediate,
                capture_framebuffer_source: ScreenshotSource::Beam,
            },
        );
        let exported = report.disk_vdk.expect("disk export should be captured");
        let reparsed = parse_vdk(&exported).expect("exported VDK should parse");

        assert_eq!(reparsed.sector(0, 0, 1).expect("sector 1")[0], 0x5a);
    }

    #[test]
    fn cli_requires_rom_path() {
        let err = parse_cli(Vec::<String>::new()).expect_err("missing ROM should fail");

        assert!(err.contains("missing required --rom"));
    }

    #[test]
    fn cli_parses_dragon64_firmware_paths() {
        let cli = parse_cli([
            "--model".to_owned(),
            "dragon64".to_owned(),
            "--rom".to_owned(),
            "dragon64-compat.rom".to_owned(),
            "--rom64".to_owned(),
            "dragon64.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
        ])
        .expect("Dragon 64 CLI should parse");

        assert_eq!(cli.model, Model::Dragon64Pal);
        assert_eq!(cli.rom, PathBuf::from("dragon64-compat.rom"));
        assert_eq!(cli.mode_rom, Some(PathBuf::from("dragon64.rom")));
        assert_eq!(cli.smoke_root, Some(PathBuf::from("tapes")));
    }

    #[test]
    fn dragon64_firmware_requires_mode_rom() {
        let cli = parse_cli([
            "--model".to_owned(),
            "dragon64".to_owned(),
            "--rom".to_owned(),
            "dragon64-compat.rom".to_owned(),
        ])
        .expect("CLI parsing should not require ROM files");

        let err = match load_dragon_firmware(&cli) {
            Ok(_) => panic!("missing --rom64 should fail"),
            Err(err) => err,
        };
        assert!(err.contains("--model dragon64 requires --rom64"));
    }

    #[test]
    fn dragon32_rejects_mode_rom_argument() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--rom64".to_owned(),
            "dragon64.rom".to_owned(),
        ])
        .expect("CLI parsing should accept syntax before firmware validation");

        let err = match load_dragon_firmware(&cli) {
            Ok(_) => panic!("Dragon 32 should reject --rom64"),
            Err(err) => err,
        };
        assert!(err.contains("--rom64 requires --model dragon64"));
    }

    #[test]
    fn load_dragon_firmware_accepts_dragon64_pair() {
        let compat_rom = rom_with_reset_vector(0x8000);
        let mut mode_rom = rom_with_reset_vector(0x8000);
        mode_rom[0] = 0x12;
        let stem = format!("emu198x-dragon64-firmware-test-{}", std::process::id());
        let compat_path = env::temp_dir().join(format!("{stem}-compat.rom"));
        let mode_path = env::temp_dir().join(format!("{stem}-mode.rom"));
        fs::write(&compat_path, compat_rom).expect("compatible ROM fixture should be writable");
        fs::write(&mode_path, mode_rom).expect("mode ROM fixture should be writable");

        let cli = parse_cli([
            "--model".to_owned(),
            "dragon64".to_owned(),
            "--rom".to_owned(),
            compat_path.display().to_string(),
            "--rom64".to_owned(),
            mode_path.display().to_string(),
        ])
        .expect("Dragon 64 CLI should parse");
        let firmware = load_dragon_firmware(&cli).expect("Dragon 64 firmware should load");

        fs::remove_file(&compat_path).expect("compatible ROM fixture should be removable");
        fs::remove_file(&mode_path).expect("mode ROM fixture should be removable");
        assert_eq!(firmware.model, Model::Dragon64Pal);
        assert_eq!(firmware.rom, compat_rom);
        assert_eq!(firmware.mode_rom, Some(mode_rom));
    }

    #[test]
    fn cli_parses_hex_cycles_and_trace_limit() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cycles".to_owned(),
            "0x20".to_owned(),
            "--trace-limit".to_owned(),
            "3".to_owned(),
            "--dump-ram".to_owned(),
            "ram.bin".to_owned(),
            "--dump-text".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.rom, PathBuf::from("dragon32.rom"));
        assert_eq!(cli.model, Model::Dragon32Pal);
        assert_eq!(cli.mode_rom, None);
        assert_eq!(cli.cycles, 32);
        assert_eq!(cli.trace_limit, 3);
        assert!(cli.fetch_watch.is_empty());
        assert!(cli.write_watch.is_empty());
        assert_eq!(cli.pressed_keys, Vec::new());
        assert_eq!(cli.dump_ram, Some(PathBuf::from("ram.bin")));
        assert!(cli.dump_text);
    }

    #[test]
    fn cli_parses_typed_command() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--type-command".to_owned(),
            "DIR".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.type_command, Some("DIR".to_owned()));
    }

    #[test]
    fn typed_command_maps_basic_quotes_to_shifted_digit_two() {
        assert_eq!(dragon_basic_key_name('S'), "s");
        assert_eq!(dragon_basic_key_name(' '), "space");
        assert_eq!(dragon_basic_key_combo('"'), Some(&["shift", "2"][..]));
    }

    #[test]
    fn typed_command_cycle_budget_rounds_up_to_frames() {
        assert_eq!(frames_for_cycles(1), 1);
        assert_eq!(frames_for_cycles(DRAGON_FRAME_CYCLES), 1);
        assert_eq!(frames_for_cycles(DRAGON_FRAME_CYCLES + 1), 2);
    }

    #[test]
    fn dragon_dos_dir_command_lists_vdk_directory() {
        let Some(rom_path) = test_required_env_path("EMU198X_DRAGON32_ROM") else {
            return;
        };
        let Some(dos_rom_path) = test_required_env_path("EMU198X_DRAGON_DOS_ROM") else {
            return;
        };
        let Some(disk_path) = test_required_env_path("EMU198X_DRAGON_DOS_DIR_VDK") else {
            return;
        };

        let cli = parse_cli([
            "--rom".to_owned(),
            rom_path.display().to_string(),
            "--cart".to_owned(),
            dos_rom_path.display().to_string(),
            "--disk".to_owned(),
            disk_path.display().to_string(),
            "--type-command".to_owned(),
            "DIR".to_owned(),
            "--cycles".to_owned(),
            "3000000".to_owned(),
        ])
        .expect("valid DragonDOS DIR CLI should parse");
        let firmware = load_dragon_firmware(&cli).unwrap_or_else(|err| {
            panic!("load Dragon 32 firmware from {}: {err}", rom_path.display())
        });

        let report =
            run_typed_command(&cli, &firmware, "DIR").expect("DragonDOS DIR should complete");

        assert_screen_contains(&report.screen_text, "DRAGONDOS 1.0");
        assert_screen_contains(&report.screen_text, "DOCTOR  .BAS  270");
        assert_screen_contains(&report.screen_text, "LINK    .BIN  94");
        assert_screen_contains(&report.screen_text, "FILE    .BIN  18");
        assert_screen_contains(&report.screen_text, "PROGLINK.BIN  21");
        assert_screen_contains(&report.screen_text, "VERIFY  .BAS  177");
        assert_screen_contains(&report.screen_text, "76800 FREE BYTES");
        assert_ne!(
            report.pc, 0xC7B5,
            "DragonDOS DIR should not end in the previous FIRQ storm path"
        );
        assert!(
            report
                .disk_traces
                .iter()
                .any(|trace| trace.contains(r#""device":"DiskController""#)),
            "DragonDOS DIR should exercise the FDC register path; screen:\n{}",
            report.screen_text.join("\n")
        );
    }

    #[test]
    fn dragon_dos_save_command_exports_persisted_vdk_entry() {
        let Some(rom_path) = test_required_env_path("EMU198X_DRAGON32_ROM") else {
            return;
        };
        let Some(dos_rom_path) = test_required_env_path("EMU198X_DRAGON_DOS_ROM") else {
            return;
        };
        let Some(disk_path) = test_required_env_path("EMU198X_DRAGON_DOS_SAVE_VDK") else {
            return;
        };

        let cli = parse_cli([
            "--rom".to_owned(),
            rom_path.display().to_string(),
            "--cart".to_owned(),
            dos_rom_path.display().to_string(),
            "--disk".to_owned(),
            disk_path.display().to_string(),
            "--type-command".to_owned(),
            "SAVE\"CODX\"".to_owned(),
            "--cycles".to_owned(),
            "5000000".to_owned(),
            "--disk-output".to_owned(),
            "saved.vdk".to_owned(),
        ])
        .expect("valid DragonDOS SAVE CLI should parse");
        let firmware = load_dragon_firmware(&cli).unwrap_or_else(|err| {
            panic!("load Dragon 32 firmware from {}: {err}", rom_path.display())
        });

        let report = run_typed_command(&cli, &firmware, "SAVE\"CODX\"")
            .expect("DragonDOS SAVE should complete");

        assert_screen_contains(&report.screen_text, "DRAGONDOS 1.0");
        assert_screen_contains(&report.screen_text, "SAVE\"CODX\"");
        assert!(
            report
                .screen_text
                .iter()
                .any(|line| line.trim_end() == "OK"),
            "DragonDOS SAVE should return to OK prompt; screen:\n{}",
            report.screen_text.join("\n")
        );
        assert!(
            !report.screen_text.iter().any(|line| line.contains("ERROR")),
            "DragonDOS SAVE should not report an error; screen:\n{}",
            report.screen_text.join("\n")
        );
        assert!(
            report
                .disk_traces
                .iter()
                .any(|trace| trace.contains(r#""rw":"write""#)),
            "DragonDOS SAVE should exercise disk-controller writes; screen:\n{}",
            report.screen_text.join("\n")
        );
        let exported = report
            .disk_vdk
            .as_deref()
            .expect("DragonDOS SAVE should capture an exported VDK image");
        let reparsed = parse_vdk(exported).expect("exported DragonDOS VDK should reparse");
        assert!(
            reparsed.contains_directory_entry(b"CODX", b"BAS"),
            "exported VDK should contain CODX.BAS in a DragonDOS directory entry"
        );
    }

    #[test]
    fn cli_parses_write_watch_range() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--watch-fetch".to_owned(),
            "0x1c00".to_owned(),
            "--watch-write".to_owned(),
            "0x2c00-0x2cff".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.fetch_watch, vec![AddressRange::new(0x1C00, 0x1C00)]);
        assert_eq!(cli.write_watch, vec![AddressRange::new(0x2C00, 0x2CFF)]);
    }

    #[test]
    fn cli_accumulates_repeated_watch_ranges() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--watch-write".to_owned(),
            "0x88-0x89".to_owned(),
            "--watch-write".to_owned(),
            "0x1fed".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.write_watch,
            vec![
                AddressRange::new(0x0088, 0x0089),
                AddressRange::new(0x1FED, 0x1FED)
            ]
        );
    }

    #[test]
    fn cli_parses_cartridge_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cart".to_owned(),
            "game.dgn".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.cart, Some(PathBuf::from("game.dgn")));
    }

    #[test]
    fn cli_parses_disk_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--disk".to_owned(),
            "game.vdk".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.disk, Some(PathBuf::from("game.vdk")));
    }

    #[test]
    fn cli_parses_disk_output_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--disk-output".to_owned(),
            "saved.vdk".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.disk_output, Some(PathBuf::from("saved.vdk")));
    }

    #[test]
    fn cli_parses_binary_program_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--bin".to_owned(),
            "game.bin".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.bin, Some(PathBuf::from("game.bin")));
    }

    #[test]
    fn cli_parses_snapshot_path() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot".to_owned(),
            "game.pak".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.snapshot, Some(PathBuf::from("game.pak")));
    }

    #[test]
    fn cli_parses_direct_screenshot_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--screenshot".to_owned(),
            "screen.png".to_owned(),
            "--screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
            "--screenshot-source".to_owned(),
            "static".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.screenshot, Some(PathBuf::from("screen.png")));
        assert_eq!(cli.screenshot_format, SmokeScreenshotFormat::XroarZoomed);
        assert_eq!(cli.screenshot_phase, SmokeScreenshotPhase::CompletedFrame);
        assert_eq!(cli.screenshot_source, ScreenshotSource::Static);
    }

    #[test]
    fn cli_parses_raw_matrix_key_presses() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press-matrix".to_owned(),
            "2,3".to_owned(),
            "--press-matrix".to_owned(),
            "4,5".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![MatrixKey::new(2, 3), MatrixKey::new(4, 5),]
        );
    }

    #[test]
    fn cli_parses_smoke_matrix_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--smoke-run-limit".to_owned(),
            "3".to_owned(),
            "--smoke-report".to_owned(),
            "report.json".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--smoke-screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--smoke-audio-dir".to_owned(),
            "audio".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,fire,20".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,right,30".to_owned(),
            "--smoke-joystick-axis".to_owned(),
            "1,x,-0.5,40".to_owned(),
            "--smoke-joystick-axis-sweep".to_owned(),
            "2,y,-1.0,1.0,3,12".to_owned(),
            "--smoke-idle-after-start".to_owned(),
            "492".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.smoke_root, Some(PathBuf::from("tapes")));
        assert_eq!(cli.smoke_run_limit, 3);
        assert_eq!(cli.smoke_report, Some(PathBuf::from("report.json")));
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(cli.smoke_audio_dir, Some(PathBuf::from("audio")));
        assert_eq!(cli.smoke_idle_after_start, 492);
        assert_eq!(
            cli.smoke_joystick,
            vec![
                SmokeJoystickStep {
                    port: 1,
                    control: SmokeJoystickControl::Fire,
                    frames: 20,
                },
                SmokeJoystickStep {
                    port: 1,
                    control: SmokeJoystickControl::Right,
                    frames: 30,
                },
            ]
        );
        assert_eq!(
            cli.smoke_joystick_axis,
            vec![SmokeJoystickAxisStep {
                port: 1,
                axis: SmokeJoystickAxis::X,
                value: -16_383,
                frames: 40,
            }]
        );
        assert_eq!(
            cli.smoke_joystick_axis_sweep,
            vec![SmokeJoystickAxisSweep {
                port: 2,
                axis: SmokeJoystickAxis::Y,
                start: i16::MIN,
                end: i16::MAX,
                steps: 3,
                frames: 12,
            }]
        );
        assert_eq!(
            cli.smoke_screenshot_format,
            SmokeScreenshotFormat::XroarZoomed
        );
    }

    #[test]
    fn cli_parses_snapshot_smoke_root() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--snapshot-smoke-root".to_owned(),
            "paks".to_owned(),
            "--smoke-run-limit".to_owned(),
            "5".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--smoke-screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.snapshot_smoke_root, Some(PathBuf::from("paks")));
        assert_eq!(cli.smoke_run_limit, 5);
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(
            cli.smoke_screenshot_format,
            SmokeScreenshotFormat::XroarZoomed
        );
        assert_eq!(cli.screenshot_phase, SmokeScreenshotPhase::CompletedFrame);
    }

    #[test]
    fn cli_parses_bin_smoke_root() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--bin-smoke-root".to_owned(),
            "bins".to_owned(),
            "--smoke-run-limit".to_owned(),
            "4".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.bin_smoke_root, Some(PathBuf::from("bins")));
        assert_eq!(cli.smoke_run_limit, 4);
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(cli.screenshot_phase, SmokeScreenshotPhase::CompletedFrame);
    }

    #[test]
    fn cli_parses_disk_smoke_root() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cart".to_owned(),
            "dragondos.rom".to_owned(),
            "--disk-smoke-root".to_owned(),
            "disks".to_owned(),
            "--disk-smoke-launch".to_owned(),
            "--smoke-run-limit".to_owned(),
            "2".to_owned(),
            "--smoke-screenshot-dir".to_owned(),
            "screens".to_owned(),
            "--smoke-screenshot-format".to_owned(),
            "xroar-zoomed".to_owned(),
        ])
        .expect("valid disk smoke CLI should parse");

        assert_eq!(cli.cart, Some(PathBuf::from("dragondos.rom")));
        assert_eq!(cli.disk_smoke_root, Some(PathBuf::from("disks")));
        assert!(cli.disk_smoke_launch);
        assert_eq!(cli.smoke_run_limit, 2);
        assert_eq!(cli.smoke_screenshot_dir, Some(PathBuf::from("screens")));
        assert_eq!(
            cli.smoke_screenshot_format,
            SmokeScreenshotFormat::XroarZoomed
        );
    }

    #[test]
    fn disk_directory_counter_detects_known_entry_layouts() {
        let mut disk_bytes = vec![0; 12 + 40 * 18 * 256];
        disk_bytes[0] = b'd';
        disk_bytes[1] = b'k';
        disk_bytes[2] = 12;
        disk_bytes[8] = 40;
        disk_bytes[9] = 1;
        disk_bytes[12..20].copy_from_slice(b"ZERO    ");
        disk_bytes[20..23].copy_from_slice(b"BAS");
        disk_bytes[23] = 0x01;
        disk_bytes[12 + 1 + 25..12 + 1 + 25 + 8].copy_from_slice(b"ONE     ");
        disk_bytes[12 + 1 + 25 + 8..12 + 1 + 25 + 11].copy_from_slice(b"BIN");
        disk_bytes[12 + 1 + 25 + 11] = 0x01;
        let disk = parse_vdk(&disk_bytes).expect("test VDK should parse");
        let entries = dragon_dos_directory_entries(&disk);

        assert_eq!(entries.len(), 2);
        assert_eq!(
            choose_dragon_dos_launch_command(Path::new("Zero Program.vdk"), &entries),
            Some("RUN\"ZERO\"".to_owned())
        );
        assert_eq!(
            choose_dragon_dos_launch_command(Path::new("One Program.vdk"), &entries[1..]),
            Some("LOAD\"ONE.BIN\":EXEC".to_owned())
        );
    }

    #[test]
    fn disk_launch_prefers_title_matched_binary_over_utility_basic() {
        let entries = vec![
            DragonDosDirectoryEntrySummary {
                name: "ICONDRAW".to_owned(),
                extension: "BAS".to_owned(),
            },
            DragonDosDirectoryEntrySummary {
                name: "CWALKER".to_owned(),
                extension: "BIN".to_owned(),
            },
            DragonDosDirectoryEntrySummary {
                name: "DUNGEON".to_owned(),
                extension: "BIN".to_owned(),
            },
        ];

        assert_eq!(
            choose_dragon_dos_launch_command(
                Path::new("Cuthbert Goes Walkabout (1984)(Microdeal).zip"),
                &entries
            ),
            Some("LOAD\"CWALKER.BIN\":EXEC".to_owned())
        );
        assert_eq!(
            choose_dragon_dos_launch_command(
                Path::new("Dungeon Raid (1984)(Microdeal).zip"),
                &entries
            ),
            Some("LOAD\"DUNGEON.BIN\":EXEC".to_owned())
        );
    }

    #[test]
    fn bin_smoke_matrix_runs_synthetic_program_when_dragon_rom_available() {
        let Some(rom_path) = test_dragon32_rom_path() else {
            eprintln!("skipping Dragon BIN smoke regression: set EMU198X_DRAGON32_ROM");
            return;
        };
        let rom = load_rom(&rom_path)
            .unwrap_or_else(|err| panic!("read Dragon 32 ROM at {}: {err}", rom_path.display()));
        let root = temp_test_dir("emu198x-dragon-bin-smoke");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("create test dir {}: {err}", root.display()));
        let bin_path = root.join("visible.bin");
        fs::write(&bin_path, synthetic_visible_bin())
            .unwrap_or_else(|err| panic!("write synthetic BIN {}: {err}", bin_path.display()));

        let cli = parse_cli([
            "--rom".to_owned(),
            rom_path.display().to_string(),
            "--bin-smoke-root".to_owned(),
            root.display().to_string(),
            "--smoke-run-limit".to_owned(),
            "1".to_owned(),
            "--cycles".to_owned(),
            "20000".to_owned(),
            "--screenshot-phase".to_owned(),
            "completed-frame".to_owned(),
        ])
        .expect("valid BIN smoke CLI should parse");

        let firmware = LoadedDragonFirmware {
            model: Model::Dragon32Pal,
            rom,
            mode_rom: None,
        };
        let report =
            run_bin_smoke_matrix(&cli, &firmware).expect("synthetic BIN smoke matrix should run");
        let _ = fs::remove_dir_all(&root);

        assert_eq!(report.program_count, 1);
        assert_eq!(report.runtime_smokes, 1);
        let row = report.rows.first().expect("one BIN row should be present");
        assert_eq!(row.parse_status, "ok");
        assert_eq!(row.load_address, Some(0x2800));
        assert_eq!(row.exec_address, Some(0x2800));
        assert_eq!(row.len, Some(100));
        let runtime = row.runtime.as_ref().expect("runtime smoke should run");
        assert_eq!(
            runtime.classification,
            SnapshotSmokeClassification::RunningVisible
        );
        assert_eq!(runtime.stop_reason, "cycle-limit");
        assert!(
            runtime.non_background_pixels > 0,
            "synthetic program should write visible text pixels"
        );
        assert!(runtime.error.is_none());
    }

    #[test]
    fn cli_rejects_mixed_smoke_roots() {
        let err = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--bin-smoke-root".to_owned(),
            "bins".to_owned(),
            "--snapshot-smoke-root".to_owned(),
            "paks".to_owned(),
            "--disk-smoke-root".to_owned(),
            "disks".to_owned(),
        ])
        .expect_err("mixed smoke roots should fail");

        assert!(err.contains("cannot be combined"));
    }

    #[test]
    fn snapshot_smoke_classification_uses_stop_and_visible_pixels() {
        assert_eq!(
            classify_snapshot_smoke(StopReason::CycleLimit, 2, 1),
            SnapshotSmokeClassification::RunningVisible
        );
        assert_eq!(
            classify_snapshot_smoke(StopReason::CycleLimit, 1, 0),
            SnapshotSmokeClassification::RunningBlank
        );
        assert_eq!(
            classify_snapshot_smoke(StopReason::CpuHalted, 2, 1),
            SnapshotSmokeClassification::HaltedVisible
        );
        assert_eq!(
            framebuffer_stats(&[0xff00_0000, 0xff00_0000, 0xffff_ffff]),
            (2, 1)
        );
    }

    #[test]
    fn pcdragon_snapshot_can_be_converted_to_xroar_v1_snapshot() {
        let snapshot = PcDragonSnapshot {
            ram: vec![0xaa, 0xbb].into_boxed_slice(),
            load_address: 0x2000,
            registers: format_dragon_pak::PcDragonRegisters {
                pc: 0x1234,
                x: 0x5678,
                y: 0x9abc,
                u: 0xdef0,
                s: 0x2468,
                dp: 0x12,
                b: 0x34,
                a: 0x56,
                cc: 0x87,
            },
            peripherals: Some(PcDragonPeripherals {
                ff02: 0xde,
                ff03: 0xb5,
                ff22: 0xfc,
            }),
            display_base: Some(0x0600),
        };

        let bytes = xroar_v1_snapshot_bytes(&snapshot);

        assert!(bytes.starts_with(b"XRoar snapshot.\n\0"));
        assert_eq!(xroar_v1_chunk(&bytes, 3), Some(&[0x40, 0x1e][..]));
        assert_eq!(
            xroar_v1_chunk(&bytes, 4).map(|chunk| &chunk[..14]),
            Some(
                &[
                    0x87, 0x56, 0x34, 0x12, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x24, 0x68, 0x12,
                    0x34,
                ][..]
            )
        );
        let ram = xroar_v1_chunk(&bytes, 1).expect("RAM chunk should exist");
        assert_eq!(ram[0x2000], 0xaa);
        assert_eq!(ram[0x2001], 0xbb);
    }

    #[test]
    fn xroar_sam_video_mode_uses_sam_counter_modes_not_vdg_gm_bits() {
        assert_eq!(xroar_snapshot_sam_video_mode(0x00), 0);
        assert_eq!(xroar_snapshot_sam_video_mode(0x80), 1);
        assert_eq!(xroar_snapshot_sam_video_mode(0x90), 1);
        assert_eq!(xroar_snapshot_sam_video_mode(0xa0), 2);
        assert_eq!(xroar_snapshot_sam_video_mode(0xb0), 3);
        assert_eq!(xroar_snapshot_sam_video_mode(0xc0), 4);
        assert_eq!(xroar_snapshot_sam_video_mode(0xd0), 5);
        assert_eq!(xroar_snapshot_sam_video_mode(0xe0), 6);
        assert_eq!(xroar_snapshot_sam_video_mode(0xf0), 6);
    }

    #[test]
    fn xroar_v2_vuint_round_trips_boundary_values() {
        for value in [
            0,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0x1f_ffff,
            0x20_0000,
            0x0fff_ffff,
            0x1000_0000,
            0x7fff_ffff,
        ] {
            let encoded = xroar_v2_vuint(value);
            assert_eq!(
                xroar_v2_read_vuint(&encoded, 0),
                Some((value, encoded.len()))
            );
        }
    }

    #[test]
    fn xroar_v2_vuint_fields_can_be_patched_and_upserted() {
        let mut bytes = xroar_v2_vuint(1);
        bytes.extend_from_slice(&xroar_v2_vuint(1));
        bytes.extend_from_slice(&xroar_v2_vuint(5));
        let mut end = bytes.len();

        patch_xroar_v2_vuint_field_in_range(&mut bytes, 0, &mut end, 1, 0x4000)
            .expect("existing field should be patchable");
        let field = xroar_v2_find_tag_payload(&bytes, 0, end, 1).expect("field should remain");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((0x4000, field.payload_end))
        );

        upsert_xroar_v2_vuint_field_in_range(&mut bytes, 0, &mut end, 2, 7)
            .expect("missing field should be inserted");
        let field = xroar_v2_find_tag_payload(&bytes, 0, end, 2).expect("field should be added");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((7, field.payload_end))
        );
    }

    #[test]
    fn xroar_v2_component_range_stops_at_structural_close_tag() {
        let mut bytes = xroar_v2_component_marker(b"SAM");
        push_xroar_v2_test_field(&mut bytes, 5, 0x1234);
        bytes.push(0);
        let vdrive_start = bytes.len();
        bytes.extend_from_slice(&xroar_v2_component_marker(b"vdrive"));
        push_xroar_v2_test_field(&mut bytes, 5, 0x56);
        bytes.push(0);

        let range = xroar_v2_component_range(&bytes, b"SAM", None)
            .expect("SAM component should be structurally bounded");
        assert_eq!(range.end, vdrive_start);

        let field = xroar_v2_find_tag_payload(&bytes, range.start, range.end, 5)
            .expect("SAM tag 5 should remain findable");
        assert_eq!(
            xroar_v2_read_vuint(&bytes, field.payload_start),
            Some((0x1234, field.payload_end))
        );
    }

    #[test]
    fn xroar_v2_pia_side_a_is_patched_to_inactive_input_state() {
        let mut bytes = Vec::new();
        for tag in [1, 2, 3, 5, 13, 6, 8, 9, 10, 11] {
            push_xroar_v2_test_field(&mut bytes, tag, 1);
        }
        let mut end = bytes.len();

        patch_xroar_v2_pia_side_a(&mut bytes, 0, &mut end).expect("PIA side A should be patchable");

        for (tag, expected) in [
            (1, 0),
            (2, 0),
            (3, 0),
            (5, 0),
            (13, 0),
            (6, 0),
            (8, 0),
            (9, 0xff),
            (10, 0),
            (11, 0xff),
        ] {
            let field =
                xroar_v2_find_tag_payload(&bytes, 0, end, tag).expect("field should remain");
            assert_eq!(
                xroar_v2_read_vuint(&bytes, field.payload_start),
                Some((expected, field.payload_end))
            );
        }
    }

    #[test]
    fn xroar_v2_vdg_patch_starts_references_at_clean_scanline_phase() {
        let mut bytes = xroar_v2_component_marker(b"VDG");
        bytes.extend_from_slice(&xroar_v2_component_marker(b"MC6847"));
        for tag in [
            6, 8, 9, 10, 11, 12, 16, 17, 18, 21, 22, 25, 26, 27, 28, 30, 31, 33, 35, 36, 37, 38,
            39, 40,
        ] {
            push_xroar_v2_test_field(&mut bytes, tag, 1);
        }
        bytes.push(0);
        bytes.extend_from_slice(&xroar_v2_component_marker(b"PIA1"));
        bytes.push(0);
        bytes.push(0);
        let snapshot = PcDragonSnapshot {
            ram: Box::new([]),
            load_address: 0,
            registers: format_dragon_pak::PcDragonRegisters {
                pc: 0,
                x: 0,
                y: 0,
                u: 0,
                s: 0,
                dp: 0,
                b: 0,
                a: 0,
                cc: 0,
            },
            peripherals: Some(format_dragon_pak::PcDragonPeripherals {
                ff02: 0,
                ff03: 0,
                ff22: 0xec,
            }),
            display_base: Some(0x0800),
        };

        patch_xroar_v2_vdg(&mut bytes, &snapshot).expect("VDG phase should be patchable");
        let range =
            xroar_v2_component_range(&bytes, b"VDG", Some(b"PIA1")).expect("VDG should exist");
        let start = find_bytes_from_until(&bytes, b"MC6847", range.start, range.end)
            .map(|offset| offset + b"MC6847".len())
            .expect("MC6847 payload should exist");

        assert_eq!(xroar_v2_test_field(&bytes, start, range.end, 14), Some(912));
        assert_eq!(xroar_v2_test_field(&bytes, start, range.end, 15), Some(64));
        assert_eq!(xroar_v2_test_field(&bytes, start, range.end, 16), Some(0));
        assert_eq!(xroar_v2_test_field(&bytes, start, range.end, 17), Some(134));
        assert_eq!(xroar_v2_test_field(&bytes, start, range.end, 18), Some(0));
    }

    fn push_xroar_v2_test_field(bytes: &mut Vec<u8>, tag: u32, value: u32) {
        let payload = xroar_v2_vuint(value);
        bytes.extend_from_slice(&xroar_v2_vuint(tag));
        bytes.extend_from_slice(&xroar_v2_vuint(payload.len() as u32));
        bytes.extend_from_slice(&payload);
        bytes.push(0);
    }

    fn xroar_v2_test_field(bytes: &[u8], start: usize, end: usize, tag: u32) -> Option<u32> {
        let field = xroar_v2_find_tag_payload(bytes, start, end, tag)?;
        xroar_v2_read_vuint(bytes, field.payload_start).map(|(value, _)| value)
    }

    #[test]
    fn xroar_snapshot_reference_traps_at_snapshot_pc() {
        let snapshot = PcDragonSnapshot {
            ram: Box::new([]),
            load_address: 0,
            registers: format_dragon_pak::PcDragonRegisters {
                pc: 0x1234,
                x: 0,
                y: 0,
                u: 0,
                s: 0,
                dp: 0,
                b: 0,
                a: 0,
                cc: 0,
            },
            peripherals: None,
            display_base: None,
        };

        assert_eq!(xroar_snapshot_trap_condition(&snapshot), "pc=0x1234");
    }

    #[test]
    fn cli_rejects_invalid_smoke_joystick_options() {
        let bad_port = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick".to_owned(),
            "3,fire,20".to_owned(),
        ])
        .expect_err("invalid joystick port should fail");
        assert!(bad_port.contains("expected 1 or 2"));

        let bad_control = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick".to_owned(),
            "1,button,20".to_owned(),
        ])
        .expect_err("invalid joystick control should fail");
        assert!(bad_control.contains("expected up, down, left, right, fire, or idle"));

        let bad_axis = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick-axis".to_owned(),
            "1,z,0.5,20".to_owned(),
        ])
        .expect_err("invalid joystick axis should fail");
        assert!(bad_axis.contains("expected x or y"));

        let bad_value = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick-axis".to_owned(),
            "1,x,1.5,20".to_owned(),
        ])
        .expect_err("invalid joystick axis value should fail");
        assert!(bad_value.contains("must be a finite number from -1.0 to 1.0"));

        let bad_sweep_steps = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-joystick-axis-sweep".to_owned(),
            "1,x,-1.0,1.0,0,20".to_owned(),
        ])
        .expect_err("invalid joystick axis sweep steps should fail");
        assert!(bad_sweep_steps.contains("steps must be greater than zero"));
    }

    #[test]
    fn smoke_axis_values_match_shared_normalized_range() {
        assert_eq!(parse_smoke_axis_value("-1.0"), Ok(i16::MIN));
        assert_eq!(parse_smoke_axis_value("0.0"), Ok(0));
        assert_eq!(parse_smoke_axis_value("1.0"), Ok(i16::MAX));
    }

    #[test]
    fn smoke_axis_sweep_values_include_endpoints() {
        let sweep = SmokeJoystickAxisSweep {
            port: 1,
            axis: SmokeJoystickAxis::X,
            start: i16::MIN,
            end: i16::MAX,
            steps: 3,
            frames: 12,
        };

        assert_eq!(axis_sweep_value(sweep, 0), i16::MIN);
        assert_eq!(axis_sweep_value(sweep, 1), 0);
        assert_eq!(axis_sweep_value(sweep, 2), i16::MAX);
    }

    #[test]
    fn cli_parses_xroar_reference_options() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--smoke-root".to_owned(),
            "tapes".to_owned(),
            "--xroar-bin".to_owned(),
            "xroar/src/xroar".to_owned(),
            "--xroar-reference-dir".to_owned(),
            "refs".to_owned(),
            "--xroar-snapshot-out".to_owned(),
            "snapshot.sna".to_owned(),
            "--xroar-motoroff".to_owned(),
            "2".to_owned(),
            "--xroar-settle-seconds".to_owned(),
            "2.5".to_owned(),
            "--xroar-timeout-seconds".to_owned(),
            "30".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.xroar_bin, Some(PathBuf::from("xroar/src/xroar")));
        assert_eq!(cli.xroar_reference_dir, Some(PathBuf::from("refs")));
        assert_eq!(cli.xroar_snapshot_out, Some(PathBuf::from("snapshot.sna")));
        assert_eq!(cli.xroar_motoroff, Some(2));
        assert_eq!(cli.xroar_settle_seconds, 2.5);
        assert_eq!(cli.xroar_timeout_seconds, 30.0);
    }

    #[test]
    fn xroar_reference_config_requires_bin_and_output_dir() {
        let missing_dir = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--xroar-bin".to_owned(),
            "xroar".to_owned(),
        ])
        .expect("CLI parsing should allow validation later");

        assert_eq!(
            xroar_reference_config(&missing_dir),
            Err("--xroar-reference-dir is required with --xroar-bin".to_owned())
        );
    }

    #[test]
    fn xroar_type_text_matches_dragon_load_command() {
        assert_eq!(
            xroar_type_text("CLOAD", Some("RUN")),
            Ok("CLOAD\rRUN\r".to_owned())
        );
        assert_eq!(xroar_type_text("CLOAD", None), Ok("CLOAD\r".to_owned()));
        assert_eq!(
            xroar_type_text("CLOADM", Some("EXEC")),
            Ok("CLOADM\rEXEC\r".to_owned())
        );
        assert_eq!(xroar_type_text("CLOADM", None), Ok("CLOADM\r".to_owned()));
        assert_eq!(default_xroar_motoroff("CLOAD", Some("RUN")), 2);
        assert_eq!(default_xroar_motoroff("CLOAD", None), 3);
        assert_eq!(default_xroar_motoroff("CLOADM", Some("EXEC")), 2);
        assert_eq!(default_xroar_motoroff("CLOADM", None), 2);
    }

    #[test]
    fn xroar_zoomed_screenshot_scales_active_area_without_downscaling() {
        let mut pixels =
            vec![0; TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4];
        let active_offset = (motorola_vdg_6847::TEXT_TOP_BORDER_LINES
            * TEXT_VISIBLE_FRAMEBUFFER_WIDTH
            + motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS)
            * 4;
        pixels[active_offset..active_offset + 4].copy_from_slice(&[0x12, 0x34, 0x56, 0xFF]);
        let frame = CapturedFrame {
            timestamp: emu198x_shell::MachineTime(0),
            format: PixelFormat::Rgba8888,
            width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            palette: None,
            pixels,
        };

        let png = xroar_zoomed_png_bytes(&frame).expect("zoomed PNG should encode");
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder
            .read_info()
            .expect("zoomed screenshot should decode");
        assert_eq!(reader.info().width, XROAR_ZOOMED_WIDTH);
        assert_eq!(reader.info().height, XROAR_ZOOMED_HEIGHT);
        let mut output = vec![
            0;
            reader
                .output_buffer_size()
                .expect("png buffer size fits in usize")
        ];
        let info = reader
            .next_frame(&mut output)
            .expect("zoomed screenshot should contain one frame");
        assert_eq!(&output[..4], &[0x12, 0x34, 0x56, 0xFF]);
        assert_eq!(&output[4..8], &[0x12, 0x34, 0x56, 0xFF]);
        assert_eq!(info.color_type, png::ColorType::Rgba);
    }

    #[test]
    fn xroar_zoomed_screenshot_accepts_pal_overscan_frames() {
        let mut pixels =
            vec![0; VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH * VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT * 4];
        let active_offset = ((VDG_PAL_OVERSCAN_VISIBLE_Y
            + motorola_vdg_6847::TEXT_TOP_BORDER_LINES)
            * VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH
            + VDG_PAL_OVERSCAN_VISIBLE_X
            + motorola_vdg_6847::TEXT_LEFT_BORDER_PIXELS * 2)
            * 4;
        pixels[active_offset..active_offset + 4].copy_from_slice(&[0xAB, 0xCD, 0xEF, 0xFF]);
        let frame = CapturedFrame {
            timestamp: emu198x_shell::MachineTime(0),
            format: PixelFormat::Rgba8888,
            width: VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32,
            height: VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32,
            palette: None,
            pixels,
        };

        let png = xroar_zoomed_png_bytes(&frame).expect("zoomed PNG should encode");
        let decoded = decode_png_rgba(&png).expect("zoomed screenshot should decode");
        assert_eq!(decoded.width, XROAR_ZOOMED_WIDTH);
        assert_eq!(decoded.height, XROAR_ZOOMED_HEIGHT);
        assert_eq!(&decoded.rgba[..4], &[0xAB, 0xCD, 0xEF, 0xFF]);
    }

    #[test]
    fn image_comparison_reports_dimension_and_pixel_differences() {
        let emu = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 0xFF, 10, 20, 30, 0xFF],
        };
        let reference = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 0xFF, 20, 10, 30, 0xFF],
        };

        let comparison = compare_images(&emu, &reference);

        assert!(comparison.dimensions_match);
        assert_eq!(comparison.compared_pixels, 2);
        assert_eq!(comparison.differing_pixels, 1);
        assert_eq!(comparison.differing_pixel_percent, 50.0);
        assert_eq!(comparison.max_channel_delta, 10);
        assert!((comparison.mean_abs_channel_delta - (20.0 / 6.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn classifies_started_graphics_that_draws_after_settle() {
        let text_video = DragonVideoState {
            sam_video_mode: 0,
            sam_display_offset: 2,
            display_base: 0x0400,
            pia1_output_b: 0x07,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };
        let graphics_video = DragonVideoState {
            sam_video_mode: 6,
            sam_display_offset: 3,
            display_base: 0x0600,
            pia1_output_b: 0xE7,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };

        let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
            command: "CLOAD",
            load_result: "ok",
            start_result: "visible-change",
            load_visible_change: true,
            visible_change_after_start: true,
            start_video_changed: true,
            start_settle_visible_change: true,
            basic_error: false,
            load_video: text_video,
            start_video: graphics_video,
        });

        assert_eq!(
            classification,
            RuntimeSmokeClassification::StartedGraphicsDrawing
        );
    }

    #[test]
    fn classifies_machine_code_autorun_after_load() {
        let video = DragonVideoState {
            sam_video_mode: 0,
            sam_display_offset: 2,
            display_base: 0x0400,
            pia1_output_b: 0x07,
            pia1_ddr_b: 0xF8,
            pia1_control_b: 0x37,
            pia1_cb2: false,
        };

        let classification = classify_runtime_smoke(RuntimeSmokeClassificationInput {
            command: "CLOADM",
            load_result: "ok",
            start_result: "already-running-after-load",
            load_visible_change: true,
            visible_change_after_start: false,
            start_video_changed: false,
            start_settle_visible_change: false,
            basic_error: false,
            load_video: video,
            start_video: video,
        });

        assert_eq!(
            classification,
            RuntimeSmokeClassification::MachineCodeRunningAfterLoad
        );
    }

    #[test]
    fn dragon_key_labels_map_to_confirmed_matrix_positions() {
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("a").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("A").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("@").expect("@ should parse")),
            MatrixKey::new(2, 0)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("space").expect("space should parse")),
            MatrixKey::new(5, 7)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(parse_dragon_key("right").expect("right should parse")),
            MatrixKey::new(5, 6)
        );
    }

    #[test]
    fn dragon_key_parser_accepts_control_key_aliases() {
        assert_eq!(parse_dragon_key("return"), Ok(DragonKey::Enter));
        assert_eq!(parse_dragon_key("clr"), Ok(DragonKey::Clear));
        assert_eq!(parse_dragon_key("brk"), Ok(DragonKey::Break));
    }

    #[test]
    fn cli_parses_named_dragon_key_presses_semantically() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press".to_owned(),
            "A".to_owned(),
            "--press".to_owned(),
            "@".to_owned(),
            "--press".to_owned(),
            "enter".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![
                MatrixKey::new(2, 1),
                MatrixKey::new(2, 0),
                MatrixKey::new(6, 0),
            ]
        );
    }

    #[test]
    fn load_rom_accepts_zip_archives() {
        let rom = rom_with_reset_vector(0x8000);
        let path = env::temp_dir().join(format!(
            "emu198x-dragon-rom-test-{}.zip",
            std::process::id()
        ));

        let file = fs::File::create(&path).expect("test zip should be creatable");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("dragon32.rom", zip::write::SimpleFileOptions::default())
            .expect("zip entry should start");
        zip.write_all(&rom).expect("zip entry should be writable");
        zip.finish().expect("zip should finish");

        let loaded = load_rom(&path).expect("zip ROM should load");
        fs::remove_file(&path).expect("test zip should be removable");

        assert_eq!(loaded, rom);
    }

    fn synthetic_visible_bin() -> Vec<u8> {
        let load_address = 0x2800_u16;
        let mut payload = vec![0x86, 0xBF]; // LDA #$BF: solid semigraphics block.
        for addr in 0x0400_u16..0x0420 {
            let [hi, lo] = addr.to_be_bytes();
            payload.extend_from_slice(&[0xB7, hi, lo]); // STA addr
        }
        payload.extend_from_slice(&[0x20, 0xFE]); // BRA *: remain visibly running.

        let len = u16::try_from(payload.len()).expect("test payload should fit in BIN header");
        let [load_hi, load_lo] = load_address.to_be_bytes();
        let [len_hi, len_lo] = len.to_be_bytes();
        let mut bytes = vec![
            0x55, 0x02, load_hi, load_lo, len_hi, len_lo, load_hi, load_lo, 0xAA,
        ];
        bytes.extend_from_slice(&payload);
        bytes
    }

    fn temp_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn test_dragon32_rom_path() -> Option<PathBuf> {
        if let Some(path) = existing_env_path("EMU198X_DRAGON32_ROM") {
            return Some(path);
        }

        if let Some(path) = home_path(".emu198x/roms/dragon/dragon32.rom") {
            return Some(path);
        }

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)?;
        let sibling_archive = repo_root
            .parent()?
            .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 32 BIOS (1982)(Dragon Data).zip");
        if sibling_archive.exists() {
            return Some(sibling_archive);
        }

        None
    }

    fn test_required_env_path(var: &str) -> Option<PathBuf> {
        match existing_env_path(var) {
            Some(path) => Some(path),
            None => {
                eprintln!("skipping DragonDOS DIR regression: set {var}");
                None
            }
        }
    }

    fn assert_screen_contains(lines: &[String], expected: &str) {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "screen text missing {expected:?}:\n{}",
            lines.join("\n")
        );
    }

    fn existing_env_path(var: &str) -> Option<PathBuf> {
        let path = PathBuf::from(env::var_os(var)?);
        if path.exists() { Some(path) } else { None }
    }

    fn home_path(relative: &str) -> Option<PathBuf> {
        let path = PathBuf::from(env::var_os("HOME")?).join(relative);
        if path.exists() { Some(path) } else { None }
    }

    fn xroar_v1_chunk(bytes: &[u8], section: u8) -> Option<&[u8]> {
        let mut offset = b"XRoar snapshot.\n\0".len();
        while offset + 3 <= bytes.len() {
            let chunk_section = bytes[offset];
            let len = u16::from_be_bytes([bytes[offset + 1], bytes[offset + 2]]) as usize;
            offset += 3;
            let end = offset.checked_add(len)?;
            if end > bytes.len() {
                return None;
            }
            if chunk_section == section {
                return Some(&bytes[offset..end]);
            }
            offset = end;
        }
        None
    }
}
