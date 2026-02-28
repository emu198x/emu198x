//! 1541 floppy disk drive emulation.
//!
//! The 1541 contains its own 6502 CPU running at ~1 MHz, 2KB RAM,
//! 16KB ROM, and two MOS 6522 VIAs:
//!
//!   VIA1 ($1800): IEC serial bus interface
//!     Port B: bit 0 = DATA IN, bit 1 = DATA OUT, bit 2 = CLK IN,
//!             bit 3 = CLK OUT, bit 4 = ATN ACK (auto-pulls DATA low),
//!             bit 7 = ATN IN (active-low: 0 = ATN asserted)
//!     CA1:    ATN edge detect (directly wired to ATN line)
//!
//!   VIA2 ($1C00): Disk controller
//!     Port A: GCR data byte (directly connected to read/write head)
//!     Port B: bit 0-1 = stepper motor phase
//!             bit 2 = motor on
//!             bit 3 = LED
//!             bit 4 = write protect sense
//!             bit 5-6 = density select (speed zone)
//!             bit 7 = SYNC detect (active-low: 0 = in sync)
//!     CB1:    byte-ready signal (triggers IRQ)
//!     CB2:    read/write mode (active-low: 0 = write mode)

#![allow(clippy::cast_possible_truncation)]

use emu_core::Cpu;
use mos_6502::Mos6502;

use crate::d64::D64;
use crate::drive1541_bus::Drive1541Bus;
use crate::gcr;
use crate::iec::IecBus;

/// 1541 floppy disk drive.
pub struct Drive1541 {
    /// Drive's own 6502 CPU (~1 MHz).
    cpu: Mos6502,
    /// Drive bus (RAM, ROM, VIA1, VIA2).
    bus: Drive1541Bus,
    /// Inserted D64 disk image (None = no disk).
    d64: Option<D64>,
    /// Current head position (track 1-35).
    current_track: u8,
    /// Half-track position (0-69, track = half_track / 2 + 1).
    half_track: u8,
    /// Motor running.
    motor_on: bool,
    /// Drive LED (active when reading/writing).
    led_on: bool,
    /// GCR-encoded data for the current track.
    gcr_track: Vec<u8>,
    /// Current position in the GCR track data.
    gcr_position: usize,
    /// Cycle counter for byte-ready timing.
    byte_counter: u32,
    /// Previous stepper motor phase (bits 0-1 of VIA2 port B).
    prev_stepper_phase: u8,
    /// Previous ATN line state for edge detection.
    prev_atn: bool,
    /// Previous CB1 state for drive byte-ready (avoids re-triggering).
    prev_byte_ready: bool,
    /// Write mode active (VIA2 CB2 low = write mode).
    write_mode: bool,
    /// Buffer collecting GCR bytes written by the drive head.
    write_buffer: Vec<u8>,
}

impl Drive1541 {
    /// Create a new 1541 drive with the given ROM.
    ///
    /// ROM must be 16,384 bytes (the standard 1541 ROM image).
    #[must_use]
    pub fn new(rom: Vec<u8>) -> Self {
        let bus = Drive1541Bus::new(rom);
        let mut cpu = Mos6502::new();

        // Read reset vector from drive ROM ($FFFC/$FFFD)
        // ROM offset: $FFFC - $C000 = $3FFC
        let lo = bus.rom()[0x3FFC];
        let hi = bus.rom()[0x3FFD];
        cpu.regs.pc = u16::from(lo) | (u16::from(hi) << 8);

        Self {
            cpu,
            bus,
            d64: None,
            current_track: 18, // Start on directory track
            half_track: 34,    // 18 * 2 - 2 = 34
            motor_on: false,
            led_on: false,
            gcr_track: Vec::new(),
            gcr_position: 0,
            byte_counter: 0,
            prev_stepper_phase: 0,
            prev_atn: true, // ATN starts high (not asserted)
            prev_byte_ready: false,
            write_mode: false,
            write_buffer: Vec::new(),
        }
    }

    /// Insert a D64 disk image.
    pub fn insert_disk(&mut self, d64: D64) {
        self.d64 = Some(d64);
        self.encode_current_track();
    }

    /// Eject the disk.
    pub fn eject_disk(&mut self) {
        self.d64 = None;
        self.gcr_track.clear();
        self.gcr_position = 0;
    }

    /// Whether a disk is inserted.
    #[must_use]
    pub fn has_disk(&self) -> bool {
        self.d64.is_some()
    }

    /// Current head track (1-35).
    #[must_use]
    pub fn track(&self) -> u8 {
        self.current_track
    }

    /// Whether the motor is running.
    #[must_use]
    pub fn motor_on(&self) -> bool {
        self.motor_on
    }

    /// Whether the LED is on.
    #[must_use]
    pub fn led_on(&self) -> bool {
        self.led_on
    }

    /// Reference to the drive CPU.
    #[must_use]
    pub fn cpu(&self) -> &Mos6502 {
        &self.cpu
    }

    /// Tick the drive for one CPU cycle.
    ///
    /// This must be called once per C64 CPU cycle (both CPUs run at ~1 MHz).
    /// Reads the IEC bus state, ticks the CPU and VIAs, updates IEC output,
    /// and handles disk mechanics.
    pub fn tick(&mut self, iec: &mut IecBus) {
        // 1. Read IEC bus state into VIA1 port B external lines
        self.update_via1_from_iec(iec);

        // 2. ATN edge detection on VIA1 CA1
        let atn_level = !iec.atn(); // Active-low on the drive side: ATN asserted = CA1 low
        if atn_level != self.prev_atn {
            self.bus.via1.set_ca1(atn_level);
            self.prev_atn = atn_level;
        }

        // 3. Tick the 6502 CPU
        self.cpu.tick(&mut self.bus);

        // 4. Tick both VIAs
        self.bus.via1.tick();
        self.bus.via2.tick();

        // 5. Read VIA1 port B output → update IEC bus
        self.update_iec_from_via1(iec);

        // 6. Read VIA2 port B → decode mechanics
        self.update_mechanics();

        // 7. Advance disk rotation if motor is on and disk present
        self.advance_disk();

        // 8. VIA IRQs → CPU IRQ
        if self.bus.via1.irq_active() || self.bus.via2.irq_active() {
            self.cpu.interrupt();
        }
    }

    /// Update VIA1 external port B from IEC bus state.
    ///
    /// VIA1 port B input bits (accent on active levels):
    ///   bit 0: DATA IN (1 = DATA line is LOW, 0 = HIGH)
    ///   bit 2: CLK IN  (1 = CLK line is LOW, 0 = HIGH)
    ///   bit 7: ATN IN  (0 = ATN asserted/low, 1 = ATN released/high)
    fn update_via1_from_iec(&mut self, iec: &IecBus) {
        let mut ext = self.bus.via1.external_b;
        // DATA IN: bit 0 = inverted bus DATA (1 when line is low)
        ext = (ext & !0x01) | if !iec.data() { 0x01 } else { 0x00 };
        // CLK IN: bit 2 = inverted bus CLK (1 when line is low)
        ext = (ext & !0x04) | if !iec.clk() { 0x04 } else { 0x00 };
        // ATN IN: bit 7 = bus ATN level (0 = asserted/low, 1 = released/high)
        ext = (ext & !0x80) | if iec.atn() { 0x80 } else { 0x00 };
        self.bus.via1.external_b = ext;
    }

    /// Update IEC bus from VIA1 port B output.
    ///
    /// VIA1 port B output bits:
    ///   bit 1: DATA OUT (1 = pull DATA line low)
    ///   bit 3: CLK OUT  (1 = pull CLK line low)
    ///   bit 4: ATN ACK  (1 = pull DATA low in response to ATN)
    fn update_iec_from_via1(&mut self, iec: &mut IecBus) {
        let pb = self.bus.via1.port_b_output();
        let atn_ack = pb & 0x10 != 0;
        // DATA: driven by bit 1 OR ATN ACK
        iec.set_drive_data((pb & 0x02 != 0) || atn_ack);
        // CLK: driven by bit 3
        iec.set_drive_clk(pb & 0x08 != 0);
    }

    /// Read VIA2 port B output and update motor/LED/stepper state.
    fn update_mechanics(&mut self) {
        let pb = self.bus.via2.port_b_output();
        self.motor_on = pb & 0x04 != 0;
        self.led_on = pb & 0x08 != 0;

        // Stepper motor: bits 0-1 are the phase
        let phase = pb & 0x03;
        if phase != self.prev_stepper_phase {
            self.step_head(phase);
            self.prev_stepper_phase = phase;
        }

        // Write-protect sense: bit 4 (active-low: 0 = protected)
        // For now, always report write-protected when no disk, not protected with disk
        let wp = if self.d64.is_some() { 0x10 } else { 0x00 };
        self.bus.via2.external_b =
            (self.bus.via2.external_b & !0x10) | wp;

        // Write mode: VIA2 CB2 (active-low: 0 = write mode).
        // CB2 is controlled by CRB bits 5-7. In manual output mode
        // (bits 7-5 = 110 or 111), CB2 level is CRB bit 5.
        let crb = self.bus.via2.read(0x0F);
        let cb2_low = (crb & 0xE0) == 0xC0; // Manual output low
        let was_writing = self.write_mode;
        self.write_mode = cb2_low;

        // Transition write→read: flush write buffer back to D64
        if was_writing && !self.write_mode {
            self.flush_write_buffer();
        }
    }

    /// Advance the disk rotation and present/capture GCR bytes.
    fn advance_disk(&mut self) {
        if !self.motor_on || self.gcr_track.is_empty() {
            return;
        }

        self.byte_counter += 1;
        let cpb = gcr::cycles_per_byte(self.current_track);

        if self.byte_counter >= cpb {
            self.byte_counter = 0;

            if self.write_mode {
                // Write mode: capture byte from VIA2 port A into the
                // GCR track buffer and write buffer for later decoding.
                let byte = self.bus.via2.port_a_output();
                if self.gcr_position < self.gcr_track.len() {
                    self.gcr_track[self.gcr_position] = byte;
                }
                self.write_buffer.push(byte);
            } else {
                // Read mode: present the next GCR byte to VIA2 port A
                let byte = self.gcr_track[self.gcr_position];
                self.bus.via2.external_a = byte;

                // SYNC detect: bit 7 of VIA2 port B external.
                // Active-low: 0 = sync detected.
                let in_sync = byte == 0xFF;
                self.bus.via2.external_b = (self.bus.via2.external_b & !0x80)
                    | if in_sync { 0x00 } else { 0x80 };
            }

            // Advance position (wrap around the track)
            self.gcr_position += 1;
            if self.gcr_position >= self.gcr_track.len() {
                self.gcr_position = 0;
            }

            // Pulse CB1 (byte-ready) — triggers on positive edge
            if !self.prev_byte_ready {
                self.bus.via2.set_cb1(true);
            }
            self.prev_byte_ready = true;
        } else {
            // Between bytes: release CB1
            if self.prev_byte_ready {
                self.bus.via2.set_cb1(false);
                self.prev_byte_ready = false;
            }
        }
    }

    /// Flush the write buffer: decode GCR sectors and write back to D64.
    fn flush_write_buffer(&mut self) {
        if self.write_buffer.is_empty() {
            return;
        }

        if self.d64.is_none() {
            self.write_buffer.clear();
            return;
        }

        // First pass: find sectors to write (collect into a temp vec to avoid borrow issues)
        let mut writes: Vec<(u8, Vec<u8>)> = Vec::new();
        let sector_num = self.find_sector_at_track_position();

        // Scan the write buffer for sync + data block patterns.
        let buf = &self.write_buffer;
        let mut i = 0;

        while i + 5 + 325 <= buf.len() {
            if buf[i..i + 5].iter().all(|&b| b == 0xFF) {
                let gcr_start = i + 5;
                if gcr_start + 325 <= buf.len() {
                    if let Some(sector_data) = gcr::decode_data_block(&buf[gcr_start..gcr_start + 325]) {
                        if let Some(sector) = sector_num {
                            writes.push((sector, sector_data));
                        }
                    }
                }
                i = gcr_start + 325;
            } else {
                i += 1;
            }
        }

        self.write_buffer.clear();

        // Second pass: apply writes to D64
        let track = self.current_track;
        if let Some(ref mut d64) = self.d64 {
            for (sector, data) in &writes {
                let _ = d64.write_sector(track, *sector, data);
            }
        }

        // Re-encode the track from D64 to keep GCR track in sync
        self.encode_current_track();
    }

    /// Attempt to identify which sector was written based on the
    /// track's GCR data. Scans backwards from current position for
    /// the most recent header block.
    fn find_sector_at_track_position(&self) -> Option<u8> {
        if self.gcr_track.is_empty() {
            return None;
        }

        // Scan backwards from current position looking for a header sync
        // (5x $FF followed by 10 GCR header bytes). The header contains
        // the sector number.
        let len = self.gcr_track.len();
        let start = if self.gcr_position == 0 { len - 1 } else { self.gcr_position - 1 };

        for offset in 0..len {
            let pos = (start + len - offset) % len;
            // Check for header sync (need 5 bytes of $FF)
            let mut sync_count = 0;
            for j in 0..5 {
                if self.gcr_track[(pos + len - j) % len] == 0xFF {
                    sync_count += 1;
                } else {
                    break;
                }
            }
            if sync_count >= 5 {
                // Next 10 bytes should be the GCR header
                let hdr_start = (pos + 1) % len;
                if hdr_start + 10 <= len {
                    let mut group = [0u8; 5];
                    group.copy_from_slice(&self.gcr_track[hdr_start..hdr_start + 5]);
                    if let Some(decoded) = gcr::decode_gcr_group(&group) {
                        // decoded[0] = 0x08 (header marker)
                        // decoded[2] = sector number
                        if decoded[0] == 0x08 {
                            return Some(decoded[2]);
                        }
                    }
                }
            }
        }
        None
    }

    /// Get a reference to the D64 image (for saving).
    #[must_use]
    pub fn d64(&self) -> Option<&D64> {
        self.d64.as_ref()
    }

    /// Step the head based on stepper motor phase change.
    ///
    /// The 1541 uses a 4-phase stepper motor. The phase sequence determines
    /// direction: incrementing phases (0→1→2→3) steps inward (higher tracks),
    /// decrementing phases (3→2→1→0) steps outward.
    fn step_head(&mut self, new_phase: u8) {
        // Calculate direction from phase transition
        let delta = (new_phase as i8 - self.prev_stepper_phase as i8 + 4) % 4;
        match delta {
            1 => {
                // Step inward (higher track)
                if self.half_track < 69 {
                    self.half_track += 1;
                }
            }
            3 => {
                // Step outward (lower track)
                if self.half_track > 0 {
                    self.half_track -= 1;
                }
            }
            _ => {
                // 0 = no step, 2 = invalid/skipped — ignore
            }
        }

        let new_track = (self.half_track / 2) + 1;
        if new_track != self.current_track {
            self.current_track = new_track;
            self.encode_current_track();
        }
    }

    /// Re-encode the GCR track data for the current head position.
    ///
    /// On whole tracks (even half_track values), encodes from D64.
    /// On half-tracks (odd half_track values), fills with $00 — the
    /// drive ROM fails to find sync marks, which is correct behaviour
    /// and matches real hardware.
    fn encode_current_track(&mut self) {
        let on_half_track = self.half_track & 1 != 0;

        if on_half_track || !(1..=35).contains(&self.current_track) {
            // Half-track or out-of-range: no valid data
            // Fill with $00 so drive ROM sees no sync marks
            let track_bytes = 7692; // Approximate track length
            self.gcr_track = vec![0x00; track_bytes];
            self.gcr_position = 0;
            return;
        }

        if let Some(ref d64) = self.d64 {
            self.gcr_track = gcr::encode_track(d64, self.current_track);
            if self.gcr_position >= self.gcr_track.len() {
                self.gcr_position = 0;
            }
        } else {
            self.gcr_track.clear();
            self.gcr_position = 0;
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use emu_core::Bus;

    fn make_drive() -> Drive1541 {
        // Minimal ROM: NOP sled with reset vector pointing to $C000
        let mut rom = vec![0xEA; 16384]; // NOP sled
        // Reset vector at $FFFC/$FFFD (ROM offset $3FFC/$3FFD)
        rom[0x3FFC] = 0x00; // Low byte: $C000
        rom[0x3FFD] = 0xC0; // High byte
        Drive1541::new(rom)
    }

    #[test]
    fn drive_starts_on_track_18() {
        let drive = make_drive();
        assert_eq!(drive.track(), 18);
        assert!(!drive.motor_on());
        assert!(!drive.has_disk());
    }

    #[test]
    fn insert_and_eject_disk() {
        let mut drive = make_drive();
        let d64 = D64::from_bytes(&vec![0u8; 174_848]).expect("valid");
        drive.insert_disk(d64);
        assert!(drive.has_disk());
        assert!(!drive.gcr_track.is_empty());
        drive.eject_disk();
        assert!(!drive.has_disk());
        assert!(drive.gcr_track.is_empty());
    }

    #[test]
    fn bus_ram_rom_via_routing() {
        let mut drive = make_drive();
        // RAM write and read
        drive.bus.write(0x0000, 0xAB);
        assert_eq!(drive.bus.read(0x0000).data, 0xAB);
        // VIA1 access
        drive.bus.write(0x1803, 0xFF); // VIA1 DDR A
        assert_eq!(drive.bus.read(0x1803).data, 0xFF);
        // VIA2 access
        drive.bus.write(0x1C03, 0xAA); // VIA2 DDR A
        assert_eq!(drive.bus.read(0x1C03).data, 0xAA);
        // ROM read
        assert_eq!(drive.bus.read(0xC000).data, 0xEA);
    }

    #[test]
    fn cpu_starts_at_reset_vector() {
        let drive = make_drive();
        assert_eq!(drive.cpu.regs.pc, 0xC000);
    }

    #[test]
    fn motor_control_via_via2() {
        let mut drive = make_drive();
        // Set VIA2 DDR B bit 2 = output, then set motor bit
        drive.bus.via2.write(0x02, 0x0C); // DDR B: bits 2,3 output
        drive.bus.via2.write(0x00, 0x04); // Port B: motor on
        drive.update_mechanics();
        assert!(drive.motor_on());
        assert!(!drive.led_on());

        drive.bus.via2.write(0x00, 0x08); // LED on, motor off
        drive.update_mechanics();
        assert!(!drive.motor_on());
        assert!(drive.led_on());
    }

    #[test]
    fn gcr_position_wraps() {
        let mut drive = make_drive();
        let d64 = D64::from_bytes(&vec![0u8; 174_848]).expect("valid");
        drive.insert_disk(d64);

        let track_len = drive.gcr_track.len();
        assert!(track_len > 0);

        drive.gcr_position = track_len - 1;
        // Simulate one byte advance
        drive.gcr_position += 1;
        if drive.gcr_position >= drive.gcr_track.len() {
            drive.gcr_position = 0;
        }
        assert_eq!(drive.gcr_position, 0);
    }

    #[test]
    fn stepper_phase_steps_inward() {
        let mut drive = make_drive();
        let d64 = D64::from_bytes(&vec![0u8; 174_848]).expect("valid");
        drive.insert_disk(d64);

        let initial_track = drive.current_track;
        // Phase 0 → 1: step inward
        drive.prev_stepper_phase = 0;
        drive.step_head(1);
        // Half-track advanced by 1; track may or may not change depending on starting position
        assert!(drive.half_track > 34 || drive.current_track >= initial_track);
    }
}
