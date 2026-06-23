//! Generic `MachineCore` wrapper for Spectrum-family variants.
//!
//! Each variant plugs into this generic wrapper for frame and audio
//! output, tape control, input plumbing, and snapshot round-trips.
//! The query provider that decodes screen text and per-variant boot
//! status is in [`crate::queries`]; it pulls variant-specific paths
//! through [`SpectrumMachine::variant_query_paths`] and
//! [`SpectrumMachine::resolve_variant_query`]. The 48K-specific
//! firmware constructors and audio-control wrappers live in
//! `spectrum_48k.rs`.
//!
//! Snapshot/restore are thin delegators into [`crate::snapshot`].
//! The per-event keyboard matrix update lives in [`crate::input`].

use common_sinclair_zx_spectrum::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::audio::{AudioControls, SpeakerChannel};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::keyboard::KeyboardMatrix;
use common_sinclair_zx_spectrum::snapshot::Snapshot;
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapeSpan};
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction, PixelFormat,
    QueryError, QueryResult, ResetKind, RunResult, StopReason,
};
use serde::{Deserialize, Serialize};

use crate::{Model, profile_for};

/// Trait satisfied by Spectrum-family machines so they can plug into
/// the generic runtime. Implementors already derive
/// `serde::Serialize`/`Deserialize`; the runtime uses that for snapshot
/// round-trips through postcard.
pub trait SpectrumMachine: Serialize + for<'de> Deserialize<'de> + SpectrumDriver {
    /// Pixel width of the framebuffer.
    const FRAME_WIDTH: u32;
    /// Pixel height of the framebuffer.
    const FRAME_HEIGHT: u32;
    /// Mono audio sample rate in Hz.
    const AUDIO_SAMPLE_RATE: u32 = 44_100;

    /// Returns the authoritative frame length in master-clock half-cycles.
    /// Exposed as a method so variants with a runtime-selected crystal
    /// (e.g. TC2068 vs TS2068) can return the correct value per-instance.
    fn frame_halfcycles(&self) -> u32;

    /// Runs exactly one frame.
    fn run_frame(&mut self);

    /// Returns the indexed-8 framebuffer as a byte slice.
    fn framebuffer(&self) -> &[u8];

    /// Returns the current audio sample buffer.
    fn audio_frame(&self) -> &[f32];

    /// Current host-side speaker audio controls.
    fn audio_controls(&self) -> AudioControls;

    /// Replaces the host-side speaker audio controls wholesale.
    fn set_audio_controls(&mut self, controls: AudioControls);

    /// Enables or disables one host-side audio channel.
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool);

    /// Sets the host-side gain for one audio channel.
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32);

    /// Copies fresh keyboard row bytes into the machine's scan matrix.
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]);

    /// Sets one Kempston joystick button's pressed state.
    ///
    /// Button index follows the bit layout of the Kempston state byte:
    /// `0` = right, `1` = left, `2` = down, `3` = up, `4` = fire. Indices
    /// outside `0..=4` are ignored.
    ///
    /// Returns `true` when the machine has a Kempston interface and the
    /// event was applied; `false` when the machine has no Kempston
    /// hardware (Amstrad-class +2A / +2B / +3) and silently drops the
    /// event. The first applied event also flips the peripheral's
    /// `attached` flag, mirroring real hardware where the interface only
    /// becomes visible to software once any input arrives — software that
    /// probes `$1F` for Kempston detection sees the floating bus
    /// (`0xFF`-ish via the ULA path) until the user touches the pad.
    ///
    /// Default implementation returns `false`. Override on variants that
    /// own a `KempstonJoystick` peripheral (every 48K-class and 128K-
    /// class machine; Pentagon and Scorpion; not the Amstrad-class).
    fn set_kempston_button(&mut self, button: u8, pressed: bool) -> bool {
        let _ = (button, pressed);
        false
    }

    /// Loads tape blocks parsed from a `.tap` container.
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>);

    /// Loads a tape pulse stream parsed from a `.tzx` container.
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>);

    /// Starts tape transport.
    fn tape_play(&mut self);

    /// Stops tape transport.
    fn tape_stop(&mut self);

    /// Decodes any captured tape SAVE signal into standard-speed blocks.
    /// Defaults to none for machine classes whose SAVE capture is not yet
    /// wired (only the 48K class records today).
    fn recorded_tape_blocks(&self) -> Vec<TapeBlock> {
        Vec::new()
    }

    /// Discards any captured tape SAVE signal (e.g. after a flush).
    fn clear_tape_recording(&mut self) {}

    /// Soft-resets the machine's CPU, timing, and audio state.
    fn reset_machine(&mut self);

    /// Hook called after the runtime has decoded a snapshot into this
    /// machine. Default: no-op. Variants override to repair `&'static`
    /// references that don't survive serde's `#[serde(skip)]` round-trip
    /// — most importantly `Z80::rehydrate_walker_sequence`, which
    /// re-derives the mid-instruction walker sequence from the preserved
    /// `(prefix, opcode)` so snapshots taken at frame boundaries (i.e.
    /// almost always mid-instruction) reload as a coherent CPU.
    fn after_restore(&mut self) {}

    /// Applies one parsed `.sna` / `.z80` snapshot to the machine's
    /// CPU registers, border, memory pages, and (where applicable)
    /// paging / AY register state. Used by the binary's
    /// `File > Open Snapshot...` and the script step
    /// `LoadPortableSnapshot` (when that lands).
    fn apply_snapshot(&mut self, snap: &Snapshot);

    /// Returns `true` when this machine accepts a disk image at the
    /// given media slot. Default: `false` (tape-only machines).
    fn supports_disk_slot(&self, _slot: &str) -> bool {
        false
    }

    /// Loads disk-image bytes into the machine's FDC. Default: reports
    /// that the machine has no disk interface. Variants with a real
    /// drive (e.g. the +3) parse the payload and hand it to the
    /// controller.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason if the image cannot be parsed
    /// or if the target slot is unknown.
    fn load_disk_image(&mut self, _slot: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("this machine has no disk interface".to_owned())
    }

    /// Ejects the disk at the given media slot. Default: reports that the
    /// machine has no disk interface. The counterpart to
    /// [`load_disk_image`](Self::load_disk_image); variants with a real drive
    /// surface their controller's existing eject.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason if the machine has no disk interface or
    /// the slot is unknown.
    fn eject_disk_image(&mut self, _slot: &str) -> Result<(), String> {
        Err("this machine has no disk interface".to_owned())
    }

    // ─── Shared query surface ─────────────────────────────────────────
    //
    // The methods below are read-only accessors used by the generic
    // `SpectrumSessionQueryProvider`. Every Spectrum variant exposes the
    // same shape (memory, keyboard rows, tape state, frame timing), so
    // the provider can query them without per-variant glue.

    /// Reads one byte from the machine's CPU-visible address space.
    /// Used by the generic screen-text / boot detection query path that
    /// reads ROM glyphs at $3D00 and screen RAM at $4000.
    fn read_byte(&self, addr: u16) -> u8;

    /// Writes one byte into the machine's CPU-visible address space.
    /// Writes to ROM regions are silently dropped by the underlying
    /// memory bus. Used by host-side helpers that poke RAM directly,
    /// e.g. installing a tokenised BASIC program at `PROG`.
    fn write_byte(&mut self, addr: u16, value: u8);

    /// Returns one byte of the machine's standard ROM glyph table —
    /// `offset` is `0..768` (96 glyphs × 8 bytes), starting at the
    /// space character (0x20) and ending at code 0x7F (the
    /// copyright sign). The default implementation reads through
    /// `read_byte($3D00 + offset)`, which works for unpaged variants
    /// (48K, TC2048) where the glyph table lives in the only ROM at
    /// `$3D00..=$3F00`.
    ///
    /// **Paged variants must override this.** On the 128K family the
    /// menu ROM is mapped at `$0000-$3FFF` after boot, but the menu
    /// ROM doesn't carry the standard glyph table at `$3D00` — only
    /// the 48 BASIC sub-ROM does. Variants override to reach the
    /// 48 BASIC sub-ROM directly via `memory.read_rom_byte(idx,
    /// $3D00 + offset)` regardless of the current paging.
    #[must_use]
    fn glyph_byte(&self, offset: u16) -> u8 {
        self.read_byte(0x3D00u16.wrapping_add(offset))
    }

    /// Returns the current keyboard matrix rows (active-low). Used by
    /// the `spectrum.keyboard.rows` query.
    fn keyboard_rows(&self) -> &[u8; 8];

    /// Returns whether a tape image is loaded.
    fn tape_is_loaded(&self) -> bool;

    /// Returns whether tape transport is currently playing.
    fn tape_is_playing(&self) -> bool;

    /// Returns the current half-cycle position within the frame.
    fn half_cycle_in_frame(&self) -> u32;

    /// Returns the current T-state position within the frame.
    fn tstate_in_frame(&self) -> u32;

    /// Begin tracing every Z80 memory write whose target falls
    /// inside `[addr, addr + len)`. Default impl returns an error
    /// so exotic variants without a tracer don't silently swallow
    /// the request. SOLID-8 variants (16K / 48K / + / 128K / +2 /
    /// +2A / +2B / +3) override to wire into their common-class
    /// core's `MemoryWriteWatch`.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a short reason on variants that don't
    /// implement the tracer.
    fn start_memory_write_watch(&mut self, _addr: u16, _len: u16) -> Result<(), &'static str> {
        Err("memory write watch is not supported on this Spectrum variant")
    }

    /// Stop the current write watch (drop the configured range and
    /// any captured records). Default impl is a no-op.
    fn stop_memory_write_watch(&mut self) {}

    /// Captured CPU writes since the last
    /// `start_memory_write_watch`. `None` means either no watch is
    /// configured *or* the variant doesn't support the tracer.
    #[must_use]
    fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        None
    }

    /// Current watch range as `(addr, len)`, or `None` when no watch
    /// is configured (or the variant doesn't support the tracer).
    /// `len` equals `hi - lo` (exclusive upper bound minus inclusive
    /// lower) and may be `0` for a placeholder watch.
    #[must_use]
    fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        None
    }

    /// Drop captured write records without removing the watch range.
    /// Default impl is a no-op.
    fn clear_memory_write_watch_records(&mut self) {}

    /// Returns a borrow of the Z80 register file. Every Spectrum-family
    /// variant carries a Z80, so this is a required method without a
    /// default. Used by the `query_cpu` script step / MCP tool to
    /// expose register state to scripts.
    fn z80_registers(&self) -> &zilog_z80::Registers;

    /// Whether the Z80 is currently halted (executing NOPs while
    /// waiting for an interrupt). Read off the Z80 chip's `halt` pin
    /// rather than the register file.
    fn z80_halted(&self) -> bool;

    /// `true` when the Z80 is at an instruction boundary (the walker
    /// reports `instruction_complete`). Exposed for the `query_cpu`
    /// surface; **not** used for stepping — see
    /// [`Self::z80_instructions_retired`].
    fn z80_instruction_complete(&self) -> bool;

    /// Monotonic count of instructions the Z80 has retired. The reliable
    /// single-step signal: tick until it advances by one. The level
    /// `instruction_complete` flag is unusable here — it stays true through
    /// the next opcode fetch and flips false→true within one tick for a
    /// one-M-cycle op, so a between-tick check over-runs short instructions.
    fn z80_instructions_retired(&self) -> u64;

    /// Bus-level Z80 I/O port read. Takes `&mut self` because
    /// some ports (notably the floating bus) and routed peripherals
    /// (Kempston, AY data) may mutate driver state on read.
    fn port_read(&mut self, port: u16) -> u8;

    /// Bus-level Z80 I/O port write. Side-effects mirror what an
    /// `OUT (C),A` would produce (border colour, beeper, paging,
    /// AY register select / data, …).
    fn port_write(&mut self, port: u16, value: u8);

    /// Begin tracing every `OUT ($BFFD), data` write. Variants
    /// without an AY (16K / 48K / Spectrum+ / TC2048) return `Err`.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a short reason on variants that don't
    /// implement the tracer.
    fn start_ay_write_watch(&mut self) -> Result<(), &'static str> {
        Err("AY register tracer is not supported on this Spectrum variant")
    }

    /// Stop the AY tracer (drop the watch and any captured records).
    fn stop_ay_write_watch(&mut self) {}

    /// Captured AY writes since the last `start_ay_write_watch`.
    /// `None` means either no watch is configured or the variant
    /// doesn't support the tracer.
    #[must_use]
    fn ay_write_watch_records(&self) -> Option<&[common_sinclair_zx_spectrum::AyWriteRecord]> {
        None
    }

    /// Drop captured AY records without removing the watch.
    fn clear_ay_write_watch_records(&mut self) {}

    /// Run cycles until exactly one Z80 instruction completes. Returns
    /// the number of master-clock half-cycles consumed.
    ///
    /// The walker's `instruction_complete` flag starts `true` between
    /// instructions, transitions to `false` while an instruction is
    /// being fetched + executed, and snaps back to `true` when the
    /// instruction's last M-cycle finishes. We tick until we've
    /// observed the in-progress → complete transition, so a call from
    /// an instruction-boundary state runs exactly one full instruction.
    /// The budget caps the loop so a pathological non-terminating
    /// instruction (chip bug, not user input) can't hang the binary.
    fn step_instruction(&mut self) -> u32 {
        const STEP_HALFCYCLE_BUDGET: u32 = 512;
        // Tick until exactly one instruction retires. The retirement counter
        // is the only reliable boundary signal (see the trait method docs):
        // the old `instruction_complete`-edge loop silently over-ran
        // one-M-cycle instructions because their false→true transition
        // happens within a single tick.
        let target = self.z80_instructions_retired().wrapping_add(1);
        let mut hc = 0u32;
        while hc < STEP_HALFCYCLE_BUDGET {
            self.tick_one_halfcycle();
            hc += 1;
            if self.z80_instructions_retired() == target {
                return hc;
            }
        }
        hc
    }

    /// Run cycles until `n` Z80 instructions have completed or the
    /// per-call budget is exhausted. Returns the total half-cycles
    /// consumed.
    fn step_instructions(&mut self, n: u32) -> u32 {
        let mut total = 0u32;
        for _ in 0..n {
            total = total.wrapping_add(self.step_instruction());
        }
        total
    }

    /// Run cycles until the Z80's PC reaches `target` at an
    /// instruction boundary, or `max_halfcycles` is exhausted.
    ///
    /// Returns `(reached, halfcycles_consumed, instructions_executed)`.
    /// `reached` is `true` when PC matched `target` before the budget
    /// ran out, `false` when the budget expired without a match.
    fn run_until_pc(&mut self, target: u16, max_halfcycles: u32) -> (bool, u32, u32) {
        let mut hc = 0u32;
        let mut instructions = 0u32;
        while hc < max_halfcycles {
            let consumed = self.step_instruction();
            hc = hc.saturating_add(consumed);
            instructions = instructions.saturating_add(1);
            if self.z80_registers().pc == target {
                return (true, hc, instructions);
            }
        }
        (false, hc, instructions)
    }

    // ─── Variant-specific query surface ───────────────────────────────
    //
    // Each variant supplies the additional path catalogue it owns
    // (e.g. AY register state, board issue, SCLD high-res flag) plus a
    // dispatcher. Default impls expose nothing, so unimplemented
    // variants simply have no extra queries.

    /// Returns the variant-specific query paths this machine owns.
    /// These are aggregated into the generic
    /// `SpectrumSessionQueryProvider`'s path catalogue alongside the
    /// shared paths.
    #[must_use]
    fn variant_query_paths() -> &'static [&'static str] {
        &[]
    }

    /// AY-3-8912 query paths this variant owns (grouped `ay` object plus
    /// decoded leaves). Default: none — the 16K / 48K / Spectrum+ /
    /// TC2048 have no AY chip. AY-bearing variants override this to
    /// return the shared `AY_QUERY_PATHS`, keeping advertisement and the
    /// `resolve_ay_path` dispatch on a single source.
    #[must_use]
    fn ay_query_paths() -> &'static [&'static str] {
        &[]
    }

    /// Resolves one variant-specific query path.
    ///
    /// Returns `Ok(None)` when the variant does not own the path; the
    /// generic provider then surfaces it as an unknown-path error.
    ///
    /// # Errors
    ///
    /// Returns `QueryError` only when the path is recognised but
    /// resolution fails (e.g. transient unavailability). Unknown paths
    /// must return `Ok(None)`, not an error.
    fn resolve_variant_query(&self, _path: &str) -> Result<Option<QueryResult>, QueryError> {
        Ok(None)
    }
}

/// One disk image's raw bytes plus the slot they were loaded into.
/// The runtime caches these alongside the machine so they survive
/// snapshot round-trips — the FDC marks its `disks` field
/// `#[serde(skip)]` (the parsed `DiskImage` is large and not all of
/// it is reconstructible from disk state alone), so the disk content
/// would otherwise vanish on restore. See Seam 3 of
/// `knowledge/decisions/spectrum-architecture-review.md`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiskCacheEntry {
    pub(crate) slot: String,
    pub(crate) bytes: Vec<u8>,
}

/// Generic `MachineCore` runtime wrapper for Spectrum-family variants.
pub struct SpectrumRuntime<M: SpectrumMachine> {
    profile: MachineProfile,
    machine: M,
    keyboard: KeyboardMatrix,
    time: MachineTime,
    /// Mounted disk images, cached as raw bytes so they re-insert
    /// cleanly into the FDC after a snapshot restore. See Seam 3.
    disk_images: Vec<DiskCacheEntry>,
}

impl<M: SpectrumMachine> SpectrumRuntime<M> {
    /// Creates a runtime for the given profile and pre-initialised machine.
    #[must_use]
    pub fn new(model: Model, machine: M) -> Self {
        Self {
            profile: profile_for(model),
            machine,
            keyboard: KeyboardMatrix::new(),
            time: MachineTime::default(),
            disk_images: Vec::new(),
        }
    }

    /// Returns the wrapped machine.
    #[must_use]
    pub fn machine(&self) -> &M {
        &self.machine
    }

    /// Returns mutable access to the wrapped machine.
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut M {
        &mut self.machine
    }

    /// Flushes any captured tape `SAVE` to a `.tap` image.
    ///
    /// Unlike a disk `SAVE`, a tape `SAVE` never mutates a mounted (playback)
    /// tape — the ROM lays a fresh signal on the MIC line that the recorder
    /// captures independently. This decodes that signal into standard-speed
    /// blocks and serialises them as a reloadable `.tap`. Returns `None` when
    /// nothing has been recorded.
    #[must_use]
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        let blocks = self.machine.recorded_tape_blocks();
        if blocks.is_empty() {
            return None;
        }

        // A recorded block's `data` is the full on-tape stream (flag, payload,
        // checksum); a TAP block stores only the payload, so strip both ends.
        let tap_blocks: Vec<_> = blocks
            .into_iter()
            .map(|block| format_sinclair_zx_spectrum_tap::TapBlock {
                flag: block.flag,
                data: block
                    .data
                    .get(1..block.data.len().saturating_sub(1))
                    .unwrap_or(&[])
                    .to_vec(),
            })
            .collect();

        Some(format_sinclair_zx_spectrum_tap::encode_tap(&tap_blocks))
    }

    /// Discards any captured tape `SAVE` signal.
    pub fn clear_tape_recording(&mut self) {
        self.machine.clear_tape_recording();
    }

    /// Returns the current runtime time in authoritative half-cycles.
    ///
    /// Named `time_value` to avoid colliding with the `MachineCore::time`
    /// trait method when called from inside the sibling snapshot module.
    #[must_use]
    pub const fn time_value(&self) -> MachineTime {
        self.time
    }

    /// Current host-side speaker audio controls.
    ///
    /// Generic-over-`M` passthrough so binary call sites
    /// (`runtime.audio_controls()`) work uniformly across every variant
    /// without importing the [`SpectrumMachine`] trait.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        SpectrumMachine::audio_controls(&self.machine)
    }

    /// Replaces the host-side speaker audio controls wholesale.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        SpectrumMachine::set_audio_controls(&mut self.machine, controls);
    }

    /// Enables or disables one host-side audio channel.
    pub fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        SpectrumMachine::set_audio_channel_enabled(&mut self.machine, channel, enabled);
    }

    /// Sets the host-side gain for one audio channel.
    pub fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        SpectrumMachine::set_audio_channel_gain(&mut self.machine, channel, gain);
    }

    /// Returns mutable access to the runtime's machine profile so that
    /// per-variant adapters can promote the support tier or extend the
    /// declared capability set without re-implementing the runtime.
    #[must_use]
    pub fn profile_mut(&mut self) -> &mut MachineProfile {
        &mut self.profile
    }

    /// Returns mutable access to the runtime-side keyboard matrix
    /// cache. Used by [`crate::input::apply_input_event`] to flip a
    /// single key while preserving the rest of the matrix state.
    pub(crate) fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// Returns the cached keyboard rows. Used by snapshot encoding.
    pub(crate) fn keyboard_rows(&self) -> &[u8; 8] {
        self.keyboard.rows()
    }

    /// Replaces the cached keyboard rows. Used by snapshot decoding;
    /// the caller is expected to push the rows into the machine after
    /// the matrix has been restored.
    pub(crate) fn set_keyboard_rows(&mut self, rows: [u8; 8]) {
        *self.keyboard.rows_mut() = rows;
    }

    /// Returns the cached disk-image payloads. Used by snapshot
    /// encoding so disk content survives the round-trip — see
    /// [`DiskCacheEntry`] and Seam 3 of the architecture review.
    pub(crate) fn disk_images(&self) -> &[DiskCacheEntry] {
        &self.disk_images
    }

    /// Replaces the cached disk images and re-injects each one into
    /// the underlying machine. Used by snapshot decoding after
    /// `after_restore`. Failures are propagated so a malformed cache
    /// surfaces rather than silently dropping the disk.
    pub(crate) fn restore_disk_images(
        &mut self,
        images: Vec<DiskCacheEntry>,
    ) -> Result<(), MachineError> {
        self.disk_images = images;
        for entry in &self.disk_images {
            self.machine
                .load_disk_image(&entry.slot, &entry.bytes)
                .map_err(|reason| MachineError::InvalidMedia {
                    slot: entry.slot.clone(),
                    reason,
                })?;
        }
        Ok(())
    }

    /// Replaces the wrapped machine. Used by snapshot decoding.
    pub(crate) fn set_machine(&mut self, machine: M) {
        self.machine = machine;
    }

    /// Replaces the runtime time stamp. Used by snapshot decoding.
    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    fn load_tape_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        if is_tzx(bytes) {
            let stream =
                format_sinclair_zx_spectrum_tzx::tzx_to_stream(bytes).map_err(|reason| {
                    MachineError::InvalidMedia {
                        slot: slot.to_owned(),
                        reason,
                    }
                })?;
            self.machine.load_tape_stream(stream);
        } else {
            let blocks = format_sinclair_zx_spectrum_tap::parse_tap(bytes).map_err(|reason| {
                MachineError::InvalidMedia {
                    slot: slot.to_owned(),
                    reason,
                }
            })?;
            self.machine
                .load_tape_blocks(tap_blocks_to_tape_blocks(blocks));
        }
        Ok(())
    }
}

impl<M: SpectrumMachine> MachineCore for SpectrumRuntime<M> {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.machine.reset_machine();
        self.keyboard.clear();
        self.machine.set_keyboard_rows(self.keyboard.rows());
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Tape if slot == "tape-1" => {
                    self.load_tape_bytes(slot, image.bytes)?;
                }
                MediaKind::Disk if self.machine.supports_disk_slot(slot) => {
                    self.machine
                        .load_disk_image(slot, image.bytes)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason,
                        })?;
                    // Cache the raw bytes so the disk survives a
                    // snapshot round-trip (Seam 3). The FDC marks
                    // `disks` `#[serde(skip)]`; without this cache the
                    // image would silently vanish on restore.
                    self.disk_images.retain(|d| d.slot != slot);
                    self.disk_images.push(DiskCacheEntry {
                        slot: slot.to_owned(),
                        bytes: image.bytes.to_vec(),
                    });
                }
                MediaKind::Tape | MediaKind::Disk => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                _ => {
                    return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
                }
            }
        }
        Ok(())
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        // Disk eject on a disk-capable variant. On a tape-only variant the
        // machine reports no disk interface, surfaced here as Unsupported so
        // the harness handles it gracefully rather than panicking.
        if self.machine.supports_disk_slot(slot) {
            self.machine
                .eject_disk_image(slot)
                .map_err(|reason| MachineError::InvalidMedia {
                    slot: slot.to_owned(),
                    reason,
                })?;
            // Drop the snapshot cache entry so the ejected disk doesn't
            // reappear on a restore.
            self.disk_images.retain(|d| d.slot != slot);
            return Ok(());
        }
        // The tape decoder has no eject path on the Spectrum machine (it
        // loads/plays/stops a tape), so tape eject stays unsupported.
        Err(MachineError::UnsupportedOperation {
            operation: "eject_media",
        })
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            crate::input::apply_input_event(self, event);
        }
        self.machine.set_keyboard_rows(self.keyboard.rows());

        while self.time < target {
            SpectrumMachine::run_frame(&mut self.machine);
            self.time = self
                .time
                .saturating_add(u64::from(self.machine.frame_halfcycles()));

            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: PixelFormat::Indexed8,
                width: M::FRAME_WIDTH,
                height: M::FRAME_HEIGHT,
                palette: Some(&SPECTRUM_PALETTE),
                pixels: self.machine.framebuffer(),
            })?;

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: M::AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: self.machine.audio_frame(),
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        crate::snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        crate::snapshot::decode(self, bytes)
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match command {
            ControlCommand::MediaTransport(cmd) => {
                if cmd.slot.as_ref() != "tape-1" {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: cmd.slot.as_ref().to_owned(),
                    });
                }
                match cmd.action {
                    MediaTransportAction::Start => self.machine.tape_play(),
                    MediaTransportAction::Stop => self.machine.tape_stop(),
                    _ => {
                        return Err(MachineError::UnsupportedOperation {
                            operation: "media-transport",
                        });
                    }
                }
                Ok(())
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            }),
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

fn is_tzx(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[..7] == b"ZXTape!" && bytes[7] == 0x1A
}

fn tap_blocks_to_tape_blocks(
    blocks: Vec<format_sinclair_zx_spectrum_tap::TapBlock>,
) -> Vec<TapeBlock> {
    blocks
        .into_iter()
        .map(|block| {
            let mut full = Vec::with_capacity(block.data.len() + 2);
            full.push(block.flag);
            full.extend_from_slice(&block.data);
            let checksum = full.iter().fold(0u8, |acc, &byte| acc ^ byte);
            full.push(checksum);

            TapeBlock {
                flag: block.flag,
                data: full,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spectrum48kRuntime;
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use emu198x_shell::{
        ControlCommand, MachineCore, MediaImage, MediaSet, MediaTransportAction,
        MediaTransportCommand, ResetKind,
    };
    use format_sinclair_zx_spectrum_tap::TapBlock;

    /// `is_tzx` recognises the canonical 8-byte TZX header
    /// (`ZXTape!\x1A`). Catches a regression where a 1-byte typo in
    /// the magic comparison silently misroutes TZX into the TAP parser.
    #[test]
    fn is_tzx_recognises_canonical_header() {
        let mut data = b"ZXTape!\x1a".to_vec();
        data.extend_from_slice(&[1, 20]); // version bytes — required to be ≥ 8 bytes total
        assert!(is_tzx(&data));
    }

    /// `is_tzx` rejects byte streams that are too short to contain
    /// the magic header, and streams whose magic doesn't match. Both
    /// cases route to the TAP parser fallback.
    #[test]
    fn is_tzx_rejects_short_or_wrong_magic() {
        assert!(!is_tzx(b""), "empty");
        assert!(!is_tzx(b"ZXTape!"), "missing $1A");
        assert!(!is_tzx(b"ZXTape!\x1b"), "wrong control byte");
        assert!(!is_tzx(b"NOT_TZX!"), "completely wrong magic");
    }

    /// `tap_blocks_to_tape_blocks` re-attaches the per-block flag byte
    /// and XOR checksum that `parse_tap` strips, so the tape player
    /// sees the same byte stream the original cassette delivered.
    #[test]
    fn tap_blocks_to_tape_blocks_round_trip_with_checksum() {
        let blocks = vec![TapBlock {
            flag: 0xFF,
            data: vec![0x01, 0x02, 0x03],
        }];
        let out = tap_blocks_to_tape_blocks(blocks);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].flag, 0xFF);
        // Full re-encoded form: flag + data + XOR checksum.
        let expected_checksum = 0xFF ^ 0x01 ^ 0x02 ^ 0x03;
        assert_eq!(out[0].data, vec![0xFF, 0x01, 0x02, 0x03, expected_checksum]);
    }

    /// `tap_blocks_to_tape_blocks` returns an empty vector for an
    /// empty input — defensive boundary for the parser's edge case.
    #[test]
    fn tap_blocks_to_tape_blocks_handles_empty_input() {
        let out = tap_blocks_to_tape_blocks(Vec::new());
        assert!(out.is_empty());
    }

    /// `MachineCore::reset` clears the runtime's keyboard cache,
    /// pushes the cleared rows into the machine, resets the machine
    /// state, and zeroes the runtime time counter.
    #[test]
    fn reset_clears_keyboard_and_zeroes_time() {
        let mut runtime = Spectrum48kRuntime::blank();
        // Dirty the keyboard cache by pressing a key.
        use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
        runtime.keyboard_mut().press_key(SpectrumKey::Q);
        // Verify it's pressed (row 2, bit 0 — Q).
        assert_eq!(runtime.keyboard_rows()[2] & 0x01, 0);

        runtime.reset(ResetKind::Hard);

        // After reset: keyboard rows are all 0xFF (released).
        assert_eq!(runtime.keyboard_rows(), &[0xFF; 8]);
        // Time counter zeroed.
        assert_eq!(runtime.time(), MachineTime::default());
    }

    /// `MachineCore::command` on `MediaTransport(tape-1, Start)`
    /// reaches `SpectrumMachine::tape_play` on the machine. The
    /// follow-up `Stop` reaches `tape_stop`. The runtime's command
    /// surface is what the native UI and script runner drive.
    #[test]
    fn command_media_transport_routes_to_tape_play_and_stop() {
        let mut runtime = Spectrum48kRuntime::blank();
        // Without a loaded tape, play/stop are no-ops on the
        // machine but must complete without error on the runtime.
        let start = ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        ));
        let stop = ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        ));
        assert!(runtime.command(&start).is_ok());
        assert!(runtime.command(&stop).is_ok());
    }

    /// `MachineCore::command` on a media-transport command for any
    /// slot other than `tape-1` surfaces `UnknownMediaSlot`. The
    /// runtime doesn't dispatch to disk transport from this path —
    /// disk insertion is a separate `load_media` call.
    #[test]
    fn command_media_transport_unknown_slot_errors() {
        let mut runtime = Spectrum48kRuntime::blank();
        let bad = ControlCommand::MediaTransport(MediaTransportCommand::new(
            "disk-a",
            MediaTransportAction::Start,
        ));
        match runtime.command(&bad) {
            Err(MachineError::UnknownMediaSlot { slot }) => assert_eq!(slot, "disk-a"),
            other => panic!("expected UnknownMediaSlot, got {other:?}"),
        }
    }

    /// `MachineCore::load_media` on an unrecognised tape slot
    /// surfaces `UnknownMediaSlot` rather than silently dropping the
    /// payload. Today only `tape-1` is recognised on the 48K; future
    /// multi-deck variants would extend this surface.
    #[test]
    fn load_media_unknown_tape_slot_errors() {
        let mut runtime = Spectrum48kRuntime::blank();
        let mut set = MediaSet::new();
        // A real TAP block: flag 0x00, one byte 0x42, checksum 0x42.
        let tap_bytes = [0x03, 0x00, 0x00, 0x42, 0x42];
        set.push(MediaImage::new(
            "tape-2",
            emu198x_shell::MediaKind::Tape,
            &tap_bytes,
        ));
        match runtime.load_media(&set) {
            Err(MachineError::UnknownMediaSlot { slot }) => assert_eq!(slot, "tape-2"),
            other => panic!("expected UnknownMediaSlot, got {other:?}"),
        }
    }

    /// `MachineCore::load_media` on a Snapshot kind surfaces
    /// `UnsupportedMediaKind` — the runtime accepts only Tape and
    /// Disk through this path. Snapshots load via `restore`.
    #[test]
    fn load_media_snapshot_kind_errors() {
        let mut runtime = Spectrum48kRuntime::blank();
        let mut set = MediaSet::new();
        set.push(MediaImage::new(
            "snap-1",
            emu198x_shell::MediaKind::Snapshot,
            &[],
        ));
        match runtime.load_media(&set) {
            Err(MachineError::UnsupportedMediaKind { kind }) => {
                assert_eq!(kind, emu198x_shell::MediaKind::Snapshot);
            }
            other => panic!("expected UnsupportedMediaKind, got {other:?}"),
        }
    }

    /// `MachineCore::load_media` rejects a disk on the 48K (which
    /// has no disk slot) with `UnknownMediaSlot`. Disk media is
    /// only supported on +3.
    #[test]
    fn load_media_disk_on_non_disk_variant_errors() {
        let mut runtime = Spectrum48kRuntime::blank();
        let mut set = MediaSet::new();
        set.push(MediaImage::new(
            "disk-a",
            emu198x_shell::MediaKind::Disk,
            &[],
        ));
        match runtime.load_media(&set) {
            Err(MachineError::UnknownMediaSlot { slot }) => assert_eq!(slot, "disk-a"),
            other => panic!("expected UnknownMediaSlot, got {other:?}"),
        }
    }

    /// `MachineCore::capabilities` returns a clone of the profile's
    /// capability set. The 48K profile advertises beeper-audio,
    /// keyboard-matrix, tape-input, and the snapshot-import /
    /// snapshot-export pair from the boots-tier promotion.
    #[test]
    fn capabilities_match_profile_capabilities() {
        let runtime = Spectrum48kRuntime::blank();
        let caps = runtime.capabilities();
        assert!(caps.contains(&emu198x_shell::known_capability("beeper-audio")));
        assert!(caps.contains(&emu198x_shell::known_capability("keyboard-matrix")));
    }

    /// `time_value` starts at default (zero) for a fresh runtime.
    #[test]
    fn time_value_starts_at_default() {
        let runtime = Spectrum48kRuntime::blank();
        assert_eq!(runtime.time_value(), MachineTime::default());
    }

    /// `machine()` and `machine_mut()` return references to the same
    /// underlying machine — mutation through `_mut()` is visible
    /// through the immutable accessor.
    #[test]
    fn machine_mut_mutation_visible_through_machine_accessor() {
        let mut runtime = Spectrum48kRuntime::blank();
        runtime.machine_mut().write(0x4000, 0x99);
        assert_eq!(runtime.machine().read(0x4000), 0x99);
    }
}
