//! Shared JSON script execution on top of one headless session.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::asset::{AssetLoadError, read_media_asset};
use crate::control::ControlCommand;
use crate::debug::DebugTarget;
use crate::debug_info::{DebugInfoError, DebugSymbols, SourceLine};
use crate::machine::{MachineCore, ResetKind};
use crate::media::{MediaImage, MediaKind, MediaSet};
use crate::query::{QueryError, QueryPathsResult, QueryResult, SessionQueryProvider};
use crate::session::{HeadlessSession, SessionError};
use crate::watch::{WatchAyRecord, WatchMemoryRecord, WatchMemorySource};

/// One user-facing script media kind with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaKind {
    Tape,
    Disk,
    Cartridge,
    Optical,
    Snapshot,
    Program,
}

impl From<ScriptMediaKind> for MediaKind {
    fn from(value: ScriptMediaKind) -> Self {
        match value {
            ScriptMediaKind::Tape => MediaKind::Tape,
            ScriptMediaKind::Disk => MediaKind::Disk,
            ScriptMediaKind::Cartridge => MediaKind::Cartridge,
            ScriptMediaKind::Optical => MediaKind::Optical,
            ScriptMediaKind::Snapshot => MediaKind::Snapshot,
            ScriptMediaKind::Program => MediaKind::Program,
        }
    }
}

/// One user-facing media transport action with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaTransportAction {
    Start,
    Stop,
}

impl From<ScriptMediaTransportAction> for crate::MediaTransportAction {
    fn from(value: ScriptMediaTransportAction) -> Self {
        match value {
            ScriptMediaTransportAction::Start => crate::MediaTransportAction::Start,
            ScriptMediaTransportAction::Stop => crate::MediaTransportAction::Stop,
        }
    }
}

/// One captured AY-3-8912 register write reported by
/// [`ScriptObservation::WatchAyLog`].
///
/// Captured at the moment of `OUT ($BFFD), data` — `register` is
/// whichever the most-recent `OUT ($FFFD), reg_index` selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AyWriteEntry {
    /// CPU program counter at the moment of the write. Widened to
    /// `u32` to match the rest of the family-wide observation shape.
    pub pc: u32,
    /// AY register index (0-15) that was selected at the write.
    pub register: u8,
    /// Byte written to the selected register.
    pub value: u8,
}

/// One decoded instruction returned by [`ScriptObservation::Disasm`].
///
/// With a Debug198x sidecar attached to the session (`load_debug_info`),
/// `mnemonic` has its address operands substituted (`JSR $C012` reads `JSR
/// init`), and `symbol` and `source` carry the label at this address and the
/// line that produced it. With no sidecar, `mnemonic` is the raw disassembly
/// and both new fields are omitted from the serialised form, so output for a
/// build without debug info is byte for byte what it was before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisasmInstruction {
    /// Address of this instruction.
    pub addr: u32,
    /// Length of the encoded instruction in bytes.
    pub bytes: u8,
    /// Raw encoded bytes (`bytes` long).
    pub raw: Vec<u8>,
    /// Decoded mnemonic.
    pub mnemonic: String,
    /// Label defined exactly at this address, from the loaded sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Source line that assembled to this instruction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLine>,
}

/// One captured write reported by [`ScriptObservation::WatchMemoryLog`].
///
/// Widened to `u32` on every address field so the same shape covers both
/// 16-bit (Z80, 6502) and 32-bit (68000) address spaces. `cck` and
/// `size_bytes` carry the richer 68000 detail the Amiga records; byte-only
/// 8/16-bit machines leave `cck` absent and `size_bytes` `1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteEntry {
    /// CPU program counter at the moment of observation. For DMA writes this
    /// is concurrent CPU context rather than the writer's instruction PC.
    pub pc: u32,
    /// Target address of the write.
    pub addr: u32,
    /// Value that was written. Bytes occupy the low 8 bits; words
    /// occupy the low 16.
    pub value: u32,
    /// Colour-clock timestamp of the write, when the machine stamps one
    /// (the Amiga does; the 8/16-bit cores do not). Omitted from the JSON
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cck: Option<u64>,
    /// Width of the write in bytes (`1` for a byte store, `2` for a word).
    pub size_bytes: u8,
    /// Hardware agent that issued the write, when the machine distinguishes
    /// writers. Omitted for CPU-only family watches to preserve their prior
    /// JSON representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::watch::WatchMemorySource>,
}

/// One shared JSON script step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ScriptStep {
    /// Load one media image into a named slot.
    LoadMedia {
        /// Stable slot identifier.
        slot: String,
        /// User-facing JSON media kind.
        kind: ScriptMediaKind,
        /// Path to the media image on disk.
        path: PathBuf,
        /// Whether the machine may write back to this image. Defaults to
        /// `false` (archive-safe); a work disk opts in. See
        /// `knowledge/decisions/disk-save-write-back.md`.
        #[serde(default)]
        writable: bool,
    },
    /// Start or stop media transport on a named slot.
    MediaTransport {
        /// Stable slot identifier.
        slot: String,
        /// Requested transport action.
        transport: ScriptMediaTransportAction,
    },
    /// Queue generic input events for the next run step.
    Input {
        /// The events to queue.
        events: Vec<crate::InputEvent>,
    },
    /// Run the machine for one number of native frames.
    RunFrames {
        /// Number of native video frames to execute.
        frames: u32,
    },
    /// Run the machine for an exact number of sub-frame ticks (one
    /// tick = one unit of the authoritative clock, e.g. one PPU dot on
    /// the NES). For cycle-exact debugging; not all runtimes support
    /// it. See [`crate::MachineCore::run_ticks`].
    RunTicks {
        /// Number of authoritative-clock ticks to execute.
        ticks: u64,
    },
    /// Run native frames until `boot.detected = true`.
    WaitForBoot {
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Run native frames until one text-bearing query contains one substring.
    WaitForQueryContains {
        /// The query path to poll.
        path: String,
        /// The required substring.
        needle: String,
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Run native frames until one boolean query path reaches one target
    /// value.
    WaitForQueryBool {
        /// The query path to poll.
        path: String,
        /// The required boolean value.
        value: bool,
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Resolve one shared query path.
    Query {
        /// The query path to resolve.
        path: String,
    },
    /// List supported query paths, optionally filtered by prefix.
    QueryPaths {
        /// Optional prefix filter.
        prefix: Option<String>,
    },
    /// Restore one snapshot file into the live machine.
    ///
    /// The shell crate's built-in executor decodes the runtime's own
    /// postcard save state. Per-system binaries that also handle
    /// portable formats intercept this step before delegation — the
    /// Spectrum binary, for example, dispatches `.sna` / `.z80` (and
    /// `.zip` archives wrapping one of those) through the appropriate
    /// format crate and falls back to the shell executor for postcard
    /// payloads. From the script author's perspective there is one
    /// step regardless of format; the binary chooses the parser from
    /// the file extension.
    LoadSnapshot {
        /// Path to the snapshot on disk.
        path: PathBuf,
    },
    /// Save the current machine snapshot to disk.
    SaveSnapshot {
        /// Output path for the snapshot.
        path: PathBuf,
    },
    /// Save the latest emitted frame as PNG.
    SaveScreenshot {
        /// Output path for the PNG file.
        path: PathBuf,
    },
    /// **Legacy.** Save the entire session capture buffer as WAV.
    ///
    /// Prefer [`Self::StartAudioRecording`] / [`Self::StopAudioRecording`]
    /// for any new script. The bracketed start/stop pair captures a
    /// clean window bounded by script steps; this variant dumps every
    /// sample emitted since session start, including silence from frames
    /// before the chapter began, and needs the `reset_after` boolean to
    /// avoid leaking that pre-roll into the next call.
    ///
    /// Retained for back-compat with existing scripts. When `reset_after`
    /// is not enough — e.g. you want to dump the buffer, keep a copy on
    /// disk, *and* keep the buffer running — pair this with
    /// [`Self::ClearAudioCapture`] instead and leave `reset_after: false`.
    SaveAudioCapture {
        /// Output path for the WAV file.
        path: PathBuf,
        /// Whether to clear captured audio after writing the file.
        ///
        /// Prefer the explicit [`Self::ClearAudioCapture`] step on new
        /// scripts; this boolean is kept to avoid breaking existing
        /// JSON payloads.
        #[serde(default = "default_true")]
        reset_after: bool,
    },
    /// Drop the session capture buffer without writing it to disk.
    ///
    /// Pairs with [`Self::SaveAudioCapture`] when you want to dump the
    /// buffer, preserve the WAV on disk, and reset the buffer in two
    /// explicit steps rather than the `reset_after` boolean. Has no
    /// effect on the start/stop recording path — that uses its own
    /// per-recording offset.
    ClearAudioCapture,
    /// Switch the live machine to the named variant, loading its
    /// default ROM bundle from the conventional on-disk location.
    ///
    /// `machine` is a system-specific identifier (e.g.
    /// `"spectrum_48k"`, `"spectrum_128k"`) that the binary translates
    /// to its native machine kind and ROM-bundle resolver. The shell
    /// crate stays system-agnostic and surfaces this step via
    /// [`ScriptError::SystemSpecificStep`] when its built-in executor
    /// is asked to run it without a binary-side handler.
    ///
    /// Always resets in-progress state — loaded media, snapshots,
    /// frame counter, audio buffer. Use this as the first step of
    /// any script that targets a non-default variant.
    SetMachine {
        /// Snake-case variant identifier (binary-defined vocabulary).
        machine: String,
    },
    /// Wait for boot, then drive the BASIC editor to type `LOAD ""`
    /// and start tape transport on the named slot.
    ///
    /// Wraps the existing host-side `autoload_basic_tape` helper.
    /// System-specific (the helper currently lives in
    /// `runtime-sinclair-zx-spectrum`); same dispatch pattern as
    /// [`Self::SetMachine`].
    AutoloadTape {
        /// Stable slot identifier carrying the tape (e.g. `"tape-1"`).
        slot: String,
        /// Maximum number of native frames to spend waiting for boot
        /// before failing the autoload.
        max_boot_frames: u32,
    },
    /// Tokenise one plain-text `.bas` file and install it as the
    /// machine's current BASIC program.
    ///
    /// System-specific (the tokeniser and RAM-poke routines are per
    /// dialect); same dispatch pattern as [`Self::SetMachine`] —
    /// the binary intercepts this step before delegation. When `run`
    /// is `true` (the default), the binary also drives the editor's
    /// RUN keyword so the program starts executing.
    LoadBasicProgram {
        /// Path to the plain-text BASIC source file on disk.
        path: PathBuf,
        /// Whether to immediately RUN the program after installing it.
        #[serde(default = "default_true")]
        run: bool,
    },
    /// Begin recording the live framebuffer + audio to one MP4 file.
    ///
    /// Subsequent `RunFrames` / wait steps tee every emitted frame into the
    /// recorder. While a recording is active, `LoadSnapshot`, nested
    /// `StartVideoRecording`, and (binary-side) `SetMachine` are rejected
    /// because each would jump-cut the clip.
    StartVideoRecording {
        /// Final output path for the MP4 file.
        path: PathBuf,
    },
    /// Finalise the in-flight video recording.
    ///
    /// Closes the ffmpeg pipe, waits for the video pass, and (when audio has
    /// been captured) runs a second ffmpeg pass to mux audio into the final
    /// MP4. Emits [`ScriptObservation::StopVideoRecording`].
    StopVideoRecording,
    /// Begin recording emitted audio to a 16-bit PCM WAV file.
    ///
    /// Mirrors [`Self::StartVideoRecording`] for audio-only capture.
    /// Subsequent `run_frames` / wait steps tee audio into the
    /// session's capture buffer; the final WAV is written when
    /// [`Self::StopAudioRecording`] is called and contains only the
    /// samples emitted between the two calls.
    ///
    /// Use this instead of [`Self::SaveAudioCapture`] when the
    /// recording window is bounded by script steps rather than the
    /// whole session lifetime — no manual `reset_after` choreography,
    /// no leaked silence from frames before the chapter started.
    StartAudioRecording {
        /// Final output path for the WAV file.
        path: PathBuf,
    },
    /// Finalise the in-flight audio recording.
    ///
    /// Slices the audio capture buffer from the offset captured at
    /// `StartAudioRecording` to the current end, encodes the result
    /// as 16-bit PCM WAV, and writes it to disk. Emits
    /// [`ScriptObservation::StopAudioRecording`].
    StopAudioRecording,
    /// Query the AY-3-8912 chip's full register state in one call.
    ///
    /// Spectrum-family system-specific step (errors with
    /// [`ScriptError::SystemSpecificStep`] on the shell's built-in
    /// executor; the Spectrum binary intercepts it before delegation
    /// and returns [`ScriptObservation::QueryAy`] with the 16 raw
    /// registers plus decoded tone periods, mixer routing, amplitudes,
    /// noise period, and envelope. Errors when the active variant
    /// does not have an AY (16K / 48K / Spectrum+).
    ///
    /// Wraps the low-level `spectrum.ay.registers` /
    /// `spectrum.ay.selected_register` queries with named fields so
    /// curriculum scripts can assert on chip state without decoding
    /// the 16-byte raw array themselves.
    QueryAy,
    /// Query the CPU's register file in one call.
    ///
    /// System-specific step (binary-dispatched). For Spectrum this
    /// returns every Z80 register — the main bank (AF/BC/DE/HL + the
    /// 8-bit halves), the alternate bank (AF'/BC'/DE'/HL'), index
    /// registers (IX/IY), control (PC/SP/I/R), interrupt state
    /// (IM/IFF1/IFF2), and the decoded F flags (S/Z/5/H/3/P-V/N/C).
    /// Emits [`ScriptObservation::QueryCpu`].
    QueryCpu,
    /// Single-step the CPU.
    ///
    /// System-specific step (binary-dispatched). Runs the machine
    /// until `instructions` instructions have completed (default 1),
    /// then emits [`ScriptObservation::Step`] with the post-step PC,
    /// halt state, and cycles consumed.
    Step {
        /// Number of instructions to execute. Defaults to 1.
        #[serde(default)]
        instructions: Option<u32>,
    },
    /// Run the CPU until PC reaches a target address.
    ///
    /// Steps whole instructions through the shared `DebugTarget` until
    /// PC matches `addr`, or `max_steps` instructions have elapsed
    /// (default 2,000,000). Emits [`ScriptObservation::RunUntilPc`].
    RunUntilPc {
        /// Target program counter.
        addr: u32,
        /// Optional instruction budget. `None` uses the default cap.
        #[serde(default)]
        max_steps: Option<u64>,
    },
    /// Attach a Debug198x sidecar to the session.
    ///
    /// Reads the `.debug198x` file Asm198x wrote beside the image, and from
    /// then on `disasm` annotates instructions with labels and source lines,
    /// `debug_symbol` resolves names, and `run_until_line` can break on a
    /// source line. Emits [`ScriptObservation::DebugInfoLoaded`].
    LoadDebugInfo {
        /// Path to the `.debug198x` sidecar.
        path: PathBuf,
        /// Absolute load addresses for relocatable sections, keyed by section
        /// id. Only needed where the sidecar cannot know the address itself —
        /// Amiga hunks placed by the loader. Absolutely-located builds carry
        /// their base in the sidecar and need nothing here.
        #[serde(default)]
        section_bases: BTreeMap<u32, u64>,
    },
    /// Look up one symbol from the loaded sidecar.
    ///
    /// Resolves a label to its absolute address, or a constant to its value —
    /// the address to hand to `run_until_pc` to break on a routine by name.
    /// Emits [`ScriptObservation::DebugSymbol`].
    DebugSymbol {
        /// Symbol name as it appears in the source.
        name: String,
    },
    /// Run the CPU until it reaches a source line.
    ///
    /// Resolves `file`:`line` to the lowest address that line assembled to,
    /// then runs as `run_until_pc` does. Requires a loaded sidecar. Emits
    /// [`ScriptObservation::RunUntilLine`].
    RunUntilLine {
        /// Source file name. Matched against the sidecar's recorded path in
        /// full, or by basename — the assembler records the path it was given,
        /// which is rarely the one a debugger UI knows.
        file: String,
        /// 1-based line number.
        line: u32,
        /// Optional instruction budget. `None` uses the default cap.
        #[serde(default)]
        max_steps: Option<u64>,
    },
    /// Run the CPU until PC reaches any of several target addresses.
    ///
    /// Steps whole instructions through the shared `DebugTarget` until
    /// PC matches any entry in `targets`, or `max_steps` instructions
    /// elapse. Emits [`ScriptObservation::RunUntilAnyPc`].
    RunUntilAnyPc {
        /// Target program counters; stops at the first one hit.
        targets: Vec<u32>,
        /// Optional instruction budget. `None` uses the default cap.
        #[serde(default)]
        max_steps: Option<u64>,
    },
    /// Run the CPU until any watched byte changes value.
    ///
    /// Steps whole instructions through the shared `DebugTarget`,
    /// peeking each address in `addrs` after every step, until one
    /// changes or `max_steps` instructions elapse. Emits
    /// [`ScriptObservation::RunUntilMemChange`].
    RunUntilMemChange {
        /// Addresses to watch.
        addrs: Vec<u32>,
        /// Optional instruction budget. `None` uses the default cap.
        #[serde(default)]
        max_steps: Option<u64>,
    },
    /// Disassemble a span of CPU memory.
    ///
    /// Reads bytes through the shared `DebugTarget` bus view (so paging
    /// is honoured) and decodes `instructions` opcodes starting at
    /// `addr`. Emits [`ScriptObservation::Disasm`].
    Disasm {
        /// Starting address.
        addr: u32,
        /// Number of instructions to decode. Defaults to 16, max 64.
        #[serde(default)]
        instructions: Option<u32>,
    },
    /// Read one CPU I/O port.
    ///
    /// System-specific step (binary-dispatched). For Spectrum this
    /// hits the same bus-level handler the Z80's `IN A,(C)` would
    /// drive — ULA at `$FE`, Kempston at `$1F`, AY data at `$FFFD`,
    /// etc. — without driving the CPU through the synthetic
    /// instruction. Emits [`ScriptObservation::PortRead`].
    PortRead {
        /// 16-bit port address.
        port: u16,
    },
    /// Write one CPU I/O port.
    ///
    /// System-specific step (binary-dispatched). Side-effects mirror
    /// `OUT (C),A` — border colour, beeper level, 128K paging, AY
    /// register select / data, etc. Silent — no observation.
    PortWrite {
        /// 16-bit port address.
        port: u16,
        /// 8-bit value to drive on the data bus.
        value: u8,
    },
    /// Begin tracing every `OUT ($BFFD), data` write to the AY data
    /// port, capturing `(pc, register, value)`.
    ///
    /// System-specific step (binary-dispatched). Errors when the
    /// active variant has no AY (16K / 48K / Spectrum+ / TC2048).
    /// Emits [`ScriptObservation::WatchAyStart`].
    WatchAyStart,
    /// Stop tracing AY writes and drop the captured log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchAyClear`].
    WatchAyClear,
    /// Fetch the captured AY write log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchAyLog`] with up to `limit`
    /// most-recent entries (default 64).
    WatchAyLog {
        /// Maximum number of entries to return.
        #[serde(default)]
        limit: Option<u32>,
        /// When `true`, deduplicate identical `(pc, register, value)`
        /// triples before applying the limit.
        #[serde(default)]
        unique: bool,
    },
    /// Press one named key, hold for a configurable number of native
    /// frames, then release.
    ///
    /// System-specific (binary-dispatched). Replaces the three-step
    /// "input press / run_frames / input release" dance with a
    /// single observation-emitting step. Default `hold_frames` is 3,
    /// which is comfortably above the ROM's typical poll interval.
    /// One extra settle frame runs after the release so the released
    /// state is visible by the next step.
    ///
    /// Errors when the key name isn't recognised by the active
    /// machine's keyboard layout.
    PressKey {
        /// Named key (see the per-system keyboard module). For
        /// Spectrum: `A`-`Z`, `0`-`9`, `Space`, `Enter`,
        /// `CapsShift`, `SymbolShift`.
        key: String,
        /// Number of native frames to hold the key. Defaults to 3.
        #[serde(default)]
        hold_frames: Option<u32>,
    },
    /// Press several named keys as a chord — all held down together for
    /// `hold_frames`, then released in reverse order. The verb for key
    /// combinations no single keystroke covers: the Amiga's
    /// Ctrl-Amiga-Amiga reset, the C64's Run/Stop+Restore, the Spectrum's
    /// Caps-Shift compound functions (`CapsShift` + `1` = Edit), and any
    /// modifier+key sequence.
    ///
    /// Errors when any key name is not recognised by the active machine's
    /// keyboard layout.
    PressKeys {
        /// Named keys to hold simultaneously, in press order (modifiers
        /// first by convention). Released in reverse.
        keys: Vec<String>,
        /// Number of native frames to hold the chord. Defaults to 3.
        #[serde(default)]
        hold_frames: Option<u32>,
    },
    /// Type a string of characters, pressing each key in sequence
    /// with proper hold/release timing. Handles uppercase via
    /// CapsShift automatically. Newlines in the string press Enter.
    /// Runs `settle_frames` after the final keystroke (default 10).
    TypeString {
        /// The text to type. A-Z, 0-9, space, and newline are
        /// supported. Uppercase letters use CapsShift automatically.
        text: String,
        /// Frames to hold each key (default 3).
        #[serde(default)]
        hold_frames: Option<u32>,
        /// Extra frames to run after the last keystroke (default 10).
        #[serde(default)]
        settle_frames: Option<u32>,
    },
    /// Reset the running machine.
    ///
    /// `kind = "hard"` is a power-cycle equivalent (machine state and
    /// RAM are reconstructed from firmware). `kind = "soft"` is a
    /// machine-local soft reset (intended to preserve RAM where the
    /// chipset supports it). Today most systems treat both kinds
    /// identically — the variant is plumbed end-to-end so per-system
    /// soft-reset semantics can land incrementally without changing
    /// the wire format.
    ///
    /// Clears session-side state (queued input, latest frame,
    /// captured audio, last run result). Rejected while a video
    /// recording is in flight, same rule as [`Self::LoadSnapshot`] —
    /// reset would jump-cut the clip.
    Reset {
        /// The kind of reset to perform. Defaults to `hard` when omitted.
        #[serde(default)]
        kind: ResetKind,
    },
    /// Read a contiguous span of CPU-visible memory.
    ///
    /// System-specific step (errors with
    /// [`ScriptError::SystemSpecificStep`] on the shell's built-in
    /// executor; per-system binaries intercept it and resolve through
    /// their machine's memory bus). Emits
    /// [`ScriptObservation::MemoryRead`] with the bytes read.
    MemoryRead {
        /// Start address (CPU-visible, low byte first).
        addr: u32,
        /// Number of bytes to read. Implementations may cap at 256.
        ///
        /// Optional, defaulting to 16. The MCP schema has always advertised
        /// `"default": 16` and listed only `addr` as required, but the field
        /// had no serde default, so omitting it failed with `missing field
        /// len` — a tool promising something it did not do (#905).
        #[serde(default = "default_memory_read_len")]
        len: u32,
    },
    /// Write one byte to CPU-visible memory.
    ///
    /// System-specific step (binary-dispatched). Silent — emits no
    /// observation.
    PokeByte {
        /// Target address.
        addr: u32,
        /// Byte to write.
        value: u8,
    },
    /// Write one 16-bit word to CPU-visible memory.
    ///
    /// System-specific step (binary-dispatched). The write order is
    /// system-defined: Spectrum/Z80 writes little-endian (low byte at
    /// `addr`); 68000-class systems write big-endian. Silent.
    PokeWord {
        /// Target address.
        addr: u32,
        /// 16-bit value to write.
        value: u16,
    },
    /// Begin recording writes inside the half-open address range
    /// `[addr, addr + len)`. Replaces any prior watch range and clears the
    /// captured log. Source-aware machines can include DMA writers.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryStart`].
    WatchMemoryStart {
        /// Watch range start address.
        addr: u32,
        /// Watch range length in bytes (must be ≥ 1).
        len: u32,
    },
    /// Stop watching and drop both the range and the captured log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryClear`] with the count of
    /// records that were dropped.
    WatchMemoryClear,
    /// Fetch the captured write log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryLog`] with up to `limit`
    /// most-recent entries (default 64).
    WatchMemoryLog {
        /// Maximum number of entries to return. Defaults to 64.
        #[serde(default)]
        limit: Option<u32>,
        /// When `true`, deduplicate identical `(pc, addr, value, source)`
        /// tuples before applying the limit.
        #[serde(default)]
        unique: bool,
        /// Return only writes explicitly attributed to this hardware agent.
        /// CPU-only family watches do not stamp provenance and therefore do
        /// not match a source filter.
        #[serde(default)]
        source: Option<WatchMemorySource>,
        /// Return only timestamped writes at or after this CCK. Records from
        /// machines without CCK timestamps do not match a CCK filter.
        #[serde(default)]
        cck_min: Option<u64>,
        /// Return only timestamped writes at or before this CCK. Records from
        /// machines without CCK timestamps do not match a CCK filter.
        #[serde(default)]
        cck_max: Option<u64>,
    },
}

/// One JSON script made of ordered shared steps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeadlessScript {
    /// The ordered steps to execute.
    pub steps: Vec<ScriptStep>,
}

/// One structured observation emitted by the shared script layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptObservation {
    /// The final frame held a single colour.
    ///
    /// Emitted once per report rather than per step, and only when the
    /// last frame is uniform. A blank final frame is legitimate for many
    /// programs — one that animates can simply end mid-blank — so this
    /// is a note to grep for, not a failure.
    ///
    /// `last_painted_frame` against `frames_seen` is what separates a
    /// dead run from a live one. `None` means nothing ever painted; a
    /// value far short of `frames_seen` means the picture stopped
    /// changing and never came back. See
    /// `knowledge/decisions/a-run-that-paints-nothing-says-so.md`.
    BlankFrame {
        /// The colour every pixel held, as `#RRGGBB`.
        colour: String,
        /// Frame width in pixels, so a degenerate frame is distinguishable
        /// from a black one.
        width: u32,
        /// Frame height in pixels.
        height: u32,
        /// Frames emitted during this run.
        frames_seen: u64,
        /// Index of the last frame that showed more than one colour.
        /// `None` if no frame ever did.
        last_painted_frame: Option<u64>,
    },
    /// Result of a frame-run step.
    RunFrames {
        /// Number of requested native frames.
        frames: u32,
        /// Machine time reached after the run.
        reached: crate::MachineTime,
        /// Why the machine stopped.
        stop_reason: crate::StopReason,
    },
    /// Result of a sub-frame tick-run step.
    RunTicks {
        /// Number of requested authoritative-clock ticks.
        ticks: u64,
        /// Machine time reached after the run.
        reached: crate::MachineTime,
        /// Why the machine stopped.
        stop_reason: crate::StopReason,
    },
    /// Result of waiting for boot detection.
    WaitForBoot {
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when boot was detected.
        reached: crate::MachineTime,
        /// Human-readable boot status note.
        reason: String,
        /// Optional decoded text row reported by `boot.row`.
        row: Option<u64>,
    },
    /// Result of waiting for one text-bearing query to contain one substring.
    WaitForQueryContains {
        /// The query path that matched.
        path: String,
        /// The required substring.
        needle: String,
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when the wait completed.
        reached: crate::MachineTime,
        /// Matching line index when the query returned an array of strings.
        line: Option<u64>,
        /// The actual matching line or string.
        matched_text: String,
    },
    /// Result of waiting for one boolean query to reach one target value.
    WaitForQueryBool {
        /// The query path that matched.
        path: String,
        /// The required boolean value.
        value: bool,
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when the wait completed.
        reached: crate::MachineTime,
    },
    /// Result of resolving one query path.
    Query {
        /// Resolved query data.
        result: QueryResult,
    },
    /// Result of listing supported query paths.
    QueryPaths {
        /// Query-path listing response.
        result: QueryPathsResult,
    },
    /// Result of switching the live machine.
    SetMachine {
        /// The variant identifier requested by the script.
        machine: String,
        /// Resolved profile id reached after the switch.
        profile_id: String,
        /// Resolved display name reached after the switch.
        display_name: String,
    },
    /// Result of an autoload-tape sequence.
    AutoloadTape {
        /// Slot the tape was autoloaded from.
        slot: String,
        /// Number of native frames spent waiting for boot.
        boot_frames: u32,
    },
    /// Result of installing one tokenised BASIC program.
    LoadBasicProgram {
        /// Tokenised program length in bytes.
        program_bytes: u16,
        /// Whether the binary drove the editor to `RUN` the program
        /// after installing it.
        ran: bool,
    },
    /// Result of finalising one video recording.
    StopVideoRecording {
        /// Final MP4 file path.
        path: PathBuf,
        /// Total frames captured during the recording window.
        frames: u64,
        /// Approximate duration of the captured clip in milliseconds.
        duration_ms: u64,
        /// Whether the final MP4 contains a muxed audio track.
        has_audio: bool,
    },
    /// Result of querying the AY-3-8912 chip state.
    QueryAy {
        /// Last register index selected by an `OUT (#FFFD)` write.
        /// Reads from `IN (#FFFD)` return `raw[selected_register]`.
        selected_register: u8,
        /// All 16 registers, post-mask. `raw[0..2]` is tone-A period
        /// (12-bit, low byte first); `raw[2..4]` tone-B; `raw[4..6]`
        /// tone-C; `raw[6]` noise period (5-bit); `raw[7]` mixer;
        /// `raw[8..11]` amplitudes A/B/C; `raw[11..13]` envelope
        /// period; `raw[13]` envelope shape; `raw[14..16]` I/O ports.
        raw: Vec<u8>,
        /// Tone-A period (12-bit value built from R0 + R1).
        tone_period_a: u16,
        /// Tone-B period (R2 + R3).
        tone_period_b: u16,
        /// Tone-C period (R4 + R5).
        tone_period_c: u16,
        /// Noise period (5-bit, R6).
        noise_period: u8,
        /// Mixer register (R7). Bits 0..2 disable tone channels
        /// A/B/C; bits 3..5 disable noise; bits 6..7 set port direction.
        mixer: u8,
        /// Channel-A amplitude (R8). Bit 4 selects envelope mode;
        /// bits 0..3 are fixed amplitude.
        amplitude_a: u8,
        /// Channel-B amplitude (R9).
        amplitude_b: u8,
        /// Channel-C amplitude (R10).
        amplitude_c: u8,
        /// Envelope period (16-bit value built from R11 + R12).
        envelope_period: u16,
        /// Envelope shape (R13, 4-bit).
        envelope_shape: u8,
    },
    /// Result of finalising one standalone audio recording.
    StopAudioRecording {
        /// Final WAV file path.
        path: PathBuf,
        /// Per-channel sample count.
        samples: u64,
        /// Sample rate of the captured stream, in Hz.
        sample_rate: u32,
        /// Channel count of the captured stream.
        channels: u8,
        /// Approximate clip duration in milliseconds.
        duration_ms: u64,
    },
    /// Result of a reset step. The `kind` field on
    /// [`ScriptObservation`] is the tag itself (`"reset"`), so the
    /// performed reset kind is reported under `performed` to avoid
    /// the tag-name collision.
    Reset {
        /// The kind of reset that was performed.
        performed: ResetKind,
        /// Machine time after the reset (typically zero).
        reached: crate::MachineTime,
    },
    /// Result of a memory-read step.
    MemoryRead {
        /// Start address that was read.
        addr: u32,
        /// Number of bytes that were read (may be capped below the
        /// requested length).
        len: u32,
        /// Raw bytes in memory order.
        bytes: Vec<u8>,
    },
    /// Result of starting a memory write watch.
    WatchMemoryStart {
        /// Watch range start.
        addr: u32,
        /// Watch range length.
        len: u32,
        /// Capacity (max records the log can hold). Once full the log
        /// stops growing; callers should poll via
        /// [`ScriptStep::WatchMemoryLog`] before that. `0` means the log
        /// is unbounded (the Amiga grows its buffer without limit).
        capacity: u32,
    },
    /// Result of stopping a memory write watch and dropping its log.
    WatchMemoryClear {
        /// `true` when a watch range was configured before the clear.
        had_watch: bool,
        /// Number of records captured between start and clear.
        captured: u32,
    },
    /// Result of `query_cpu` — the machine's CPU register snapshot.
    ///
    /// Carries the per-CPU register file as a JSON value (the shape
    /// `DebugTarget::cpu_state` emits): Z80, 6502 and 68000 each report
    /// their own fields, so this is generic rather than a fixed list.
    QueryCpu {
        /// Machine-specific register snapshot.
        registers: serde_json::Value,
    },
    /// Result of stepping the CPU.
    Step {
        /// Number of instructions actually executed.
        instructions: u32,
        /// Machine-native ticks consumed (Spectrum master half-cycles,
        /// NES / Amiga master ticks).
        ticks: u64,
        /// Program counter after the final step.
        pc: u32,
        /// PC at each instruction boundary, in order.
        pc_trace: Vec<u32>,
        /// Disassembly of the instruction at the final PC, when a
        /// disassembler is wired for this CPU.
        next: Option<String>,
    },
    /// Result of `run_until_pc`.
    RunUntilPc {
        /// `true` when PC reached the target before the budget expired.
        reached: bool,
        /// Final PC.
        pc: u32,
        /// Machine-native ticks consumed.
        ticks: u64,
        /// Number of instructions executed.
        steps: u64,
    },
    /// Result of `load_debug_info`.
    DebugInfoLoaded {
        /// Sidecar that was loaded.
        path: PathBuf,
        /// CPU the build targets, from the sidecar header.
        cpu: String,
        /// Assembler dialect the source was written in.
        dialect: String,
        /// Source files that produced the image.
        sources: Vec<String>,
        /// Number of sections described.
        sections: usize,
        /// Number of symbols available for lookup.
        symbols: usize,
        /// Number of line spans available for lookup.
        lines: usize,
    },
    /// Result of `debug_symbol`.
    DebugSymbol {
        /// Name that was looked up.
        name: String,
        /// Resolved address (or constant value), absent if the name is
        /// unknown or its section has no base.
        addr: Option<u32>,
    },
    /// Result of `run_until_line`.
    RunUntilLine {
        /// Source file asked for.
        file: String,
        /// Source line asked for.
        line: u32,
        /// Address that line assembled to, absent if it emitted no bytes.
        addr: Option<u32>,
        /// `true` when PC reached that address before the budget expired.
        reached: bool,
        /// Final PC.
        pc: u32,
        /// The line the machine actually stopped on — what a debugger
        /// highlights. Equal to the requested line on a clean hit.
        stopped_at: Option<SourceLine>,
        /// Machine-native ticks consumed.
        ticks: u64,
        /// Number of instructions executed.
        steps: u64,
    },
    /// Result of `run_until_any_pc`.
    RunUntilAnyPc {
        /// `true` when PC matched any target before the budget expired.
        reached: bool,
        /// Final PC.
        pc: u32,
        /// Machine-native ticks consumed.
        ticks: u64,
        /// Number of instructions executed.
        steps: u64,
    },
    /// Result of `run_until_mem_change` — watches a list of addresses.
    RunUntilMemChange {
        /// Watched addresses.
        addrs: Vec<u32>,
        /// `true` when one of the watched bytes changed in budget.
        changed: bool,
        /// The address that changed first (when `changed`).
        changed_addr: Option<u32>,
        /// Value of `changed_addr` before the change.
        old: Option<u8>,
        /// Value of `changed_addr` after the change.
        new: Option<u8>,
        /// Machine-native ticks consumed.
        ticks: u64,
        /// Number of instructions executed.
        steps: u64,
        /// Final PC.
        pc: u32,
    },
    /// Result of a `poke_byte` write.
    PokeByte {
        /// Address written.
        addr: u32,
        /// Byte written.
        value: u8,
    },
    /// Result of a `poke_word` write.
    PokeWord {
        /// Address written (low byte).
        addr: u32,
        /// 16-bit value written.
        value: u16,
    },
    /// Result of disassembling memory.
    Disasm {
        /// Starting address.
        addr: u32,
        /// Number of decoded instructions.
        count: u32,
        /// Decoded instructions, in memory order.
        instructions: Vec<DisasmInstruction>,
    },
    /// Result of a port read.
    PortRead {
        /// Port that was read.
        port: u16,
        /// Byte returned by the bus-level handler.
        value: u8,
    },
    /// Result of starting an AY write watch.
    WatchAyStart {
        /// Capacity (max records the AY log can hold).
        capacity: u32,
    },
    /// Result of stopping an AY write watch.
    WatchAyClear {
        /// `true` when an AY watch was configured before the clear.
        had_watch: bool,
        /// Number of records captured between start and clear.
        captured: u32,
    },
    /// Result of fetching the AY write log.
    WatchAyLog {
        /// Total records currently held.
        total_writes: u32,
        /// Number of records returned (after limit + unique).
        returned: u32,
        /// Most-recent entries up to the requested limit, oldest first.
        entries: Vec<AyWriteEntry>,
    },
    /// Result of pressing one key.
    PressKey {
        /// The key that was pressed (echoed back from the request).
        key: String,
        /// Frames the key was held before release.
        hold_frames: u32,
        /// Machine time after the press / hold / release sequence.
        reached: crate::MachineTime,
    },
    /// Result of pressing a key chord.
    PressKeys {
        /// The keys that were held together (echoed back from the request).
        keys: Vec<String>,
        /// Frames the chord was held before release.
        hold_frames: u32,
        /// Machine time after the press / hold / release sequence.
        reached: crate::MachineTime,
    },
    /// Result of typing a string.
    TypeString {
        /// Number of characters that were typed.
        chars_typed: u32,
        /// Machine time after the full string was typed.
        reached: crate::MachineTime,
    },
    /// Result of fetching the memory write log.
    WatchMemoryLog {
        /// Current watch range start, or `None` if no watch is active.
        addr: Option<u32>,
        /// Current watch range length, or `None` if no watch is active.
        len: Option<u32>,
        /// Total number of records currently held.
        total_writes: u32,
        /// Number of records actually returned after filtering,
        /// deduplication, and limiting.
        returned: u32,
        /// Most-recent entries up to the requested limit, in capture
        /// order (oldest first).
        entries: Vec<MemoryWriteEntry>,
    },
}

impl HeadlessScript {
    /// Parses one script from UTF-8 JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid.
    pub fn from_json_str(text: &str) -> Result<Self, ScriptError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Loads one script from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O or JSON parsing fails.
    pub fn from_path(path: &Path) -> Result<Self, ScriptError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json_str(&text)
    }

    /// Executes this script against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes this script and returns any structured observations it emits.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<Vec<ScriptObservation>, ScriptError> {
        let mut observations = Vec::new();
        for step in &self.steps {
            if let Some(observation) = step.execute_collect(session)? {
                observations.push(observation);
            }
        }

        Ok(observations)
    }
}

/// Maximum bytes returned by a single `memory_read` step.
const MEMORY_READ_MAX: u32 = 256;
/// Maximum instructions a single `step` step will execute.
const STEP_MAX: u32 = 100_000;
/// Maximum instructions a single `disasm` step will decode.
const DISASM_MAX: u32 = 256;
/// Default instruction budget for the `run_until_*` steps.
const RUN_UNTIL_MAX_STEPS: u64 = 2_000_000;
/// Default number of entries a `watch_*_log` step returns.
const WATCH_LOG_DEFAULT_LIMIT: u32 = 64;

/// Run one bounded debug-step attempt and report whether the target's
/// monotonic instruction-boundary counter advanced. Targets without that
/// optional counter explicitly guarantee the historical exact-step contract.
/// One step inside a run-until loop.
///
/// Uses [`DebugTarget::step_instruction_no_resync`], so the caller **must**
/// call [`DebugTarget::resync`] once the loop ends. Resyncing per instruction
/// meant a full framebuffer conversion for every step: 200,000 steps took
/// 24.1s with it and 0.7s without (#915).
fn step_debug_target(target: &mut dyn DebugTarget) -> (u64, bool) {
    let before = target.instruction_boundary_count();
    let ticks = target.step_instruction_no_resync();
    let after = target.instruction_boundary_count();
    let completed = match (before, after) {
        (Some(before), Some(after)) => after != before,
        _ => true,
    };
    (ticks, completed)
}

impl ScriptStep {
    /// Executes one script step against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes one script step and returns any structured observation it
    /// produces.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<Option<ScriptObservation>, ScriptError> {
        match self {
            Self::LoadMedia {
                slot,
                kind,
                path,
                writable,
            } => {
                let loaded = read_media_asset(path, (*kind).into())?;
                let mut media = MediaSet::new();
                media.push(
                    MediaImage::new(slot.clone(), (*kind).into(), &loaded.bytes)
                        .writable(*writable),
                );
                session.load_media(&media)?;
                Ok(None)
            }
            Self::MediaTransport { slot, transport } => {
                session.command(&ControlCommand::MediaTransport(
                    crate::MediaTransportCommand::new(slot.clone(), (*transport).into()),
                ))?;
                Ok(None)
            }
            Self::Input { events } => {
                session.queue_inputs(events.iter().cloned());
                Ok(None)
            }
            Self::RunFrames { frames } => {
                let result = session.run_frames(*frames)?;
                Ok(Some(ScriptObservation::RunFrames {
                    frames: *frames,
                    reached: result.reached,
                    stop_reason: result.stop_reason,
                }))
            }
            Self::RunTicks { ticks } => {
                let result = session.run_ticks(*ticks)?;
                Ok(Some(ScriptObservation::RunTicks {
                    ticks: *ticks,
                    reached: result.reached,
                    stop_reason: result.stop_reason,
                }))
            }
            Self::WaitForBoot { max_frames } => {
                let result = session.wait_for_boot(*max_frames)?;
                Ok(Some(ScriptObservation::WaitForBoot {
                    frames: result.frames,
                    reached: result.reached,
                    reason: result.reason,
                    row: result.row,
                }))
            }
            Self::WaitForQueryContains {
                path,
                needle,
                max_frames,
            } => {
                let result = session.wait_for_query_text_contains(path, needle, *max_frames)?;
                Ok(Some(ScriptObservation::WaitForQueryContains {
                    path: result.path,
                    needle: result.needle,
                    frames: result.frames,
                    reached: result.reached,
                    line: result.line,
                    matched_text: result.matched_text,
                }))
            }
            Self::WaitForQueryBool {
                path,
                value,
                max_frames,
            } => {
                let result = session.wait_for_query_bool(path, *value, *max_frames)?;
                Ok(Some(ScriptObservation::WaitForQueryBool {
                    path: result.path,
                    value: result.expected,
                    frames: result.frames,
                    reached: result.reached,
                }))
            }
            Self::Query { path } => {
                let result = session.query(path)?;
                Ok(Some(ScriptObservation::Query { result }))
            }
            Self::QueryPaths { prefix } => {
                let result = session.query_paths(prefix.as_deref());
                Ok(Some(ScriptObservation::QueryPaths { result }))
            }
            Self::LoadSnapshot { path } => {
                let bytes = std::fs::read(path)?;
                session.restore_snapshot(&bytes)?;
                Ok(None)
            }
            Self::SaveSnapshot { path } => {
                session.save_snapshot(path)?;
                Ok(None)
            }
            Self::SaveScreenshot { path } => {
                session.save_screenshot(path)?;
                Ok(None)
            }
            Self::SaveAudioCapture { path, reset_after } => {
                session.save_audio_capture(path)?;
                if *reset_after {
                    session.clear_audio_capture();
                }
                Ok(None)
            }
            Self::ClearAudioCapture => {
                session.clear_audio_capture();
                Ok(None)
            }
            Self::SetMachine { .. } => Err(ScriptError::SystemSpecificStep {
                step: "set_machine",
            }),
            Self::QueryAy => Err(ScriptError::SystemSpecificStep { step: "query_ay" }),
            // CPU/memory/disassembly debug verbs run generically through the
            // shared `DebugTarget`, so MCP and `--script` execute the identical
            // body (the MCP tools are `ScriptStepTool` wrappers over these).
            // Machines exposing no debug target fall back to the system-specific
            // error.
            Self::QueryCpu => {
                let Some(target) = session.machine().debug_target() else {
                    return Err(ScriptError::SystemSpecificStep { step: "query_cpu" });
                };
                Ok(Some(ScriptObservation::QueryCpu {
                    registers: target.cpu_state(),
                }))
            }
            Self::Step { instructions } => {
                let count = instructions.unwrap_or(1).min(STEP_MAX);
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep { step: "step" });
                };
                let mut ticks = 0u64;
                let mut pc_trace = Vec::with_capacity(count as usize);
                let mut completed = 0u32;
                for _ in 0..count {
                    let (step_ticks, instruction_completed) = step_debug_target(target);
                    ticks += step_ticks;
                    if !instruction_completed {
                        break;
                    }
                    completed += 1;
                    pc_trace.push(target.pc());
                }
                let pc = target.pc();
                let next = target.disassemble(pc).map(|(text, _)| text);
                // Derived state was left behind while stepping; bring it
                // back before anything can read it (#915).
                target.resync();
                Ok(Some(ScriptObservation::Step {
                    instructions: completed,
                    ticks,
                    pc,
                    pc_trace,
                    next,
                }))
            }
            Self::RunUntilPc { addr, max_steps } => {
                let budget = max_steps.unwrap_or(RUN_UNTIL_MAX_STEPS);
                let target_pc = *addr;
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "run_until_pc",
                    });
                };
                let mut ticks = 0u64;
                let mut steps = 0u64;
                let mut reached = false;
                while steps < budget {
                    if target.pc() == target_pc {
                        reached = true;
                        break;
                    }
                    let (step_ticks, instruction_completed) = step_debug_target(target);
                    ticks += step_ticks;
                    if !instruction_completed {
                        break;
                    }
                    steps += 1;
                }
                reached |= target.pc() == target_pc;
                // Derived state was left behind while stepping; bring it
                // back before anything can read it (#915).
                target.resync();
                Ok(Some(ScriptObservation::RunUntilPc {
                    reached,
                    pc: target.pc(),
                    ticks,
                    steps,
                }))
            }
            Self::LoadDebugInfo {
                path,
                section_bases,
            } => {
                let mut symbols = DebugSymbols::load(path)?;
                for (section, base) in section_bases {
                    symbols.set_section_base(*section, *base);
                }
                let header = symbols.header();
                let (sections, symbol_count, lines) = symbols.counts();
                let observation = ScriptObservation::DebugInfoLoaded {
                    path: path.clone(),
                    cpu: header.cpu.clone(),
                    dialect: header.dialect.clone(),
                    sources: header.sources.clone(),
                    sections,
                    symbols: symbol_count,
                    lines,
                };
                session.set_debug_symbols(Some(symbols));
                Ok(Some(observation))
            }
            Self::DebugSymbol { name } => {
                let Some(symbols) = session.debug_symbols() else {
                    return Err(ScriptError::NoDebugInfo {
                        step: "debug_symbol",
                    });
                };
                Ok(Some(ScriptObservation::DebugSymbol {
                    name: name.clone(),
                    addr: symbols.addr_of(name),
                }))
            }
            Self::RunUntilLine {
                file,
                line,
                max_steps,
            } => {
                let budget = max_steps.unwrap_or(RUN_UNTIL_MAX_STEPS);
                let Some(symbols) = session.debug_symbols() else {
                    return Err(ScriptError::NoDebugInfo {
                        step: "run_until_line",
                    });
                };
                let resolved = symbols.addr_of_line(file, *line);
                let Some(target_pc) = resolved else {
                    // The line emitted no bytes, so there is nowhere to break.
                    // Report that without running the machine: running to the
                    // budget would look like "the line was never reached".
                    let pc = session.machine().debug_target().map_or(0, |t| t.pc());
                    return Ok(Some(ScriptObservation::RunUntilLine {
                        file: file.clone(),
                        line: *line,
                        addr: None,
                        reached: false,
                        pc,
                        stopped_at: None,
                        ticks: 0,
                        steps: 0,
                    }));
                };

                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "run_until_line",
                    });
                };
                let mut ticks = 0u64;
                let mut steps = 0u64;
                let mut reached = false;
                while steps < budget {
                    if target.pc() == target_pc {
                        reached = true;
                        break;
                    }
                    let (step_ticks, instruction_completed) = step_debug_target(target);
                    ticks += step_ticks;
                    if !instruction_completed {
                        break;
                    }
                    steps += 1;
                }
                reached |= target.pc() == target_pc;
                // As `run_until_pc`: derived state was left behind while
                // stepping, so resync before anything reads it (#915).
                target.resync();
                let pc = target.pc();
                // Re-borrowed after the run: this is where the machine
                // actually stopped, which is the line a debugger highlights.
                // On a clean hit it is the requested line; when the budget
                // ran out it is wherever the program got to, which is the
                // more useful answer than repeating what was asked for.
                let stopped_at = session.debug_symbols().and_then(|s| s.line_at(pc));
                Ok(Some(ScriptObservation::RunUntilLine {
                    file: file.clone(),
                    line: *line,
                    addr: Some(target_pc),
                    reached,
                    pc,
                    stopped_at,
                    ticks,
                    steps,
                }))
            }
            Self::RunUntilAnyPc { targets, max_steps } => {
                let budget = max_steps.unwrap_or(RUN_UNTIL_MAX_STEPS);
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "run_until_any_pc",
                    });
                };
                let mut ticks = 0u64;
                let mut steps = 0u64;
                let mut reached = false;
                while steps < budget {
                    if targets.contains(&target.pc()) {
                        reached = true;
                        break;
                    }
                    let (step_ticks, instruction_completed) = step_debug_target(target);
                    ticks += step_ticks;
                    if !instruction_completed {
                        break;
                    }
                    steps += 1;
                }
                reached |= targets.contains(&target.pc());
                // Derived state was left behind while stepping; bring it
                // back before anything can read it (#915).
                target.resync();
                Ok(Some(ScriptObservation::RunUntilAnyPc {
                    reached,
                    pc: target.pc(),
                    ticks,
                    steps,
                }))
            }
            Self::RunUntilMemChange { addrs, max_steps } => {
                let budget = max_steps.unwrap_or(RUN_UNTIL_MAX_STEPS);
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "run_until_mem_change",
                    });
                };
                let initial: Vec<u8> = addrs.iter().map(|a| target.peek(*a)).collect();
                let mut ticks = 0u64;
                let mut steps = 0u64;
                let mut changed_addr = None;
                let mut old = None;
                let mut new = None;
                while steps < budget {
                    let (step_ticks, instruction_completed) = step_debug_target(target);
                    ticks += step_ticks;
                    if instruction_completed {
                        steps += 1;
                    }
                    let mut hit = false;
                    for (i, a) in addrs.iter().enumerate() {
                        let now = target.peek(*a);
                        if now != initial[i] {
                            changed_addr = Some(*a);
                            old = Some(initial[i]);
                            new = Some(now);
                            hit = true;
                            break;
                        }
                    }
                    if hit {
                        break;
                    }
                    if !instruction_completed {
                        break;
                    }
                }
                // Derived state was left behind while stepping; bring it
                // back before anything can read it (#915).
                target.resync();
                Ok(Some(ScriptObservation::RunUntilMemChange {
                    addrs: addrs.clone(),
                    changed: changed_addr.is_some(),
                    changed_addr,
                    old,
                    new,
                    ticks,
                    steps,
                    pc: target.pc(),
                }))
            }
            Self::Disasm { addr, instructions } => {
                let count = instructions.unwrap_or(16).min(DISASM_MAX);
                let Some(target) = session.machine().debug_target() else {
                    return Err(ScriptError::SystemSpecificStep { step: "disasm" });
                };
                let mut decoded = Vec::with_capacity(count as usize);
                let mut a = *addr;
                for _ in 0..count {
                    let Some((mnemonic, len)) = target.disassemble(a) else {
                        break;
                    };
                    let span = u32::from(len.max(1));
                    let raw = (0..span).map(|i| target.peek(a.wrapping_add(i))).collect();
                    decoded.push(DisasmInstruction {
                        addr: a,
                        bytes: len.max(1),
                        raw,
                        mnemonic,
                        symbol: None,
                        source: None,
                    });
                    a = a.wrapping_add(span);
                }
                // Symbolised in a second pass: `target` holds a borrow of the
                // session for the decode loop, and the sidecar lives on the
                // session beside the machine, not inside it.
                if let Some(symbols) = session.debug_symbols() {
                    for instruction in &mut decoded {
                        instruction.symbol = symbols.symbol_at(instruction.addr).map(str::to_owned);
                        instruction.source = symbols.line_at(instruction.addr);
                        instruction.mnemonic = symbols.symbolise(&instruction.mnemonic);
                    }
                }
                Ok(Some(ScriptObservation::Disasm {
                    addr: *addr,
                    count: decoded.len() as u32,
                    instructions: decoded,
                }))
            }
            Self::PortRead { .. } => Err(ScriptError::SystemSpecificStep { step: "port_read" }),
            Self::PortWrite { .. } => Err(ScriptError::SystemSpecificStep { step: "port_write" }),
            // AY register-write watch runs generically through the shared
            // `WatchTarget`, so MCP and `--script` execute the identical body.
            // Machines with no AY surface fall back to the system-specific
            // error (start) or an empty log (clear / log).
            Self::WatchAyStart => {
                let Some(target) = session.machine_mut().watch_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_ay_start",
                    });
                };
                let capacity = target
                    .start_ay_watch()
                    .map_err(|err| ScriptError::InvalidStep {
                        step: "watch_ay_start",
                        reason: err.to_string(),
                    })?;
                Ok(Some(ScriptObservation::WatchAyStart { capacity }))
            }
            Self::WatchAyClear => {
                let Some(target) = session.machine_mut().watch_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_ay_clear",
                    });
                };
                let (had_watch, captured) = target.clear_ay_watch();
                Ok(Some(ScriptObservation::WatchAyClear {
                    had_watch,
                    captured,
                }))
            }
            Self::WatchAyLog { limit, unique } => {
                let limit = limit.unwrap_or(WATCH_LOG_DEFAULT_LIMIT) as usize;
                let Some(target) = session.machine().watch_target() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_ay_log",
                    });
                };
                let Some(records) = target.ay_watch_records() else {
                    return Ok(Some(ScriptObservation::WatchAyLog {
                        total_writes: 0,
                        returned: 0,
                        entries: Vec::new(),
                    }));
                };
                let total_writes = records.len() as u32;
                let mut filtered: Vec<&WatchAyRecord> = records.iter().collect();
                if *unique {
                    let mut seen = std::collections::HashSet::new();
                    filtered.retain(|r| seen.insert((r.pc, r.register, r.value)));
                }
                // Take the most-recent `limit`, restored to oldest-first order.
                let start = filtered.len().saturating_sub(limit);
                let entries: Vec<AyWriteEntry> = filtered[start..]
                    .iter()
                    .map(|r| AyWriteEntry {
                        pc: r.pc,
                        register: r.register,
                        value: r.value,
                    })
                    .collect();
                Ok(Some(ScriptObservation::WatchAyLog {
                    total_writes,
                    returned: entries.len() as u32,
                    entries,
                }))
            }
            // press_key / type_string run generically through the shared
            // `KeyboardTarget`: the machine describes its layout + timing, the
            // session does the key injection. One body for MCP and `--script`
            // on every keyboard machine (RULES.md #30). Machines without a
            // keyboard fall back to the system-specific error.
            Self::PressKey { key, hold_frames } => {
                // Resolve timing and any compound-key expansion under one borrow.
                let (timing, chord) = {
                    let Some(kt) = session.machine().keyboard_target() else {
                        return Err(ScriptError::SystemSpecificStep { step: "press_key" });
                    };
                    // A friendly compound name (e.g. Spectrum `Edit`) expands to
                    // a chord; otherwise the name must be a single valid key.
                    let chord = kt.expand_named_key(key);
                    if chord.is_none() && !kt.key_name_is_valid(key) {
                        return Err(ScriptError::InvalidStep {
                            step: "press_key",
                            reason: format!("unknown key `{key}` — valid: {}", kt.key_names_hint()),
                        });
                    }
                    (kt.key_timing(), chord)
                };
                let hold = hold_frames
                    .unwrap_or(timing.default_hold_frames)
                    .clamp(1, timing.max_hold_frames);
                // A plain key is a chord of one; a compound name presses its
                // whole chord together and releases it in reverse.
                let chord = chord.unwrap_or_else(|| vec![key.clone()]);
                for k in &chord {
                    session.queue_input(crate::host::InputEvent::Key {
                        name: k.clone().into(),
                        pressed: true,
                    });
                }
                session.run_frames(hold)?;
                for k in chord.iter().rev() {
                    session.queue_input(crate::host::InputEvent::Key {
                        name: k.clone().into(),
                        pressed: false,
                    });
                }
                if timing.press_settle_frames > 0 {
                    session.run_frames(timing.press_settle_frames)?;
                }
                Ok(Some(ScriptObservation::PressKey {
                    key: key.clone(),
                    hold_frames: hold,
                    reached: session.time(),
                }))
            }
            Self::PressKeys { keys, hold_frames } => {
                if keys.is_empty() {
                    return Err(ScriptError::InvalidStep {
                        step: "press_keys",
                        reason: "`keys` must list at least one key".to_owned(),
                    });
                }
                let timing = {
                    let Some(kt) = session.machine().keyboard_target() else {
                        return Err(ScriptError::SystemSpecificStep { step: "press_keys" });
                    };
                    if let Some(bad) = keys.iter().find(|k| !kt.key_name_is_valid(k)) {
                        return Err(ScriptError::InvalidStep {
                            step: "press_keys",
                            reason: format!("unknown key `{bad}` — valid: {}", kt.key_names_hint()),
                        });
                    }
                    kt.key_timing()
                };
                let hold = hold_frames
                    .unwrap_or(timing.default_hold_frames)
                    .clamp(1, timing.max_hold_frames);
                // Press the whole chord, hold it together, release in reverse.
                for k in keys {
                    session.queue_input(crate::host::InputEvent::Key {
                        name: k.clone().into(),
                        pressed: true,
                    });
                }
                session.run_frames(hold)?;
                for k in keys.iter().rev() {
                    session.queue_input(crate::host::InputEvent::Key {
                        name: k.clone().into(),
                        pressed: false,
                    });
                }
                if timing.press_settle_frames > 0 {
                    session.run_frames(timing.press_settle_frames)?;
                }
                Ok(Some(ScriptObservation::PressKeys {
                    keys: keys.clone(),
                    hold_frames: hold,
                    reached: session.time(),
                }))
            }
            Self::TypeString {
                text,
                hold_frames,
                settle_frames,
            } => {
                // Translate every character to its key chord up front (under a
                // read-only borrow), then drive the injection on the session.
                let (timing, chords) = {
                    let Some(kt) = session.machine().keyboard_target() else {
                        return Err(ScriptError::SystemSpecificStep {
                            step: "type_string",
                        });
                    };
                    // Refuse rather than skip. `filter_map` here dropped any
                    // character the machine could not type and carried on, so
                    // a script asked for one string and the machine received
                    // another. On the C64 that quietly turned
                    // `H=48+C+(C>9)*57` into `H=48+C+(C9)*57` — still valid
                    // BASIC, still runs, wrong answer (#916). For a scripted,
                    // non-interactive tool, refusing to type is far better
                    // than typing something else.
                    let mut chords: Vec<Vec<String>> = Vec::with_capacity(text.chars().count());
                    for ch in text.chars() {
                        let Some(chord) = kt.keys_for_char(ch) else {
                            return Err(ScriptError::UntypableCharacter {
                                ch,
                                supported: kt.key_names_hint().to_owned(),
                            });
                        };
                        chords.push(chord);
                    }
                    (kt.key_timing(), chords)
                };
                let hold = hold_frames
                    .unwrap_or(timing.default_hold_frames)
                    .clamp(1, timing.max_hold_frames);
                let final_settle = settle_frames.unwrap_or(timing.default_type_settle_frames);
                let mut prev: Option<String> = None;
                let mut typed = 0u32;
                for chord in &chords {
                    let base = chord.last().cloned();
                    // Extra settle before a repeated key so the ROM scan sees
                    // the release between two identical presses.
                    if timing.repeat_settle_frames > 0 && base.is_some() && prev == base {
                        session.run_frames(timing.repeat_settle_frames)?;
                    }
                    for k in chord {
                        session.queue_input(crate::host::InputEvent::Key {
                            name: k.clone().into(),
                            pressed: true,
                        });
                    }
                    session.run_frames(hold)?;
                    for k in chord.iter().rev() {
                        session.queue_input(crate::host::InputEvent::Key {
                            name: k.clone().into(),
                            pressed: false,
                        });
                    }
                    session.run_frames(timing.inter_key_settle_frames)?;
                    prev = base;
                    typed += 1;
                }
                if final_settle > 0 {
                    session.run_frames(final_settle)?;
                }
                Ok(Some(ScriptObservation::TypeString {
                    chars_typed: typed,
                    reached: session.time(),
                }))
            }
            Self::AutoloadTape { .. } => Err(ScriptError::SystemSpecificStep {
                step: "autoload_tape",
            }),
            Self::LoadBasicProgram { .. } => Err(ScriptError::SystemSpecificStep {
                step: "load_basic_program",
            }),
            Self::MemoryRead { addr, len } => {
                // Generic, side-effect-free read through the machine's shared
                // DebugTarget bus view — works for every debug-capable family
                // (6502/Z80/6809 via the debug-primitive macros, Amiga by hand),
                // so a per-binary intercept is no longer required. Machines that
                // expose no debug target fall back to the system-specific error.
                let Some(target) = session.machine().debug_target() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "memory_read",
                    });
                };
                let base = *addr;
                let capped = (*len).min(MEMORY_READ_MAX);
                let bytes = (0..capped)
                    .map(|i| target.peek(base.wrapping_add(i)))
                    .collect();
                Ok(Some(ScriptObservation::MemoryRead {
                    addr: base,
                    len: capped,
                    bytes,
                }))
            }
            Self::PokeByte { addr, value } => {
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep { step: "poke_byte" });
                };
                target.poke(*addr, *value);
                Ok(Some(ScriptObservation::PokeByte {
                    addr: *addr,
                    value: *value,
                }))
            }
            Self::PokeWord { addr, value } => {
                // Little-endian (Z80 / 6502). Big-endian CPUs (the 68000)
                // keep a bespoke poke_word override; this generic path serves
                // the little-endian fleet.
                let Some(target) = session.machine_mut().debug_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep { step: "poke_word" });
                };
                let [lo, hi] = value.to_le_bytes();
                target.poke(*addr, lo);
                target.poke(addr.wrapping_add(1), hi);
                Ok(Some(ScriptObservation::PokeWord {
                    addr: *addr,
                    value: *value,
                }))
            }
            // Memory-write watch runs generically through the shared
            // `WatchTarget` (the Amiga + Spectrum capture buffers), so MCP and
            // `--script` execute the identical body. Machines with no watch
            // surface fall back to the system-specific error / empty log.
            Self::WatchMemoryStart { addr, len } => {
                if *len == 0 {
                    return Err(ScriptError::InvalidStep {
                        step: "watch_memory_start",
                        reason: "`len` must be at least 1".to_owned(),
                    });
                }
                let Some(target) = session.machine_mut().watch_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_memory_start",
                    });
                };
                let capacity = target.start_memory_watch(*addr, *len).map_err(|err| {
                    ScriptError::InvalidStep {
                        step: "watch_memory_start",
                        reason: err.to_string(),
                    }
                })?;
                Ok(Some(ScriptObservation::WatchMemoryStart {
                    addr: *addr,
                    len: *len,
                    capacity,
                }))
            }
            Self::WatchMemoryClear => {
                let Some(target) = session.machine_mut().watch_target_mut() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_memory_clear",
                    });
                };
                let (had_watch, captured) = target.clear_memory_watch();
                Ok(Some(ScriptObservation::WatchMemoryClear {
                    had_watch,
                    captured,
                }))
            }
            Self::WatchMemoryLog {
                limit,
                unique,
                source,
                cck_min,
                cck_max,
            } => {
                if cck_min.zip(*cck_max).is_some_and(|(min, max)| min > max) {
                    return Err(ScriptError::InvalidStep {
                        step: "watch_memory_log",
                        reason: "`cck_min` must not exceed `cck_max`".to_owned(),
                    });
                }
                let limit = limit.unwrap_or(WATCH_LOG_DEFAULT_LIMIT) as usize;
                let Some(target) = session.machine().watch_target() else {
                    return Err(ScriptError::SystemSpecificStep {
                        step: "watch_memory_log",
                    });
                };
                let range = target.memory_watch_range();
                let Some(records) = target.memory_watch_records() else {
                    return Ok(Some(ScriptObservation::WatchMemoryLog {
                        addr: None,
                        len: None,
                        total_writes: 0,
                        returned: 0,
                        entries: Vec::new(),
                    }));
                };
                let total_writes = records.len() as u32;
                let mut filtered: Vec<&WatchMemoryRecord> = records.iter().collect();
                if let Some(source) = source {
                    filtered.retain(|record| record.source == Some(*source));
                }
                if let Some(cck_min) = cck_min {
                    filtered.retain(|record| record.cck.is_some_and(|cck| cck >= *cck_min));
                }
                if let Some(cck_max) = cck_max {
                    filtered.retain(|record| record.cck.is_some_and(|cck| cck <= *cck_max));
                }
                if *unique {
                    let mut seen = std::collections::HashSet::new();
                    filtered.retain(|r| seen.insert((r.pc, r.addr, r.value, r.source)));
                }
                // Take the most-recent `limit`, restored to oldest-first order.
                let start = filtered.len().saturating_sub(limit);
                let entries: Vec<MemoryWriteEntry> = filtered[start..]
                    .iter()
                    .map(|r| MemoryWriteEntry {
                        pc: r.pc,
                        addr: r.addr,
                        value: r.value,
                        cck: r.cck,
                        size_bytes: r.size_bytes,
                        source: r.source,
                    })
                    .collect();
                Ok(Some(ScriptObservation::WatchMemoryLog {
                    addr: range.map(|(lo, _)| lo),
                    len: range.map(|(_, len)| len),
                    total_writes,
                    returned: entries.len() as u32,
                    entries,
                }))
            }
            Self::StartVideoRecording { path } => {
                session.start_video_recording(path.clone())?;
                Ok(None)
            }
            Self::StopVideoRecording => {
                let summary = session.stop_video_recording()?;
                Ok(Some(ScriptObservation::StopVideoRecording {
                    path: summary.path,
                    frames: summary.frames,
                    duration_ms: summary.duration_ms,
                    has_audio: summary.has_audio,
                }))
            }
            Self::Reset { kind } => {
                session.reset(*kind)?;
                Ok(Some(ScriptObservation::Reset {
                    performed: *kind,
                    reached: session.time(),
                }))
            }
            Self::StartAudioRecording { path } => {
                session.start_audio_recording(path.clone())?;
                Ok(None)
            }
            Self::StopAudioRecording => {
                let summary = session.stop_audio_recording()?;
                Ok(Some(ScriptObservation::StopAudioRecording {
                    path: summary.path,
                    samples: summary.samples as u64,
                    sample_rate: summary.sample_rate,
                    channels: summary.channels,
                    duration_ms: summary.duration_ms,
                }))
            }
        }
    }
}

/// Error surfaced by the shared JSON script layer.
#[derive(Debug, Error)]
pub enum ScriptError {
    /// Asset loading or archive extraction failed.
    #[error(transparent)]
    Asset(#[from] AssetLoadError),

    /// One filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON parsing failed.
    #[error(transparent)]
    Parse(#[from] serde_json::Error),

    /// Session execution failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Query resolution failed.
    #[error(transparent)]
    Query(#[from] QueryError),

    /// `type_string` was given a character this machine's keyboard cannot
    /// produce. Reported rather than skipped: a script that asks for text and
    /// silently receives different text is worse than one that fails.
    #[error(
        "`type_string` cannot type {ch:?} on this machine — it has no keycap \
         or shift chord for it. Supported keys: {supported}"
    )]
    UntypableCharacter { ch: char, supported: String },

    /// A Debug198x sidecar could not be loaded.
    #[error(transparent)]
    DebugInfo(#[from] DebugInfoError),

    /// A step needing symbols ran with no sidecar attached. Reported rather
    /// than treated as "symbol not found": the two have different fixes, and
    /// a source-line breakpoint that silently never fires because nobody
    /// called `load_debug_info` is a bad half-hour.
    #[error("script step `{step}` needs debug info — run `load_debug_info` first")]
    NoDebugInfo {
        /// The step's serde tag (e.g. `"run_until_line"`).
        step: &'static str,
    },

    /// One step requires a binary-side handler the shell crate does
    /// not own (e.g. `SetMachine`, `AutoloadTape`). Per-system binaries
    /// intercept these steps before delegating to the shell executor.
    #[error("script step `{step}` requires a system-specific handler")]
    SystemSpecificStep {
        /// The step's serde tag (e.g. `"set_machine"`, `"autoload_tape"`).
        step: &'static str,
    },

    /// A step's arguments were rejected by the active machine — e.g. a
    /// zero-length watch range, or an address outside the CPU's space.
    #[error("script step `{step}` rejected: {reason}")]
    InvalidStep {
        /// The step's serde tag (e.g. `"watch_memory_start"`).
        step: &'static str,
        /// Why the machine rejected the request.
        reason: String,
    },
}

const fn default_true() -> bool {
    true
}

/// Matches the `len` default the `memory_read` MCP schema advertises.
const fn default_memory_read_len() -> u32 {
    16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::error::MachineError;
    use crate::host::{AudioPacket, FramePacket, HostIo, PixelFormat};
    use crate::machine::{
        Family, MachineId, MachineProfile, ProfileId, Region, ResetKind, RunResult, StopReason,
    };
    use crate::media::{FirmwareRequirement, MediaSlot, WritebackPolicy};
    use crate::query::SessionQueryProvider;
    use crate::time::{ClockDesc, ClockRate, MachineTime};
    use serde_json::json;

    struct DummyMachine {
        profile: MachineProfile,
        time: MachineTime,
        tape_loaded: usize,
        commands: usize,
        restored: usize,
        /// Counts `dbg_resync` calls, so tests can assert that every stepping
        /// verb puts derived state back before returning (#915).
        resyncs: usize,
        ram: Vec<u8>,
        mem_watch: Option<(u32, u32)>,
        mem_log: Vec<crate::watch::WatchMemoryRecord>,
        ay_watching: bool,
        ay_log: Vec<crate::watch::WatchAyRecord>,
        instruction_starts: u64,
        step_completes: bool,
        step_ticks: u64,
    }

    impl DummyMachine {
        fn new() -> Self {
            Self {
                profile: MachineProfile {
                    machine_id: MachineId::from("dummy-machine"),
                    profile_id: ProfileId::from("dummy-profile"),
                    display_name: "Dummy".into(),
                    family: Family::Spectrum,
                    region: Region::Pal,
                    release_year: 1982,
                    summary: "dummy".into(),
                    clock: ClockDesc::new("master-cycle", ClockRate::from_hz(1)),
                    firmware: vec![FirmwareRequirement::new("rom-0", "ROM 0", false)],
                    media_slots: vec![MediaSlot::new(
                        "tape-1",
                        "Tape Deck",
                        MediaKind::Tape,
                        false,
                        WritebackPolicy::InMemoryOnly,
                    )],
                    capabilities: CapabilitySet::new(),
                },
                time: MachineTime::default(),
                tape_loaded: 0,
                commands: 0,
                restored: 0,
                resyncs: 0,
                ram: vec![0u8; 0x1_0000],
                mem_watch: None,
                mem_log: Vec::new(),
                ay_watching: false,
                ay_log: Vec::new(),
                instruction_starts: 0,
                step_completes: true,
                step_ticks: 0,
            }
        }
    }

    // A trivial debug surface over the dummy machine's RAM, so the generic
    // `memory_read` step has a `DebugTarget` to read through.
    impl crate::debug::DebugPrimitives for DummyMachine {
        fn dbg_resync(&mut self) {
            self.resyncs += 1;
        }
        fn dbg_pc(&self) -> u32 {
            0
        }
        fn dbg_peek(&self, addr: u32) -> u8 {
            self.ram.get(addr as usize).copied().unwrap_or(0)
        }
        fn dbg_poke(&mut self, addr: u32, value: u8) {
            if let Some(slot) = self.ram.get_mut(addr as usize) {
                *slot = value;
            }
            // Capture into the memory-write watch log when armed and in range,
            // so the generic `watch_memory_*` arms have something to report.
            if let Some((lo, len)) = self.mem_watch
                && addr >= lo
                && addr < lo.wrapping_add(len)
            {
                self.mem_log.push(crate::watch::WatchMemoryRecord {
                    pc: 0,
                    addr,
                    value: u32::from(value),
                    cck: None,
                    size_bytes: 1,
                    source: None,
                });
            }
        }
        fn dbg_cpu_state(&self) -> serde_json::Value {
            json!({"instruction_starts": self.instruction_starts})
        }
        fn dbg_instruction_boundary_count(&self) -> Option<u64> {
            Some(self.instruction_starts)
        }
        fn dbg_disassemble(&self, _addr: u32) -> Option<(String, u8)> {
            None
        }
        fn dbg_step(&mut self) -> u64 {
            if self.step_completes {
                self.instruction_starts = self.instruction_starts.wrapping_add(1);
            }
            self.step_ticks
        }
    }

    impl MachineCore for DummyMachine {
        fn profile(&self) -> &MachineProfile {
            &self.profile
        }

        fn time(&self) -> MachineTime {
            self.time
        }

        fn reset(&mut self, _kind: ResetKind) {}

        fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
            self.tape_loaded += media.images.len();
            Ok(())
        }

        fn run_until(
            &mut self,
            target: MachineTime,
            host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            self.time = target;
            host.frame_sink.push_frame(FramePacket {
                timestamp: target,
                format: PixelFormat::Indexed8,
                width: 1,
                height: 1,
                palette: Some(&[0x000000FF, 0xFFFFFFFF]),
                pixels: &[1],
            })?;
            host.audio_sink.push_audio(AudioPacket {
                timestamp: target,
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0, 0.25],
            })?;
            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }

        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![0x55])
        }

        fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
            self.restored += 1;
            Ok(())
        }

        fn command(&mut self, _command: &ControlCommand) -> Result<(), MachineError> {
            self.commands += 1;
            Ok(())
        }

        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::new()
        }

        fn debug_target(&self) -> Option<&dyn crate::debug::DebugTarget> {
            Some(self)
        }
        fn debug_target_mut(&mut self) -> Option<&mut dyn crate::debug::DebugTarget> {
            Some(self)
        }
        fn watch_target(&self) -> Option<&dyn crate::watch::WatchTarget> {
            Some(self)
        }
        fn watch_target_mut(&mut self) -> Option<&mut dyn crate::watch::WatchTarget> {
            Some(self)
        }
        fn keyboard_target(&self) -> Option<&dyn crate::keyboard::KeyboardTarget> {
            Some(self)
        }
    }

    // A trivial keyboard: letters (uppercase keycap), Space, Enter. Enough to
    // exercise the generic `press_key` / `type_string` arms.
    impl crate::keyboard::KeyboardTarget for DummyMachine {
        fn key_name_is_valid(&self, name: &str) -> bool {
            matches!(name, "Space" | "Enter")
                || (name.len() == 1 && name.chars().all(|c| c.is_ascii_uppercase()))
        }
        fn key_names_hint(&self) -> &'static str {
            "A-Z, Space, Enter"
        }
        fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
            match ch {
                'a'..='z' | 'A'..='Z' => Some(vec![ch.to_ascii_uppercase().to_string()]),
                ' ' => Some(vec!["Space".to_owned()]),
                '\n' => Some(vec!["Enter".to_owned()]),
                _ => None,
            }
        }
        fn key_timing(&self) -> crate::keyboard::KeyTiming {
            crate::keyboard::KeyTiming {
                default_hold_frames: 3,
                max_hold_frames: 600,
                press_settle_frames: 1,
                inter_key_settle_frames: 1,
                repeat_settle_frames: 3,
                default_type_settle_frames: 5,
            }
        }
        // `Combo` is not a valid single key, so a successful press_key proves
        // the compound-key expansion path ran.
        fn expand_named_key(&self, name: &str) -> Option<Vec<String>> {
            (name == "Combo").then(|| vec!["A".to_owned(), "B".to_owned()])
        }
    }

    // A trivial watch surface: memory captures on poke (see `dbg_poke`); AY
    // captures one synthetic record per `start` so both verb families are
    // exercised generically.
    impl crate::watch::WatchTarget for DummyMachine {
        fn supports_memory_watch(&self) -> bool {
            true
        }
        fn start_memory_watch(
            &mut self,
            addr: u32,
            len: u32,
        ) -> Result<u32, crate::watch::WatchError> {
            self.mem_watch = Some((addr, len));
            self.mem_log.clear();
            Ok(256)
        }
        fn clear_memory_watch(&mut self) -> (bool, u32) {
            let had = self.mem_watch.take().is_some();
            let captured = self.mem_log.len() as u32;
            self.mem_log.clear();
            (had, captured)
        }
        fn memory_watch_range(&self) -> Option<(u32, u32)> {
            self.mem_watch
        }
        fn memory_watch_records(&self) -> Option<Vec<crate::watch::WatchMemoryRecord>> {
            self.mem_watch.map(|_| self.mem_log.clone())
        }

        fn supports_ay_watch(&self) -> bool {
            true
        }
        fn start_ay_watch(&mut self) -> Result<u32, crate::watch::WatchError> {
            self.ay_watching = true;
            self.ay_log = vec![crate::watch::WatchAyRecord {
                pc: 0,
                register: 7,
                value: 0x3E,
            }];
            Ok(128)
        }
        fn clear_ay_watch(&mut self) -> (bool, u32) {
            let had = self.ay_watching;
            let captured = self.ay_log.len() as u32;
            self.ay_watching = false;
            self.ay_log.clear();
            (had, captured)
        }
        fn ay_watch_records(&self) -> Option<Vec<crate::watch::WatchAyRecord>> {
            self.ay_watching.then(|| self.ay_log.clone())
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct DummyQueryProvider;

    impl SessionQueryProvider<DummyMachine> for DummyQueryProvider {
        fn query(
            &self,
            machine: &DummyMachine,
            path: &str,
        ) -> Result<Option<QueryResult>, QueryError> {
            match path {
                "boot.detected" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get() >= 3 * 69_888),
                })),
                "boot.reason" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 3 * 69_888 {
                        "dummy boot banner is visible"
                    } else {
                        "dummy boot banner not visible yet"
                    }),
                })),
                "boot.row" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 3 * 69_888 {
                        Some(23u64)
                    } else {
                        None::<u64>
                    }),
                })),
                "dummy.flag" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get() >= 2 * 69_888),
                })),
                "screen.text.lines" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 4 * 69_888 {
                        vec!["READY".to_owned(), "MANIC MINER".to_owned()]
                    } else {
                        vec!["READY".to_owned(), "LOADING".to_owned()]
                    }),
                })),
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn memory_read_step_reads_through_the_debug_target() {
        let mut machine = DummyMachine::new();
        machine.ram[0xC000] = 0x42;
        machine.ram[0xC001] = 0x99;
        let mut session = HeadlessSession::new(machine, 1);

        let obs = ScriptStep::MemoryRead {
            addr: 0xC000,
            len: 4,
        }
        .execute_collect(&mut session)
        .expect("memory_read should execute")
        .expect("memory_read should emit an observation");

        match obs {
            ScriptObservation::MemoryRead { addr, len, bytes } => {
                assert_eq!(addr, 0xC000);
                assert_eq!(len, 4);
                assert_eq!(bytes, vec![0x42, 0x99, 0x00, 0x00]);
            }
            other => panic!("expected a MemoryRead observation, got {other:?}"),
        }
    }

    #[test]
    fn memory_read_len_defaults_to_sixteen() {
        // The MCP schema advertises `"default": 16` and lists only `addr` as
        // required; before #905 the field had no serde default, so omitting it
        // failed to deserialise with `missing field len`.
        let step: ScriptStep =
            serde_json::from_str(r#"{"action":"memory_read","addr":49152}"#).expect("deserialises");
        match step {
            ScriptStep::MemoryRead { addr, len } => {
                assert_eq!(addr, 0xC000);
                assert_eq!(len, 16);
            }
            other => panic!("expected a MemoryRead step, got {other:?}"),
        }
    }

    #[test]
    fn memory_read_step_caps_length_at_256() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 1);
        let obs = ScriptStep::MemoryRead {
            addr: 0x0000,
            len: 1000,
        }
        .execute_collect(&mut session)
        .expect("memory_read should execute")
        .expect("memory_read should emit an observation");
        match obs {
            ScriptObservation::MemoryRead { len, bytes, .. } => {
                assert_eq!(len, 256);
                assert_eq!(bytes.len(), 256);
            }
            other => panic!("expected a MemoryRead observation, got {other:?}"),
        }
    }

    #[test]
    fn debug_verb_steps_run_generically_through_the_debug_target() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 1);
        let run = |session: &mut HeadlessSession<DummyMachine, _>, step: ScriptStep| {
            step.execute_collect(session)
                .expect("step executes")
                .expect("step emits an observation")
        };

        // query_cpu → carries the machine's CPU-specific state object.
        match run(&mut session, ScriptStep::QueryCpu) {
            ScriptObservation::QueryCpu { registers } => assert!(registers.is_object()),
            other => panic!("expected QueryCpu, got {other:?}"),
        }

        // poke_byte writes through the debug target, then memory_read sees it.
        run(
            &mut session,
            ScriptStep::PokeByte {
                addr: 0x4000,
                value: 0xAB,
            },
        );
        match run(
            &mut session,
            ScriptStep::MemoryRead {
                addr: 0x4000,
                len: 1,
            },
        ) {
            ScriptObservation::MemoryRead { bytes, .. } => assert_eq!(bytes, vec![0xAB]),
            other => panic!("expected MemoryRead, got {other:?}"),
        }

        // step → pc_trace has one entry per requested instruction.
        match run(
            &mut session,
            ScriptStep::Step {
                instructions: Some(3),
            },
        ) {
            ScriptObservation::Step {
                instructions,
                pc_trace,
                ..
            } => {
                assert_eq!(instructions, 3);
                assert_eq!(pc_trace.len(), 3, "one PC traced per step");
            }
            other => panic!("expected Step, got {other:?}"),
        }

        // run_until_pc: DummyMachine PC is 0, so target 0 is reached at once.
        match run(
            &mut session,
            ScriptStep::RunUntilPc {
                addr: 0,
                max_steps: Some(4),
            },
        ) {
            ScriptObservation::RunUntilPc { reached, .. } => assert!(reached),
            other => panic!("expected RunUntilPc, got {other:?}"),
        }

        // run_until_any_pc: 0 is in the target set → reached.
        match run(
            &mut session,
            ScriptStep::RunUntilAnyPc {
                targets: vec![0x1234, 0],
                max_steps: Some(4),
            },
        ) {
            ScriptObservation::RunUntilAnyPc { reached, .. } => assert!(reached),
            other => panic!("expected RunUntilAnyPc, got {other:?}"),
        }

        // run_until_mem_change: nothing changes the byte, so it runs the budget
        // and reports changed=false over the watched list.
        match run(
            &mut session,
            ScriptStep::RunUntilMemChange {
                addrs: vec![0x4000, 0x4001],
                max_steps: Some(4),
            },
        ) {
            ScriptObservation::RunUntilMemChange {
                changed,
                addrs,
                steps,
                ..
            } => {
                assert!(!changed);
                assert_eq!(addrs, vec![0x4000, 0x4001]);
                assert_eq!(steps, 4);
            }
            other => panic!("expected RunUntilMemChange, got {other:?}"),
        }
    }

    #[test]
    fn bounded_debug_attempt_does_not_claim_an_unfinished_instruction() {
        let mut machine = DummyMachine::new();
        machine.step_completes = false;
        machine.step_ticks = 1_000_000;
        let mut session = HeadlessSession::new(machine, 1);

        let observation = ScriptStep::Step {
            instructions: Some(3),
        }
        .execute_collect(&mut session)
        .expect("bounded step should execute")
        .expect("bounded step should emit an observation");

        match observation {
            ScriptObservation::Step {
                instructions,
                ticks,
                pc_trace,
                ..
            } => {
                assert_eq!(instructions, 0);
                assert_eq!(ticks, 1_000_000);
                assert!(pc_trace.is_empty());
            }
            other => panic!("expected Step, got {other:?}"),
        }
    }

    #[test]
    fn every_stepping_verb_resyncs_derived_state_before_returning() {
        // The run-until loops step with `step_instruction_no_resync`, because
        // resyncing per instruction meant a full framebuffer conversion for
        // every step — 200,000 steps took 24.1s with it and 0.7s without
        // (#915). That speed is only safe if the loop resyncs when it stops.
        //
        // Counting the resyncs is what makes forgetting one fail here rather
        // than showing up as a stale screenshot much later.
        for (name, step) in [
            (
                "step",
                ScriptStep::Step {
                    instructions: Some(4),
                },
            ),
            (
                "run_until_pc",
                ScriptStep::RunUntilPc {
                    addr: 0xFFFF,
                    max_steps: Some(4),
                },
            ),
            (
                "run_until_any_pc",
                ScriptStep::RunUntilAnyPc {
                    targets: vec![0xFFFF],
                    max_steps: Some(4),
                },
            ),
            (
                "run_until_mem_change",
                ScriptStep::RunUntilMemChange {
                    addrs: vec![0x1234],
                    max_steps: Some(4),
                },
            ),
        ] {
            let mut session = HeadlessSession::new(DummyMachine::new(), 1);
            step.execute_collect(&mut session).expect("step executes");
            assert_eq!(
                session.machine().resyncs,
                1,
                "`{name}` must resync exactly once when it stops"
            );
        }
    }

    #[test]
    fn keyboard_verbs_run_generically_through_the_keyboard_target() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 1);
        let run = |session: &mut HeadlessSession<DummyMachine, _>, step: ScriptStep| {
            step.execute_collect(session).expect("step executes")
        };

        // press_key with a valid name → PressKey observation, default hold 3.
        match run(
            &mut session,
            ScriptStep::PressKey {
                key: "A".to_owned(),
                hold_frames: None,
            },
        )
        .expect("emits observation")
        {
            ScriptObservation::PressKey {
                key, hold_frames, ..
            } => {
                assert_eq!(key, "A");
                assert_eq!(hold_frames, 3);
            }
            other => panic!("expected PressKey, got {other:?}"),
        }

        // press_key with an unknown name → InvalidStep, not a silent no-op.
        let invalid = ScriptStep::PressKey {
            key: "nope".to_owned(),
            hold_frames: None,
        };
        match invalid.execute_collect(&mut session) {
            Err(ScriptError::InvalidStep { step, .. }) => assert_eq!(step, "press_key"),
            other => panic!("expected InvalidStep, got {other:?}"),
        }

        // press_key with a compound name expands to a chord: `Combo` is not a
        // valid single key, so success proves it routed through expansion.
        match run(
            &mut session,
            ScriptStep::PressKey {
                key: "Combo".to_owned(),
                hold_frames: None,
            },
        )
        .expect("compound name expands instead of erroring")
        {
            ScriptObservation::PressKey { key, .. } => assert_eq!(key, "Combo"),
            other => panic!("expected PressKey, got {other:?}"),
        }

        // press_keys holds a chord of valid keys together.
        match run(
            &mut session,
            ScriptStep::PressKeys {
                keys: vec!["A".to_owned(), "B".to_owned()],
                hold_frames: Some(5),
            },
        )
        .expect("emits observation")
        {
            ScriptObservation::PressKeys {
                keys, hold_frames, ..
            } => {
                assert_eq!(keys, vec!["A".to_owned(), "B".to_owned()]);
                assert_eq!(hold_frames, 5);
            }
            other => panic!("expected PressKeys, got {other:?}"),
        }

        // press_keys rejects a chord containing an unknown key.
        let bad_chord = ScriptStep::PressKeys {
            keys: vec!["A".to_owned(), "nope".to_owned()],
            hold_frames: None,
        };
        match bad_chord.execute_collect(&mut session) {
            Err(ScriptError::InvalidStep { step, .. }) => assert_eq!(step, "press_keys"),
            other => panic!("expected InvalidStep, got {other:?}"),
        }

        // type_string types every character it was given.
        match run(
            &mut session,
            ScriptStep::TypeString {
                text: "aB ".to_owned(),
                hold_frames: None,
                settle_frames: None,
            },
        )
        .expect("emits observation")
        {
            ScriptObservation::TypeString { chars_typed, .. } => assert_eq!(chars_typed, 3),
            other => panic!("expected TypeString, got {other:?}"),
        }

        // ...and refuses one it cannot. This assertion used to read the other
        // way round — `"aB !"` typed three characters and dropped the `!`
        // without complaint, which is the behaviour #916 was filed against.
        // The dummy target here supports only A-Z, Space and Enter, so `!` is
        // untypable on it exactly as the eight missing symbols were on a real
        // C64.
        match (ScriptStep::TypeString {
            text: "aB !".to_owned(),
            hold_frames: None,
            settle_frames: None,
        })
        .execute_collect(&mut session)
        {
            Err(ScriptError::UntypableCharacter { ch, .. }) => assert_eq!(ch, '!'),
            other => panic!("expected UntypableCharacter, got {other:?}"),
        }
    }

    #[test]
    fn watch_verbs_run_generically_through_the_watch_target() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 1);
        let run = |session: &mut HeadlessSession<DummyMachine, _>, step: ScriptStep| {
            step.execute_collect(session)
                .expect("step executes")
                .expect("step emits an observation")
        };

        // Arm a memory watch, drive a write through poke_byte, see it logged.
        match run(
            &mut session,
            ScriptStep::WatchMemoryStart {
                addr: 0x4000,
                len: 4,
            },
        ) {
            ScriptObservation::WatchMemoryStart {
                addr,
                len,
                capacity,
            } => {
                assert_eq!((addr, len), (0x4000, 4));
                assert_eq!(capacity, 256);
            }
            other => panic!("expected WatchMemoryStart, got {other:?}"),
        }
        run(
            &mut session,
            ScriptStep::PokeByte {
                addr: 0x4001,
                value: 0x99,
            },
        );
        // A poke outside the range is not captured.
        run(
            &mut session,
            ScriptStep::PokeByte {
                addr: 0x5000,
                value: 0x11,
            },
        );
        match run(
            &mut session,
            ScriptStep::WatchMemoryLog {
                limit: None,
                unique: false,
                source: None,
                cck_min: None,
                cck_max: None,
            },
        ) {
            ScriptObservation::WatchMemoryLog {
                addr,
                len,
                total_writes,
                entries,
                ..
            } => {
                assert_eq!((addr, len), (Some(0x4000), Some(4)));
                assert_eq!(total_writes, 1, "only the in-range write is captured");
                assert_eq!(entries.len(), 1);
                assert_eq!((entries[0].addr, entries[0].value), (0x4001, 0x99));
                assert_eq!(entries[0].size_bytes, 1);
                assert_eq!(entries[0].source, None);
            }
            other => panic!("expected WatchMemoryLog, got {other:?}"),
        }
        match run(&mut session, ScriptStep::WatchMemoryClear) {
            ScriptObservation::WatchMemoryClear {
                had_watch,
                captured,
            } => {
                assert!(had_watch);
                assert_eq!(captured, 1);
            }
            other => panic!("expected WatchMemoryClear, got {other:?}"),
        }

        // AY watch: start seeds one record, log returns it, clear drops it.
        match run(&mut session, ScriptStep::WatchAyStart) {
            ScriptObservation::WatchAyStart { capacity } => assert_eq!(capacity, 128),
            other => panic!("expected WatchAyStart, got {other:?}"),
        }
        match run(
            &mut session,
            ScriptStep::WatchAyLog {
                limit: None,
                unique: false,
            },
        ) {
            ScriptObservation::WatchAyLog { entries, .. } => {
                assert_eq!(entries.len(), 1);
                assert_eq!((entries[0].register, entries[0].value), (7, 0x3E));
            }
            other => panic!("expected WatchAyLog, got {other:?}"),
        }
        match run(&mut session, ScriptStep::WatchAyClear) {
            ScriptObservation::WatchAyClear {
                had_watch,
                captured,
            } => {
                assert!(had_watch);
                assert_eq!(captured, 1);
            }
            other => panic!("expected WatchAyClear, got {other:?}"),
        }
    }

    #[test]
    fn memory_watch_log_filters_by_source_and_inclusive_cck_window() {
        let mut machine = DummyMachine::new();
        machine.mem_watch = Some((0x78000, 8));
        machine.mem_log = vec![
            WatchMemoryRecord {
                pc: 0x100,
                addr: 0x78000,
                value: 0x1111,
                cck: Some(100),
                size_bytes: 2,
                source: Some(WatchMemorySource::Cpu),
            },
            WatchMemoryRecord {
                pc: 0x102,
                addr: 0x78002,
                value: 0x2222,
                cck: Some(110),
                size_bytes: 2,
                source: Some(WatchMemorySource::Blitter),
            },
            WatchMemoryRecord {
                pc: 0x104,
                addr: 0x78004,
                value: 0x3333,
                cck: Some(120),
                size_bytes: 2,
                source: Some(WatchMemorySource::Blitter),
            },
            WatchMemoryRecord {
                pc: 0x106,
                addr: 0x78006,
                value: 0x4444,
                cck: Some(115),
                size_bytes: 2,
                source: Some(WatchMemorySource::DiskDma),
            },
        ];
        let mut session = HeadlessSession::new(machine, 1);

        let observation = ScriptStep::WatchMemoryLog {
            limit: None,
            unique: false,
            source: Some(WatchMemorySource::Blitter),
            cck_min: Some(105),
            cck_max: Some(115),
        }
        .execute_collect(&mut session)
        .expect("filtered log should execute")
        .expect("filtered log should emit an observation");
        match observation {
            ScriptObservation::WatchMemoryLog {
                total_writes,
                returned,
                entries,
                ..
            } => {
                assert_eq!(total_writes, 4);
                assert_eq!(returned, 1);
                assert_eq!(entries[0].value, 0x2222);
                assert_eq!(entries[0].cck, Some(110));
                assert_eq!(entries[0].source, Some(WatchMemorySource::Blitter));
            }
            other => panic!("expected WatchMemoryLog, got {other:?}"),
        }

        let reversed = ScriptStep::WatchMemoryLog {
            limit: None,
            unique: false,
            source: None,
            cck_min: Some(200),
            cck_max: Some(100),
        };
        assert!(matches!(
            reversed.execute_collect(&mut session),
            Err(ScriptError::InvalidStep {
                step: "watch_memory_log",
                ..
            })
        ));
    }

    #[test]
    fn memory_write_source_is_typed_and_legacy_json_remains_compatible() {
        let source_aware = MemoryWriteEntry {
            pc: 0x1234,
            addr: 0x78000,
            value: 0xA55A,
            cck: Some(42),
            size_bytes: 2,
            source: Some(crate::watch::WatchMemorySource::DiskDma),
        };
        let json = serde_json::to_value(source_aware).expect("serialize source-aware write");
        assert_eq!(json["source"], "disk_dma");

        let legacy: MemoryWriteEntry = serde_json::from_value(json!({
            "pc": 0x1234,
            "addr": 0x4000,
            "value": 0x99,
            "size_bytes": 1
        }))
        .expect("deserialize legacy CPU-only write");
        assert_eq!(legacy.source, None);
        let legacy_json = serde_json::to_value(legacy).expect("serialize legacy write");
        assert!(legacy_json.get("source").is_none());
    }

    #[test]
    fn headless_script_parses_json_array() {
        let script = HeadlessScript::from_json_str(
            r#"
            [
              {"action":"run_frames","frames":2},
              {"action":"query","path":"session.time"},
              {"action":"save_screenshot","path":"boot.png"}
            ]
            "#,
        )
        .expect("script json should parse");

        assert_eq!(
            script.steps,
            vec![
                ScriptStep::RunFrames { frames: 2 },
                ScriptStep::Query {
                    path: "session.time".to_owned()
                },
                ScriptStep::SaveScreenshot {
                    path: PathBuf::from("boot.png")
                }
            ]
        );
    }

    #[test]
    fn headless_script_executes_media_run_capture_and_snapshot_steps() {
        let temp_dir = std::env::temp_dir();
        let media_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-demo.tap",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-shot.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-audio.wav",
            std::process::id()
        ));
        let snapshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-state.pst",
            std::process::id()
        ));
        std::fs::write(&media_path, [0x13, 0x00, 0x00]).expect("media fixture should write");

        let script = HeadlessScript {
            steps: vec![
                ScriptStep::LoadMedia {
                    slot: "tape-1".to_owned(),
                    kind: ScriptMediaKind::Tape,
                    path: media_path.clone(),
                    writable: false,
                },
                ScriptStep::MediaTransport {
                    slot: "tape-1".to_owned(),
                    transport: ScriptMediaTransportAction::Start,
                },
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::SaveScreenshot {
                    path: screenshot_path.clone(),
                },
                ScriptStep::SaveAudioCapture {
                    path: audio_path.clone(),
                    reset_after: true,
                },
                ScriptStep::SaveSnapshot {
                    path: snapshot_path.clone(),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should run to completion");

        assert_eq!(session.machine().tape_loaded, 1);
        assert_eq!(session.machine().commands, 1);
        assert_eq!(
            observations,
            vec![ScriptObservation::RunFrames {
                frames: 1,
                reached: MachineTime::new(69888),
                stop_reason: StopReason::ReachedTarget,
            }]
        );
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());
        assert!(snapshot_path.is_file());

        let _ = std::fs::remove_file(media_path);
        let _ = std::fs::remove_file(screenshot_path);
        let _ = std::fs::remove_file(audio_path);
        let _ = std::fs::remove_file(snapshot_path);
    }

    #[test]
    fn headless_script_collects_query_observations() {
        let script = HeadlessScript {
            steps: vec![
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::Query {
                    path: "session.time".to_owned(),
                },
                ScriptStep::QueryPaths {
                    prefix: Some("capture.".to_owned()),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should produce observations");

        assert_eq!(observations.len(), 3);
        assert_eq!(
            observations[1],
            ScriptObservation::Query {
                result: QueryResult {
                    path: "session.time".to_owned(),
                    value: serde_json::json!(69888),
                }
            }
        );
        assert_eq!(
            observations[2],
            ScriptObservation::QueryPaths {
                result: QueryPathsResult {
                    prefix: Some("capture.".to_owned()),
                    paths: vec![
                        "capture.has_audio".to_owned(),
                        "capture.has_frame".to_owned()
                    ],
                }
            }
        );
    }

    #[test]
    fn headless_script_waits_for_boot_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForBoot { max_frames: 3 }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy boot");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForBoot {
                frames: 3,
                reached: MachineTime::new(209_664),
                reason: "dummy boot banner is visible".to_owned(),
                row: Some(23),
            }]
        );
    }

    #[test]
    fn headless_script_waits_for_query_text_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForQueryContains {
                path: "screen.text.lines".to_owned(),
                needle: "MANIC MINER".to_owned(),
                max_frames: 4,
            }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy title text");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForQueryContains {
                path: "screen.text.lines".to_owned(),
                needle: "MANIC MINER".to_owned(),
                frames: 4,
                reached: MachineTime::new(279_552),
                line: Some(1),
                matched_text: "MANIC MINER".to_owned(),
            }]
        );
    }

    #[test]
    fn headless_script_waits_for_query_bool_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForQueryBool {
                path: "dummy.flag".to_owned(),
                value: true,
                max_frames: 2,
            }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy boolean query");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForQueryBool {
                path: "dummy.flag".to_owned(),
                value: true,
                frames: 2,
                reached: MachineTime::new(139_776),
            }]
        );
    }

    #[test]
    fn set_machine_round_trips_through_json() {
        let json = r#"[{"action":"set_machine","machine":"spectrum_48k"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::SetMachine {
                machine: "spectrum_48k".to_owned(),
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(
            serialized,
            r#"[{"action":"set_machine","machine":"spectrum_48k"}]"#
        );
    }

    #[test]
    fn set_machine_step_returns_system_specific_error_from_shell_executor() {
        // Shell-level executor stays system-agnostic; the binary
        // intercepts SetMachine before delegating. Calling the shell
        // executor directly surfaces the error the binary uses to
        // detect "this is one of mine".
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::SetMachine {
            machine: "spectrum_48k".to_owned(),
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => assert_eq!(step, "set_machine"),
            other => panic!("expected SystemSpecificStep error, got {other:?}"),
        }
    }

    #[test]
    fn autoload_tape_round_trips_through_json() {
        let json = r#"[{"action":"autoload_tape","slot":"tape-1","max_boot_frames":250}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::AutoloadTape {
                slot: "tape-1".to_owned(),
                max_boot_frames: 250,
            }]
        );
    }

    #[test]
    fn autoload_tape_step_returns_system_specific_error_from_shell_executor() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::AutoloadTape {
            slot: "tape-1".to_owned(),
            max_boot_frames: 100,
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => assert_eq!(step, "autoload_tape"),
            other => panic!("expected SystemSpecificStep error, got {other:?}"),
        }
    }

    #[test]
    fn load_basic_program_round_trips_through_json() {
        let json = r#"[{"action":"load_basic_program","path":"hello.bas","run":true}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::LoadBasicProgram {
                path: PathBuf::from("hello.bas"),
                run: true,
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn load_basic_program_defaults_run_to_true_when_omitted() {
        let json = r#"[{"action":"load_basic_program","path":"hello.bas"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::LoadBasicProgram {
                path: PathBuf::from("hello.bas"),
                run: true,
            }]
        );
    }

    #[test]
    fn load_basic_program_step_returns_system_specific_error_from_shell_executor() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::LoadBasicProgram {
            path: PathBuf::from("hello.bas"),
            run: true,
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => {
                assert_eq!(step, "load_basic_program");
            }
            other => panic!("expected SystemSpecificStep, got {other:?}"),
        }
    }

    #[test]
    fn load_basic_program_observation_serializes_summary_fields() {
        let observation = ScriptObservation::LoadBasicProgram {
            program_bytes: 42,
            ran: true,
        };
        let json = serde_json::to_string(&observation).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"load_basic_program","program_bytes":42,"ran":true}"#
        );
    }

    #[test]
    fn start_video_recording_round_trips_through_json() {
        let json = r#"[{"action":"start_video_recording","path":"clip.mp4"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::StartVideoRecording {
                path: PathBuf::from("clip.mp4"),
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn stop_video_recording_round_trips_through_json() {
        let json = r#"[{"action":"stop_video_recording"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(script.steps, vec![ScriptStep::StopVideoRecording]);
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn stop_video_recording_observation_serializes_the_summary_fields() {
        let observation = ScriptObservation::StopVideoRecording {
            path: PathBuf::from("/tmp/clip.mp4"),
            frames: 250,
            duration_ms: 5_000,
            has_audio: true,
        };
        let json = serde_json::to_string(&observation).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"stop_video_recording","path":"/tmp/clip.mp4","frames":250,"duration_ms":5000,"has_audio":true}"#
        );
    }

    #[test]
    fn set_machine_observation_uses_machine_field_to_avoid_kind_tag_clash() {
        // The ScriptObservation enum is internally tagged with
        // `kind: <variant_name>`. A field literally named `kind`
        // collides; we use `machine` instead. This regression test
        // freezes the JSON shape so a future "rename for symmetry"
        // edit doesn't accidentally break Code198x's report parsers.
        let observation = ScriptObservation::SetMachine {
            machine: "spectrum_48k".to_owned(),
            profile_id: "sinclair-zx-spectrum-48k-pal".to_owned(),
            display_name: "ZX Spectrum 48K (PAL)".to_owned(),
        };
        let json = serde_json::to_string(&observation).expect("serialize observation");
        assert_eq!(
            json,
            r#"{"kind":"set_machine","machine":"spectrum_48k","profile_id":"sinclair-zx-spectrum-48k-pal","display_name":"ZX Spectrum 48K (PAL)"}"#
        );
    }

    #[test]
    fn audio_recording_round_trips_through_json() {
        for json in [
            r#"[{"action":"start_audio_recording","path":"out.wav"}]"#,
            r#"[{"action":"stop_audio_recording"}]"#,
        ] {
            let script = HeadlessScript::from_json_str(json).expect("script should parse");
            let reserialised = serde_json::to_string(&script.steps).expect("re-serialize");
            assert_eq!(reserialised, json);
        }
    }

    #[test]
    fn recording_steps_are_exposed_on_the_common_mcp_surface() {
        // #453: start/stop audio and video recording must be reachable
        // over MCP on every common machine, not only the bespoke
        // Amiga/Spectrum surfaces. `register_common_tools` is the single
        // registrar every common machine calls, so asserting the four
        // recording tools land there guarantees fleet-wide script/MCP
        // parity in one place.
        use crate::mcp::ToolRegistry;
        use crate::mcp_tools::register_common_tools;

        let mut registry: ToolRegistry<HeadlessSession<DummyMachine, DummyQueryProvider>> =
            ToolRegistry::new();
        register_common_tools(&mut registry);

        for tool in [
            "start_audio_recording",
            "stop_audio_recording",
            "start_video_recording",
            "stop_video_recording",
        ] {
            assert!(
                registry.get(tool).is_some(),
                "common MCP surface must expose `{tool}` for script/MCP parity"
            );
        }
    }

    #[test]
    fn register_base_tools_exposes_the_whole_base_surface() {
        // #456: `register_base_tools` is the single entry point every
        // machine's mcp.rs calls. It must register the common session /
        // media / capture tools AND the generic debug verbs together, so
        // no machine can half-adopt the base set (the drift that left
        // C64 and Dragon without memory_read / step / disasm over MCP).
        use crate::mcp::ToolRegistry;
        use crate::mcp_tools::register_base_tools;

        // The canonical base surface: 18 common tools + 8 debug verbs.
        const BASE_TOOLS: &[&str] = &[
            // common (session / media / capture)
            "run_frames",
            "run_ticks",
            "wait_for_boot",
            "wait_for_query_contains",
            "wait_for_query_bool",
            "query",
            "query_paths",
            "input",
            "load_media",
            "media_transport",
            "load_snapshot",
            "save_snapshot",
            "save_screenshot",
            "save_audio_capture",
            "start_audio_recording",
            "stop_audio_recording",
            "start_video_recording",
            "stop_video_recording",
            "reset",
            // debug verbs (driven through DebugTarget)
            "query_cpu",
            "memory_read",
            "poke_byte",
            "poke_word",
            "disasm",
            "run_until_pc",
            "step",
            "io_trace",
        ];

        let mut registry: ToolRegistry<HeadlessSession<DummyMachine, DummyQueryProvider>> =
            ToolRegistry::new();
        register_base_tools(&mut registry);

        for tool in BASE_TOOLS {
            assert!(
                registry.get(tool).is_some(),
                "base MCP surface must expose `{tool}`"
            );
        }
    }

    #[test]
    fn reset_step_round_trips_through_json_for_both_kinds() {
        for (json, kind) in [
            (r#"[{"action":"reset","kind":"hard"}]"#, ResetKind::Hard),
            (r#"[{"action":"reset","kind":"soft"}]"#, ResetKind::Soft),
        ] {
            let script = HeadlessScript::from_json_str(json).expect("script should parse");
            assert_eq!(script.steps, vec![ScriptStep::Reset { kind }]);
            let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
            assert_eq!(serialized, json);
        }
    }

    #[test]
    fn reset_observation_uses_performed_field_to_avoid_kind_tag_clash() {
        // Same constraint as set_machine: ScriptObservation tags with
        // `kind`, so the variant uses `performed` for the reset kind.
        let observation = ScriptObservation::Reset {
            performed: ResetKind::Soft,
            reached: MachineTime::new(0),
        };
        let json = serde_json::to_string(&observation).expect("serialize observation");
        assert_eq!(json, r#"{"kind":"reset","performed":"soft","reached":0}"#);
    }

    #[test]
    fn headless_script_executes_reset_and_emits_observation() {
        let script = HeadlessScript {
            steps: vec![
                ScriptStep::RunFrames { frames: 2 },
                ScriptStep::Reset {
                    kind: ResetKind::Hard,
                },
                ScriptStep::Reset {
                    kind: ResetKind::Soft,
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should run to completion");

        assert_eq!(
            observations,
            vec![
                ScriptObservation::RunFrames {
                    frames: 2,
                    reached: MachineTime::new(2 * 69888),
                    stop_reason: StopReason::ReachedTarget,
                },
                ScriptObservation::Reset {
                    performed: ResetKind::Hard,
                    reached: MachineTime::new(2 * 69888),
                },
                ScriptObservation::Reset {
                    performed: ResetKind::Soft,
                    reached: MachineTime::new(2 * 69888),
                },
            ]
        );
    }
}
