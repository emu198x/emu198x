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
use common_sinclair_zx_spectrum::keyboard::KeyboardMatrix;
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
pub trait SpectrumMachine: Serialize + for<'de> Deserialize<'de> {
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

    /// Copies fresh keyboard row bytes into the machine's scan matrix.
    fn set_keyboard_rows(&mut self, rows: &[u8; 8]);

    /// Loads tape blocks parsed from a `.tap` container.
    fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>);

    /// Loads a tape pulse stream parsed from a `.tzx` container.
    fn load_tape_stream(&mut self, stream: Vec<TapeSpan>);

    /// Starts tape transport.
    fn tape_play(&mut self);

    /// Stops tape transport.
    fn tape_stop(&mut self);

    /// Soft-resets the machine's CPU, timing, and audio state.
    fn reset_machine(&mut self);

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
    fn resolve_variant_query(
        &self,
        _path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        Ok(None)
    }
}

/// Generic `MachineCore` runtime wrapper for Spectrum-family variants.
pub struct SpectrumRuntime<M: SpectrumMachine> {
    profile: MachineProfile,
    machine: M,
    keyboard: KeyboardMatrix,
    time: MachineTime,
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

    /// Returns the current runtime time in authoritative half-cycles.
    ///
    /// Named `time_value` to avoid colliding with the `MachineCore::time`
    /// trait method when called from inside the sibling snapshot module.
    #[must_use]
    pub const fn time_value(&self) -> MachineTime {
        self.time
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
            self.machine.run_frame();
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
