//! The shared Commodore GCR rotation/serialiser engine.
//!
//! This is the cycle-accurate floppy-read mechanism the 1541 and 1571 drive
//! cores share verbatim: head stepping, density-zone rotation, the read shift
//! register assembling bytes off the surface (with SYNC detection and weak-bit
//! flux), and the write shift register laying the port latch back onto it. Each
//! drive *embeds* a [`GcrRotationEngine`] and delegates to it (composition, not
//! a trait), feeding the per-cycle bus/VIA state in through [`RotationContext`]
//! and reading assembled bytes and byte-ready/SYNC edges back out.
//!
//! The engine is codec-agnostic — the *physical* floppy model — while the
//! Commodore GCR codec and the [`DriveGeometry`] it spins over are specific. The
//! only structural difference between the two drives is the physical side: the
//! surface accessors take `side` (always `0` on the single-sided 1541, `0`/`1`
//! on the 1571), threaded through [`RotationContext::side`].

use format_commodore_c64_d64::D64ParseError;
use format_commodore_c64_g64::G64Image;

use crate::{
    GAP_SIZE_BY_ZONE, MAX_HEAD_POSITION, RAW_TRACK_SIZE_BY_ZONE, build_gcr_tracks_from_d64,
    build_gcr_tracks_from_g64, track_slot_index,
};

/// Reference clock sub-cycles per drive CPU cycle. The rotation budget is
/// tracked at 16× the 1 MHz drive clock so a byte's read/write edges land on a
/// sub-cycle boundary rather than being quantised to whole CPU cycles.
const ROTATION_REF_CYCLES_PER_CPU_CYCLE: u64 = 16;
/// Extra reference sub-cycles the read path pays for the VIA bus-read settling
/// delay, on top of the normal per-cycle rotation budget.
const BUS_READ_DELAY_REF_CYCLES: u64 = 14;
/// Nominal drive CPU clock the reference clock is derived from (1 MHz on the
/// 1541 and 1571 alike).
const DRIVE_CPU_HZ: u64 = 1_000_000;
/// Non-zero seed for the weak-bit LFSR so it never starts in a degenerate state.
const WEAK_BIT_SEED: u32 = 0x2545_F491;

/// The physical geometry a Commodore GCR drive spins over: per-zone bit rates
/// and raw track sizes, the inter-sector gap, and the head-position range. The
/// drive constructs this and passes it into the engine, so other in-family GCR
/// drives (4040/2031, 1551, 8050/8250) can reuse the same engine with a
/// different track layout without touching it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriveGeometry {
    /// Surface bit rate (bits/second) for each of the four speed zones.
    pub read_bits_per_second_by_zone: [u64; 4],
    /// Raw GCR bytes per revolution for each speed zone.
    pub raw_track_size_by_zone: [usize; 4],
    /// Inter-sector gap in bytes for each speed zone.
    pub gap_size_by_zone: [usize; 4],
    /// Highest addressable head position (half-track count + 2).
    pub max_head_position: u8,
}

impl DriveGeometry {
    /// The standard Commodore 5.25" GCR geometry shared by the 1541 and 1571:
    /// 35 tracks across four speed zones, 84 half-track head positions. The
    /// raw-track/gap/head-position values are sourced from the module constants
    /// the track builders use, so the geometry has a single source of truth even
    /// though the builders read the constants directly for now.
    pub const COMMODORE_GCR: Self = Self {
        read_bits_per_second_by_zone: [250_000, 266_667, 285_714, 307_692],
        raw_track_size_by_zone: RAW_TRACK_SIZE_BY_ZONE,
        gap_size_by_zone: GAP_SIZE_BY_ZONE,
        max_head_position: MAX_HEAD_POSITION,
    };

    /// The number of addressable track slots (head positions
    /// `2..=max_head_position`).
    #[must_use]
    pub const fn track_slot_count(&self) -> usize {
        (self.max_head_position as usize) - 1
    }
}

/// The live GCR track surface: one entry per physical side (one for the 1541,
/// two for the 1571), each a vector of half-track slots, each slot the raw GCR
/// bytes for that half-track (an empty slot is an unreachable/unformatted
/// track). Read and written through `(head_position, side)` accessors.
#[derive(Clone, Default)]
pub struct GcrSurface {
    sides: Vec<Vec<Vec<u8>>>,
}

impl GcrSurface {
    /// A single-sided surface (the 1541, and the 1571's D64/G64 mounts).
    #[must_use]
    pub fn single(slots: Vec<Vec<u8>>) -> Self {
        Self { sides: vec![slots] }
    }

    /// A double-sided surface (the 1571's D71 mount).
    #[must_use]
    pub fn double(side0: Vec<Vec<u8>>, side1: Vec<Vec<u8>>) -> Self {
        Self {
            sides: vec![side0, side1],
        }
    }

    /// Builds a single-sided surface by GCR-encoding a decoded D64 image.
    ///
    /// # Errors
    ///
    /// Propagates a [`D64ParseError`] if the BAM or any sector can't be read.
    pub fn from_d64(bytes: &[u8]) -> Result<Self, D64ParseError> {
        Ok(Self::single(build_gcr_tracks_from_d64(bytes)?))
    }

    /// Builds a single-sided surface from a parsed raw-GCR G64 image.
    #[must_use]
    pub fn from_g64(image: &G64Image) -> Self {
        Self::single(build_gcr_tracks_from_g64(image))
    }

    /// The raw GCR for the half-track under `head_position` on `side`, or `None`
    /// when the head is off the addressable range, the side is absent, or the
    /// slot is unformatted.
    #[must_use]
    pub fn track_bytes(&self, head_position: u8, side: u8) -> Option<&[u8]> {
        let slot = track_slot_index(head_position)?;
        let track = self.sides.get(usize::from(side))?.get(slot)?;
        if track.is_empty() { None } else { Some(track) }
    }

    /// Mutable raw GCR for the half-track under `head_position` on `side`, for
    /// the write path. `None` under the same conditions as [`track_bytes`].
    ///
    /// [`track_bytes`]: Self::track_bytes
    pub fn track_bytes_mut(&mut self, head_position: u8, side: u8) -> Option<&mut [u8]> {
        let slot = track_slot_index(head_position)?;
        let track = self.sides.get_mut(usize::from(side))?.get_mut(slot)?;
        if track.is_empty() {
            None
        } else {
            Some(track.as_mut_slice())
        }
    }

    /// All half-track slots for `side` (empty slice when the side is absent), so
    /// a flush can walk the whole surface back into an image.
    #[must_use]
    pub fn side_slots(&self, side: u8) -> &[Vec<u8>] {
        self.sides.get(usize::from(side)).map_or(&[], Vec::as_slice)
    }
}

/// The drive-supplied bus/VIA state the engine needs for one rotation advance.
/// These lines are constant across a single advance (the CPU is not running
/// mid-advance), so the drive samples them once and passes them in, rather than
/// the engine reaching back into the VIA/disk/RAM it does not own.
#[derive(Clone, Copy)]
pub struct RotationContext {
    /// VIA2 CB2: the head is reading (assembling bytes) rather than writing.
    pub read_mode: bool,
    /// The mounted disk is present and not write-protected.
    pub writable: bool,
    /// VIA2 CA2: byte-ready is enabled, so an assembled byte pulses the line.
    pub byte_ready_active: bool,
    /// The internal mechanism is selected (else no surface is under the head).
    pub present: bool,
    /// Physical side under the head (`0` on the 1541; `0`/`1` on the 1571).
    pub side: u8,
}

/// The persistent rotation state, moved between the engine and a drive's
/// serialized snapshot. Excludes the transient write-serialiser index/shift
/// (reset on restore) — mirrors the drive snapshots' historical field set so
/// the postcard layout is unchanged.
#[derive(Clone, Copy)]
pub struct RotationState {
    pub head_position: u8,
    pub stepper_phase: u8,
    pub motor_on: bool,
    pub density_code: u8,
    pub gcr_read: u8,
    pub gcr_write_value: u8,
    pub gcr_head_offset: usize,
    pub last_read_data: u16,
    pub bit_counter: u8,
    pub weak_bit_lfsr: u32,
    pub sync_active: bool,
    pub byte_ready_level: bool,
    pub byte_ready_edge: bool,
    pub byte_ready_delay_ref_cycles: u8,
    pub sync_event_count: u64,
    pub byte_ready_event_count: u64,
    pub rotation_accum: u64,
    pub rotation_ref_phase: u8,
}

// The surface never serialises directly (a snapshot rebuilds it from the
// mounted image), so `GcrRotationEngine` deliberately does not derive
// `Serialize`/`Deserialize`; the drive moves persistent state via
// [`RotationState`].
/// The shared GCR rotation/serialiser engine embedded by each drive.
#[derive(Clone)]
pub struct GcrRotationEngine {
    geometry: DriveGeometry,
    surface: GcrSurface,
    head_position: u8,
    stepper_phase: u8,
    motor_on: bool,
    density_code: u8,
    gcr_read: u8,
    gcr_write_value: u8,
    gcr_head_offset: usize,
    last_read_data: u16,
    bit_counter: u8,
    /// LFSR/LCG state feeding weak-bit reads: over a `0x00` (no-flux) GCR byte
    /// the head picks up random flux, so each revolution reads differently.
    weak_bit_lfsr: u32,
    /// Which bit of the write serialiser the head emits next, MSB first.
    /// Transient write-mode state; not snapshotted.
    write_bit_index: u8,
    /// The write serialiser: emits its MSB onto the surface and shifts left each
    /// bit, reloading from the `gcr_write_value` port latch only at the byte
    /// boundary so a mid-byte store by the ROM's one-byte-ahead write loop
    /// cannot corrupt the byte already on its way to the surface. Mirrors VICE
    /// `rotation.c`. Transient write-mode state; not snapshotted.
    write_shift: u8,
    sync_active: bool,
    byte_ready_level: bool,
    byte_ready_edge: bool,
    byte_ready_delay_ref_cycles: u8,
    sync_event_count: u64,
    byte_ready_event_count: u64,
    rotation_accum: u64,
    rotation_ref_phase: u8,
}

impl GcrRotationEngine {
    /// Constructs an engine at power-on for `geometry`, with the head parked at
    /// `initial_head_position`.
    #[must_use]
    pub fn new(geometry: DriveGeometry, initial_head_position: u8) -> Self {
        Self {
            geometry,
            surface: GcrSurface::default(),
            head_position: initial_head_position,
            stepper_phase: 0x03,
            motor_on: false,
            density_code: 0,
            gcr_read: 0x11,
            gcr_write_value: 0,
            gcr_head_offset: 0,
            last_read_data: 0,
            bit_counter: 0,
            weak_bit_lfsr: WEAK_BIT_SEED,
            write_bit_index: 0,
            write_shift: 0,
            sync_active: false,
            byte_ready_level: false,
            byte_ready_edge: false,
            byte_ready_delay_ref_cycles: 0,
            sync_event_count: 0,
            byte_ready_event_count: 0,
            rotation_accum: 0,
            rotation_ref_phase: 0,
        }
    }

    /// Replaces the live track surface (on mount).
    pub fn set_surface(&mut self, surface: GcrSurface) {
        self.surface = surface;
    }

    /// Drops the track surface (on eject / no media).
    pub fn clear_surface(&mut self) {
        self.surface = GcrSurface::default();
    }

    /// The live track surface, for a flush back to an image.
    #[must_use]
    pub const fn surface(&self) -> &GcrSurface {
        &self.surface
    }

    #[must_use]
    pub const fn head_position(&self) -> u8 {
        self.head_position
    }

    #[must_use]
    pub const fn motor_on(&self) -> bool {
        self.motor_on
    }

    #[must_use]
    pub const fn density_code(&self) -> u8 {
        self.density_code
    }

    /// The last byte assembled off the surface, presented on VIA2 Port A.
    #[must_use]
    pub const fn gcr_read(&self) -> u8 {
        self.gcr_read
    }

    /// The byte-ready level (VIA2 CA1 input source).
    #[must_use]
    pub const fn byte_ready_level(&self) -> bool {
        self.byte_ready_level
    }

    /// The pending byte-ready edge (drives the CPU V-flag / overflow).
    #[must_use]
    pub const fn byte_ready_edge(&self) -> bool {
        self.byte_ready_edge
    }

    /// Whether a SYNC mark is currently under the head.
    #[must_use]
    pub const fn sync_active(&self) -> bool {
        self.sync_active
    }

    #[must_use]
    pub const fn sync_event_count(&self) -> u64 {
        self.sync_event_count
    }

    #[must_use]
    pub const fn byte_ready_event_count(&self) -> u64 {
        self.byte_ready_event_count
    }

    /// Latches the byte the ROM's write loop wants emitted next (VIA2 Port A
    /// store in write mode).
    pub const fn set_gcr_write_value(&mut self, value: u8) {
        self.gcr_write_value = value;
    }

    /// Clears the byte-ready level, edge, and pending delay together.
    pub const fn clear_byte_ready(&mut self) {
        self.byte_ready_level = false;
        self.byte_ready_edge = false;
        self.byte_ready_delay_ref_cycles = 0;
    }

    /// Clears only the byte-ready level (a Port A read / mode change).
    pub const fn clear_byte_ready_level(&mut self) {
        self.byte_ready_level = false;
    }

    /// Consumes the byte-ready edge once the drive has applied the CPU overflow.
    pub const fn clear_byte_ready_edge(&mut self) {
        self.byte_ready_edge = false;
    }

    /// Resets the read/write serialiser and byte-ready/SYNC state on mount or
    /// eject. Leaves the head position, stepper phase, motor, and density
    /// untouched (those track the physical mechanism, not the media).
    pub const fn reset_rotation_state(&mut self) {
        self.gcr_read = 0x11;
        self.gcr_write_value = 0;
        self.gcr_head_offset = 0;
        self.last_read_data = 0;
        self.bit_counter = 0;
        self.write_bit_index = 0;
        self.write_shift = 0;
        self.sync_active = false;
        self.byte_ready_level = false;
        self.byte_ready_edge = false;
        self.byte_ready_delay_ref_cycles = 0;
        self.sync_event_count = 0;
        self.byte_ready_event_count = 0;
        self.rotation_accum = 0;
        self.rotation_ref_phase = 0;
    }

    /// Snapshots the persistent rotation state for serialization.
    #[must_use]
    pub const fn state(&self) -> RotationState {
        RotationState {
            head_position: self.head_position,
            stepper_phase: self.stepper_phase,
            motor_on: self.motor_on,
            density_code: self.density_code,
            gcr_read: self.gcr_read,
            gcr_write_value: self.gcr_write_value,
            gcr_head_offset: self.gcr_head_offset,
            last_read_data: self.last_read_data,
            bit_counter: self.bit_counter,
            weak_bit_lfsr: self.weak_bit_lfsr,
            sync_active: self.sync_active,
            byte_ready_level: self.byte_ready_level,
            byte_ready_edge: self.byte_ready_edge,
            byte_ready_delay_ref_cycles: self.byte_ready_delay_ref_cycles,
            sync_event_count: self.sync_event_count,
            byte_ready_event_count: self.byte_ready_event_count,
            rotation_accum: self.rotation_accum,
            rotation_ref_phase: self.rotation_ref_phase,
        }
    }

    /// Restores the persistent rotation state from a snapshot. The transient
    /// write-serialiser index/shift are left as they are (a fresh engine has
    /// them at zero; an in-place restore keeps the live values), matching the
    /// drives' historical restore behaviour.
    pub const fn restore_state(&mut self, state: RotationState) {
        self.head_position = state.head_position;
        self.stepper_phase = state.stepper_phase;
        self.motor_on = state.motor_on;
        self.density_code = state.density_code;
        self.gcr_read = state.gcr_read;
        self.gcr_write_value = state.gcr_write_value;
        self.gcr_head_offset = state.gcr_head_offset;
        self.last_read_data = state.last_read_data;
        self.bit_counter = state.bit_counter;
        self.weak_bit_lfsr = state.weak_bit_lfsr;
        self.sync_active = state.sync_active;
        self.byte_ready_level = state.byte_ready_level;
        self.byte_ready_edge = state.byte_ready_edge;
        self.byte_ready_delay_ref_cycles = state.byte_ready_delay_ref_cycles;
        self.sync_event_count = state.sync_event_count;
        self.byte_ready_event_count = state.byte_ready_event_count;
        self.rotation_accum = state.rotation_accum;
        self.rotation_ref_phase = state.rotation_ref_phase;
    }

    /// Applies a VIA2 Port B write to the drive mechanism: steps the head by the
    /// stepper phase delta, updates the motor and density, and resets the read
    /// assembly on a motor edge. `present`/`side` gate the head-offset
    /// normalisation to the surface currently under the head.
    pub fn apply_mechanics(&mut self, port_b: u8, present: bool, side: u8) {
        let was_motor_on = self.motor_on;
        let new_stepper_position = port_b & 0x03;
        let old_stepper_position = self.head_position.saturating_sub(2) & 0x03;
        let step_count = new_stepper_position.wrapping_sub(old_stepper_position) & 0x03;

        self.motor_on = port_b & 0x04 != 0;
        self.density_code = (port_b >> 5) & 0x03;

        if self.motor_on {
            match step_count {
                1 => {
                    self.head_position = self
                        .head_position
                        .saturating_add(1)
                        .min(self.geometry.max_head_position);
                }
                3 => {
                    self.head_position = self.head_position.saturating_sub(1);
                }
                _ => {}
            }
        }

        if !self.motor_on && was_motor_on {
            self.clear_byte_ready();
            self.last_read_data = 0;
            self.bit_counter = 0;
            self.sync_active = false;
            self.rotation_accum = 0;
            self.rotation_ref_phase = 0;
        } else if self.motor_on && !was_motor_on {
            self.rotation_accum = 0;
            self.rotation_ref_phase = 0;
        }

        self.normalize_head_offset(present, side);
        self.stepper_phase = new_stepper_position;
    }

    /// Wraps the head bit-offset into the current track, or zeroes it when no
    /// track is under the head.
    pub fn normalize_head_offset(&mut self, present: bool, side: u8) {
        let total_bits = self.current_track_bit_len(present, side);
        if total_bits == 0 {
            self.gcr_head_offset = 0;
        } else {
            self.gcr_head_offset %= total_bits;
        }
    }

    /// One drive CPU cycle of rotation: the full per-cycle reference budget,
    /// then the reference phase resets for the next cycle.
    pub fn finish_cpu_cycle(&mut self, ctx: RotationContext) {
        self.advance(ROTATION_REF_CYCLES_PER_CPU_CYCLE, ctx);
        self.rotation_ref_phase = 0;
    }

    /// The extra rotation the read path pays for the VIA bus-read settling
    /// delay, on top of the per-cycle budget (phase is *not* reset here).
    pub fn bus_read_delay(&mut self, ctx: RotationContext) {
        self.advance(BUS_READ_DELAY_REF_CYCLES, ctx);
    }

    /// Advances the surface by `ref_cycles` reference sub-cycles. The disk spins
    /// whenever the motor is on, in read *or* write mode: read mode assembles
    /// bytes off the surface, write mode lays the latch onto it.
    fn advance(&mut self, ref_cycles: u64, ctx: RotationContext) {
        if ref_cycles == 0 || !self.motor_on {
            return;
        }

        let bits_per_second =
            self.geometry.read_bits_per_second_by_zone[usize::from(self.density_code)];
        let ref_hz = DRIVE_CPU_HZ * ROTATION_REF_CYCLES_PER_CPU_CYCLE;
        let mut remaining = ref_cycles;

        while remaining > 0 {
            let to_next_bit = self.ref_cycles_until_next_bit(bits_per_second, ref_hz);
            let to_byte_ready = if self.byte_ready_delay_ref_cycles == 0 {
                u64::MAX
            } else {
                u64::from(self.byte_ready_delay_ref_cycles)
            };
            let step = remaining.min(to_next_bit.min(to_byte_ready));
            debug_assert!(step > 0);

            self.rotation_accum = self
                .rotation_accum
                .saturating_add(bits_per_second.saturating_mul(step));
            self.rotation_ref_phase = self
                .rotation_ref_phase
                .saturating_add(u8::try_from(step).unwrap_or(u8::MAX));
            self.advance_byte_ready_delay_ref_cycles(step);
            remaining -= step;

            if self.rotation_accum >= ref_hz {
                self.rotation_accum -= ref_hz;
                self.rotate_bit(ctx);
            }
        }
    }

    fn ref_cycles_until_next_bit(&self, bits_per_second: u64, ref_hz: u64) -> u64 {
        let remaining = ref_hz.saturating_sub(self.rotation_accum);
        remaining.div_ceil(bits_per_second).max(1)
    }

    fn advance_byte_ready_delay_ref_cycles(&mut self, ref_cycles: u64) {
        if self.byte_ready_delay_ref_cycles == 0 {
            return;
        }

        if ref_cycles >= u64::from(self.byte_ready_delay_ref_cycles) {
            self.byte_ready_delay_ref_cycles = 0;
            self.byte_ready_level = true;
            self.byte_ready_edge = true;
            self.byte_ready_event_count += 1;
        } else {
            self.byte_ready_delay_ref_cycles -= ref_cycles as u8;
        }
    }

    fn schedule_byte_ready(&mut self, byte_ready_active: bool, edge_phase: u8) {
        if !byte_ready_active {
            return;
        }
        let _ = edge_phase;
        self.byte_ready_delay_ref_cycles = 0;
        self.byte_ready_level = true;
        self.byte_ready_edge = true;
        self.byte_ready_event_count += 1;
    }

    fn rotate_bit(&mut self, ctx: RotationContext) {
        let total_bits = self.current_track_bit_len(ctx.present, ctx.side);
        if total_bits == 0 {
            return;
        }

        self.gcr_head_offset += 1;
        if self.gcr_head_offset >= total_bits {
            self.gcr_head_offset = 0;
        }

        if !ctx.read_mode {
            self.write_bit(ctx);
            return;
        }

        // Reading holds the write serialiser at bit 0 and keeps it pre-loaded
        // with the current latch, so the next write phase starts on a byte
        // boundary aligned with the ROM's first latched byte.
        self.write_bit_index = 0;
        self.write_shift = self.gcr_write_value;

        let bit = self.next_read_bit(self.gcr_head_offset, ctx.present, ctx.side);

        self.last_read_data = ((self.last_read_data << 1) | u16::from(bit)) & 0x03FF;
        let sync_now = self.last_read_data == 0x03FF;
        if sync_now {
            if !self.sync_active {
                self.sync_event_count += 1;
            }
            self.sync_active = true;
            self.bit_counter = 0;
            return;
        }

        self.sync_active = false;
        self.bit_counter = self.bit_counter.wrapping_add(1);
        if self.bit_counter == 8 {
            self.bit_counter = 0;
            self.gcr_read = self.last_read_data as u8;
            self.schedule_byte_ready(
                ctx.byte_ready_active,
                self.rotation_ref_phase.saturating_sub(1),
            );
        }
    }

    /// Lays one bit of the write serialiser onto the surface at the head, MSB
    /// first, then shifts left. After eight bits a byte has been written, so the
    /// serialiser reloads from the `gcr_write_value` port latch and byte-ready
    /// pulses to make the ROM's write loop feed the next byte. Writes are
    /// dropped on a protected/absent disk (`ctx.writable`).
    fn write_bit(&mut self, ctx: RotationContext) {
        if ctx.writable {
            let bit = (self.write_shift >> 7) & 0x01;
            let offset = self.gcr_head_offset;
            let head = self.head_position;
            if let Some(track) = self.surface.track_bytes_mut(head, ctx.side) {
                let byte_index = offset / 8;
                let bit_index = 7 - (offset & 0x07);
                if bit != 0 {
                    track[byte_index] |= 1 << bit_index;
                } else {
                    track[byte_index] &= !(1 << bit_index);
                }
            }
        }

        self.write_shift <<= 1;
        self.write_bit_index = (self.write_bit_index + 1) & 0x07;
        if self.write_bit_index == 0 {
            self.write_shift = self.gcr_write_value;
            self.schedule_byte_ready(
                ctx.byte_ready_active,
                self.rotation_ref_phase.saturating_sub(1),
            );
        }
    }

    /// Reads the next surface bit, substituting random flux over a weak byte.
    ///
    /// A `0x00` GCR byte cannot occur in valid GCR (no code has eight zero
    /// bits), so it marks an unformatted/no-flux area; the LFSR makes it read
    /// differently each revolution, as a copy-protection weak-bit check
    /// requires. Non-zero GCR reads back bit-exact.
    fn next_read_bit(&mut self, bit_offset: usize, present: bool, side: u8) -> u8 {
        if self.current_track_byte(bit_offset, present, side) == Some(0) {
            self.weak_bit_lfsr = self
                .weak_bit_lfsr
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            return (self.weak_bit_lfsr >> 31) as u8;
        }
        self.current_track_bit(bit_offset, present, side)
    }

    fn current_track_byte(&self, bit_offset: usize, present: bool, side: u8) -> Option<u8> {
        self.current_track_bytes(present, side)
            .and_then(|track| track.get(bit_offset / 8).copied())
    }

    fn current_track_bit(&self, bit_offset: usize, present: bool, side: u8) -> u8 {
        let Some(track) = self.current_track_bytes(present, side) else {
            return 0;
        };

        let byte_index = bit_offset / 8;
        let bit_index = 7 - (bit_offset & 0x07);
        u8::from(track[byte_index] & (1 << bit_index) != 0)
    }

    fn current_track_bit_len(&self, present: bool, side: u8) -> usize {
        self.current_track_bytes(present, side)
            .map_or(0, |track| track.len() * 8)
    }

    fn current_track_bytes(&self, present: bool, side: u8) -> Option<&[u8]> {
        if !present {
            return None;
        }
        self.surface.track_bytes(self.head_position, side)
    }
}

/// Low-level state access for the drive crates' white-box rotation tests. Gated
/// behind the `test-support` feature so it never reaches production builds; the
/// drives enable it as a dev-dependency feature and wrap these in `#[cfg(test)]`
/// shims that preserve the pre-hoist test API.
#[cfg(feature = "test-support")]
impl GcrRotationEngine {
    pub const fn set_head_position(&mut self, value: u8) {
        self.head_position = value;
    }

    pub const fn set_stepper_phase(&mut self, value: u8) {
        self.stepper_phase = value;
    }

    pub const fn set_motor_on(&mut self, value: bool) {
        self.motor_on = value;
    }

    pub const fn set_density_code(&mut self, value: u8) {
        self.density_code = value;
    }

    pub const fn set_gcr_read(&mut self, value: u8) {
        self.gcr_read = value;
    }

    pub const fn set_gcr_head_offset(&mut self, value: usize) {
        self.gcr_head_offset = value;
    }

    pub const fn set_last_read_data(&mut self, value: u16) {
        self.last_read_data = value;
    }

    pub const fn set_bit_counter(&mut self, value: u8) {
        self.bit_counter = value;
    }

    pub const fn set_write_shift(&mut self, value: u8) {
        self.write_shift = value;
    }

    pub const fn set_write_bit_index(&mut self, value: u8) {
        self.write_bit_index = value;
    }

    pub const fn set_byte_ready_level(&mut self, value: bool) {
        self.byte_ready_level = value;
    }

    pub const fn set_byte_ready_edge(&mut self, value: bool) {
        self.byte_ready_edge = value;
    }

    pub const fn set_byte_ready_delay_ref_cycles(&mut self, value: u8) {
        self.byte_ready_delay_ref_cycles = value;
    }

    pub const fn set_sync_active(&mut self, value: bool) {
        self.sync_active = value;
    }

    pub const fn set_rotation_accum(&mut self, value: u64) {
        self.rotation_accum = value;
    }

    pub const fn set_rotation_ref_phase(&mut self, value: u8) {
        self.rotation_ref_phase = value;
    }

    #[must_use]
    pub const fn stepper_phase(&self) -> u8 {
        self.stepper_phase
    }

    #[must_use]
    pub const fn gcr_write_value(&self) -> u8 {
        self.gcr_write_value
    }

    #[must_use]
    pub const fn gcr_head_offset(&self) -> usize {
        self.gcr_head_offset
    }

    #[must_use]
    pub const fn last_read_data(&self) -> u16 {
        self.last_read_data
    }

    #[must_use]
    pub const fn bit_counter(&self) -> u8 {
        self.bit_counter
    }

    #[must_use]
    pub const fn write_shift(&self) -> u8 {
        self.write_shift
    }

    #[must_use]
    pub const fn write_bit_index(&self) -> u8 {
        self.write_bit_index
    }

    #[must_use]
    pub const fn byte_ready_delay_ref_cycles(&self) -> u8 {
        self.byte_ready_delay_ref_cycles
    }

    #[must_use]
    pub const fn rotation_accum(&self) -> u64 {
        self.rotation_accum
    }

    #[must_use]
    pub const fn rotation_ref_phase(&self) -> u8 {
        self.rotation_ref_phase
    }

    /// Runs one surface-bit rotation (the read/write serialiser step).
    pub fn rotate_one_track_bit(&mut self, ctx: RotationContext) {
        self.rotate_bit(ctx);
    }

    /// Lays one write-serialiser bit onto the surface.
    pub fn write_one_track_bit(&mut self, ctx: RotationContext) {
        self.write_bit(ctx);
    }

    /// Advances the surface by `ref_cycles` reference sub-cycles.
    pub fn advance_rotation_ref_cycles(&mut self, ref_cycles: u64, ctx: RotationContext) {
        self.advance(ref_cycles, ctx);
    }

    /// Schedules a byte-ready pulse (the delay-free path).
    pub fn schedule_byte_ready_now(&mut self, byte_ready_active: bool, edge_phase: u8) {
        self.schedule_byte_ready(byte_ready_active, edge_phase);
    }

    /// The surface bit at `bit_offset` under the head.
    #[must_use]
    pub fn track_bit(&self, bit_offset: usize, present: bool, side: u8) -> u8 {
        self.current_track_bit(bit_offset, present, side)
    }

    /// The next read bit, advancing the weak-bit LFSR over a no-flux byte.
    pub fn read_next_bit(&mut self, bit_offset: usize, present: bool, side: u8) -> u8 {
        self.next_read_bit(bit_offset, present, side)
    }

    /// The bit length of the track under the head.
    #[must_use]
    pub fn track_bit_len(&self, present: bool, side: u8) -> usize {
        self.current_track_bit_len(present, side)
    }

    /// Advances only the pending byte-ready delay by `ref_cycles`.
    pub fn advance_byte_ready_delay(&mut self, ref_cycles: u64) {
        self.advance_byte_ready_delay_ref_cycles(ref_cycles);
    }

    /// The raw GCR under the head, or `None` when no track is present.
    #[must_use]
    pub fn track_bytes_under_head(&self, present: bool, side: u8) -> Option<&[u8]> {
        self.current_track_bytes(present, side)
    }

    /// Mutable access to the live track surface, for tests that write GCR back.
    pub const fn surface_mut(&mut self) -> &mut GcrSurface {
        &mut self.surface
    }
}
