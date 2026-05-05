//! Amiga floppy drive mechanism emulator.
//!
//! Emulates the physical drive: head positioning, motor control, disk
//! change detection, and MFM track encoding. Control signals come from
//! CIA-B port B; status signals feed back to CIA-A port A.

pub mod mfm;

use format_commodore_amiga_adf::Adf;
use mfm::{decode_mfm_track, encode_mfm_track};
use serde::{Deserialize, Serialize};

/// Trait abstracting the disk data source.
///
/// Implemented by `AdfDiskImage` and `IpfImage` (in the `format-ipf` crate).
/// The floppy drive holds a `Box<dyn DiskImage>` and delegates track encoding
/// and write-back through this interface.
pub trait DiskImage: Send {
    /// Encode the specified track as raw MFM bytes for the drive read path.
    fn encode_mfm_track(&self, cyl: u32, head: u32) -> Option<Vec<u8>>;

    /// Number of sectors per track (11 for DD, 22 for HD).
    fn sectors_per_track(&self) -> u32;

    /// Whether the image supports writing.
    fn is_writable(&self) -> bool;

    /// Write a decoded sector back to the image.
    fn write_sector(&mut self, cyl: u32, head: u32, sector: u32, data: &[u8]);

    /// Serialise the current image state for saving (e.g. ADF bytes).
    /// Returns `None` for read-only formats like IPF.
    fn save_data(&self) -> Option<Vec<u8>>;
}

/// ADF disk image wrapper implementing `DiskImage`.
pub struct AdfDiskImage {
    adf: Adf,
}

impl AdfDiskImage {
    pub fn new(adf: Adf) -> Self {
        Self { adf }
    }
}

impl DiskImage for AdfDiskImage {
    fn encode_mfm_track(&self, cyl: u32, head: u32) -> Option<Vec<u8>> {
        let track_num = (cyl * 2 + head) as u8;
        let sectors = self.adf.read_track_sectors(cyl, head);
        Some(encode_mfm_track(
            sectors,
            track_num,
            self.adf.sectors_per_track(),
        ))
    }

    fn sectors_per_track(&self) -> u32 {
        self.adf.sectors_per_track()
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn write_sector(&mut self, cyl: u32, head: u32, sector: u32, data: &[u8]) {
        self.adf.write_sector(cyl, head, sector, data);
    }

    fn save_data(&self) -> Option<Vec<u8>> {
        Some(self.adf.data().to_vec())
    }
}

/// E-clock ticks for motor spin-up (~500ms at 709 kHz).
const MOTOR_SPINUP_TICKS: u32 = 350_000;
/// E-clock ticks per disk revolution at 300 RPM (~200ms at 709 kHz).
const INDEX_PULSE_TICKS: u32 = 141_876;

/// Drive status bits for CIA-A PRA (active-low: 0 = asserted).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DriveStatus {
    /// PA2: /DSKCHANGE — low when disk has been removed since last step.
    pub disk_change: bool,
    /// PA3: /DSKPROT — low when disk is write-protected.
    pub write_protect: bool,
    /// PA4: /DSKTRACK0 — low when head is at cylinder 0.
    pub track0: bool,
    /// PA5: /DSKRDY — low when motor is at speed.
    pub ready: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AmigaFloppyDrive {
    /// Inserted disk image, if any. Not serialised — disk media is
    /// re-mounted by the runtime envelope on snapshot restore so that
    /// snapshots stay small and reference disks by source path rather
    /// than embedded bytes.
    #[serde(skip)]
    disk: Option<Box<dyn DiskImage>>,
    cylinder: u32,
    head: u32,
    motor_on: bool,
    motor_spinning: bool,
    spin_timer: u32,
    index_timer: u32,
    selected: bool,
    disk_changed: bool,
    prev_step: bool,
    /// Monotonically increasing counter of head step events.
    /// Used by the drive sound generator to detect new seeks.
    step_event_counter: u32,
    /// Observable capture log — all MFM words written to the drive.
    /// Cleared only by explicit `clear_write_mfm_capture()`.
    write_mfm_capture: Vec<u16>,
    /// Pending decode buffer — consumed and cleared by `flush_write_capture()`.
    write_mfm_pending: Vec<u16>,
    /// Drive-ID shift register. Per Amiga HRM §Floppy / Device I.D.:
    /// while the motor is OFF, each DSKSEL falling edge shifts one bit
    /// out through /DSKRDY (MSB first). The Amiga reference value is
    /// $FFFFFFFF, identifying a 3.5" drive.
    id_shift_register: u32,
    /// Next bit to shift out (0..=31). Reloaded to 0 on motor-on.
    id_bit: u8,
    /// Latched /DSKRDY output bit — 0 = asserted (ready/id-0), 1 = deasserted.
    id_ready_bit: bool,
}

impl Clone for AmigaFloppyDrive {
    /// Cloning drops the inserted disk — matching the `#[serde(skip)]`
    /// snapshot semantics. Disk media is re-mounted at the runtime
    /// layer after snapshot restore (or after clone, where applicable);
    /// the drive's persistable mechanical state — head, motor, MFM
    /// capture, ID shift register — copies bit-for-bit.
    fn clone(&self) -> Self {
        Self {
            disk: None,
            cylinder: self.cylinder,
            head: self.head,
            motor_on: self.motor_on,
            motor_spinning: self.motor_spinning,
            spin_timer: self.spin_timer,
            index_timer: self.index_timer,
            selected: self.selected,
            disk_changed: self.disk_changed,
            prev_step: self.prev_step,
            step_event_counter: self.step_event_counter,
            write_mfm_capture: self.write_mfm_capture.clone(),
            write_mfm_pending: self.write_mfm_pending.clone(),
            id_shift_register: self.id_shift_register,
            id_bit: self.id_bit,
            id_ready_bit: self.id_ready_bit,
        }
    }
}

impl AmigaFloppyDrive {
    pub fn new() -> Self {
        Self {
            disk: None,
            cylinder: 0,
            head: 0,
            motor_on: false,
            motor_spinning: false,
            spin_timer: 0,
            index_timer: 0,
            selected: false,
            disk_changed: true, // No disk at power-on
            prev_step: true,    // Active-low: idle = high
            step_event_counter: 0,
            write_mfm_capture: Vec::new(),
            write_mfm_pending: Vec::new(),
            id_shift_register: 0xFFFF_FFFF,
            id_bit: 0,
            id_ready_bit: true, // MSB of $FFFFFFFF = 1 (deasserted)
        }
    }

    /// Insert an ADF disk image (convenience wrapper).
    pub fn insert_disk(&mut self, adf: Adf) {
        self.disk = Some(Box::new(AdfDiskImage::new(adf)));
        self.disk_changed = true;
    }

    /// Insert any disk image implementing `DiskImage`.
    pub fn insert_disk_image(&mut self, image: Box<dyn DiskImage>) {
        self.disk = Some(image);
        self.disk_changed = true;
    }

    pub fn eject_disk(&mut self) {
        self.disk = None;
        self.disk_changed = true;
    }

    /// Acknowledge an already-present disk so `/DSKCHANGE` reads inactive.
    ///
    /// This is useful when the emulator starts with media already mounted:
    /// the user expectation is "disk is already in the drive", not "a new
    /// insertion is still waiting for the first post-insert step pulse".
    pub fn acknowledge_disk_change(&mut self) {
        if self.disk.is_some() {
            self.disk_changed = false;
        }
    }

    /// Update control signals from CIA-B PRB.
    /// All active-low: the boolean parameters are true when the signal
    /// is asserted (pin driven low).
    pub fn update_control(
        &mut self,
        step: bool,
        dir_inward: bool,
        side_upper: bool,
        sel: bool,
        motor: bool,
    ) {
        let sel_falling_edge = sel && !self.selected;

        // Drive select updates the current motor state while asserted.
        if sel {
            self.selected = true;
            let motor_on_before = self.motor_on;
            self.motor_on = motor;
            if motor && !motor_on_before {
                // Motor turning on — reset the ID shift cursor so the
                // next motor-off + select sequence starts a fresh ID
                // stream at bit 31 (MSB), and start the spin-up timer
                // from zero. Resetting the timer on every PRB write
                // (the previous behaviour) pegged it at zero whenever
                // the ROM polled the drive during spin-up — KS 1.3
                // tolerated that, KS 2.04 polls aggressively enough
                // that the motor never reached spin-up speed and the
                // boot block was never read.
                self.id_bit = 0;
                if !self.motor_spinning {
                    self.spin_timer = 0;
                }
            }
            if !motor {
                self.motor_spinning = false;
                self.spin_timer = 0;
                self.index_timer = 0;
            }
        } else {
            self.selected = false;
        }

        // Per Amiga HRM §Device I.D.: with motor OFF, each DSKSEL
        // falling edge shifts one bit out of the ID register onto
        // /DSKRDY (MSB first). With motor ON the /DSKRDY line reflects
        // the motor-at-speed signal instead.
        if sel_falling_edge && !self.motor_on {
            let bit = (self.id_shift_register >> (31 - self.id_bit)) & 1;
            self.id_ready_bit = bit != 0;
            self.id_bit = (self.id_bit + 1) & 31;
        }

        // Only the selected drive responds to side select changes.
        if self.selected {
            // Head side: 0 = upper (head 1), 1 = lower (head 0)
            // The parameter is already decoded: side_upper = true means DSKSIDE* asserted (low)
            self.head = if side_upper { 1 } else { 0 };
        }

        // Step on falling edge (prev was high/deasserted, now low/asserted)
        let step_edge = step && !self.prev_step;
        self.prev_step = step;

        if step_edge && self.selected {
            self.step_event_counter = self.step_event_counter.wrapping_add(1);
            if dir_inward {
                if self.cylinder < 79 {
                    self.cylinder += 1;
                }
            } else if self.cylinder > 0 {
                self.cylinder -= 1;
            }
            // Any step pulse clears DSKCHANGE when a disk is present
            if self.disk.is_some() {
                self.disk_changed = false;
            }
        }
    }

    /// Advance motor spin-up and rotational timing. Call at E-clock rate.
    /// Returns `true` when the spinning drive emits one index pulse.
    pub fn tick(&mut self) -> bool {
        if self.motor_on && !self.motor_spinning {
            self.spin_timer += 1;
            if self.spin_timer >= MOTOR_SPINUP_TICKS {
                self.motor_spinning = true;
                self.index_timer = 0;
            }
            return false;
        }

        if !(self.selected && self.motor_spinning && self.disk.is_some()) {
            if !self.motor_spinning || !self.motor_on {
                self.index_timer = 0;
            }
            return false;
        }

        self.index_timer += 1;
        if self.index_timer >= INDEX_PULSE_TICKS {
            self.index_timer = 0;
            return true;
        }

        false
    }

    /// Current drive status for CIA-A PRA input.
    /// All values are active-low booleans (true = signal asserted = pin low).
    /// - ready: with motor ON, true when spinning. With motor OFF, reflects
    ///   the last bit shifted out of the ID register. Kickstart's trackdisk
    ///   relies on the ID stream to determine drive type before trusting
    ///   /DSKRDY as a speed signal.
    pub fn status(&self) -> DriveStatus {
        let ready = if self.motor_on {
            self.motor_spinning
        } else {
            // Latched ID bit: 0 = asserted (line low), 1 = deasserted.
            // DriveStatus.ready is "true = asserted = pin low"; invert.
            !self.id_ready_bit
        };
        DriveStatus {
            disk_change: self.disk_changed,
            write_protect: false, // Not write-protected
            track0: self.cylinder == 0,
            ready,
        }
    }

    /// Encode the current track as raw MFM data. Returns `None` if no disk.
    pub fn encode_mfm_track(&self) -> Option<Vec<u8>> {
        self.disk
            .as_ref()?
            .encode_mfm_track(self.cylinder, self.head)
    }

    pub fn read_data_available(&self) -> bool {
        self.selected && self.motor_spinning && self.disk.is_some()
    }

    pub fn has_disk(&self) -> bool {
        self.disk.is_some()
    }

    pub fn selected(&self) -> bool {
        self.selected
    }

    pub fn cylinder(&self) -> u32 {
        self.cylinder
    }

    pub fn head(&self) -> u32 {
        self.head
    }

    pub fn motor_on(&self) -> bool {
        self.motor_on
    }

    pub fn motor_spinning(&self) -> bool {
        self.motor_spinning
    }

    pub fn step_event_counter(&self) -> u32 {
        self.step_event_counter
    }

    /// Record one raw MFM word presented to the drive write path.
    ///
    /// This is a simplified capture buffer until full magnetic write
    /// persistence is modeled.
    pub fn note_write_mfm_word(&mut self, word: u16) {
        self.write_mfm_capture.push(word);
        self.write_mfm_pending.push(word);
    }

    pub fn write_mfm_capture(&self) -> &[u16] {
        &self.write_mfm_capture
    }

    pub fn clear_write_mfm_capture(&mut self) {
        self.write_mfm_capture.clear();
        self.write_mfm_pending.clear();
    }

    /// Decode captured MFM write data and persist decoded sectors to the disk image.
    ///
    /// Returns the number of sectors successfully written back.
    pub fn flush_write_capture(&mut self) -> usize {
        if self.write_mfm_pending.is_empty() {
            return 0;
        }

        let decoded = decode_mfm_track(&self.write_mfm_pending);
        self.write_mfm_pending.clear();

        let image = match self.disk.as_mut() {
            Some(img) if img.is_writable() => img,
            _ => return 0,
        };

        let spt = image.sectors_per_track();
        let mut written = 0;
        for sector in &decoded {
            let track_num = sector.track as u32;
            let cyl = track_num / 2;
            let head = track_num % 2;
            if cyl < 80 && (sector.sector as u32) < spt {
                image.write_sector(cyl, head, sector.sector as u32, &sector.data);
                written += 1;
            }
        }
        written
    }

    /// Return the current disk image as raw bytes, or `None` if no disk is
    /// inserted or the format doesn't support saving.
    pub fn save_adf(&self) -> Option<Vec<u8>> {
        self.disk.as_ref().and_then(|img| img.save_data())
    }
}

impl Default for AmigaFloppyDrive {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_toward_center() {
        let mut drive = AmigaFloppyDrive::new();
        assert_eq!(drive.cylinder(), 0);

        // Select drive, motor on, direction inward
        drive.update_control(false, true, false, true, true);
        // Step pulse: deasserted -> asserted (falling edge)
        drive.update_control(true, true, false, true, true);
        assert_eq!(drive.cylinder(), 1);
    }

    #[test]
    fn step_toward_edge() {
        let mut drive = AmigaFloppyDrive::new();
        // First move inward
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        assert_eq!(drive.cylinder(), 2);

        // Now step toward edge (dir_inward = false)
        drive.update_control(false, false, false, true, true);
        drive.update_control(true, false, false, true, true);
        assert_eq!(drive.cylinder(), 1);
    }

    #[test]
    fn no_step_below_zero() {
        let mut drive = AmigaFloppyDrive::new();
        drive.update_control(false, false, false, true, true);
        drive.update_control(true, false, false, true, true);
        assert_eq!(drive.cylinder(), 0);
    }

    #[test]
    fn no_step_above_79() {
        let mut drive = AmigaFloppyDrive::new();
        // Move to track 79
        for _ in 0..80 {
            drive.update_control(false, true, false, true, true);
            drive.update_control(true, true, false, true, true);
        }
        assert_eq!(drive.cylinder(), 79);
    }

    #[test]
    fn track0_status() {
        let mut drive = AmigaFloppyDrive::new();
        drive.update_control(false, false, false, true, true);
        assert!(drive.status().track0);
    }

    #[test]
    fn motor_off_select_shifts_id_stream_through_ready() {
        // Per Amiga HRM §Device I.D., the drive shifts $FFFFFFFF out of
        // /DSKRDY on each DSKSEL falling edge while motor is OFF.
        // For a standard 3.5" drive the ID is all-ones, so every shifted
        // bit should leave /DSKRDY deasserted (ready=false in our
        // "asserted low" boolean).
        let mut drive = AmigaFloppyDrive::new();
        for _ in 0..32 {
            // Select with motor off (sel=true, motor=false) triggers a shift.
            drive.update_control(false, false, false, true, false);
            assert!(
                !drive.status().ready,
                "all-ones ID stream should leave /DSKRDY deasserted"
            );
            // Deselect between pulses so the next select is a fresh falling edge.
            drive.update_control(false, false, false, false, false);
        }
    }

    #[test]
    fn motor_on_bypasses_id_stream_and_reports_spin_state() {
        let mut drive = AmigaFloppyDrive::new();
        // Motor turning on must reset the ID cursor.
        drive.update_control(false, false, false, true, true);
        assert!(!drive.status().ready); // not spinning yet
        for _ in 0..MOTOR_SPINUP_TICKS {
            drive.tick();
        }
        assert!(drive.status().ready); // now spinning
    }

    #[test]
    fn deselected_drive_still_reports_mechanical_status() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();

        let status = drive.status();
        assert!(!status.disk_change);
        assert!(!status.write_protect);
        assert!(status.track0);
        assert!(!status.ready);
    }

    #[test]
    fn motor_spinup() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);
        assert!(!drive.status().ready);

        for _ in 0..MOTOR_SPINUP_TICKS {
            assert!(!drive.tick());
        }
        assert!(drive.status().ready);
    }

    /// Repeated motor-on PRB writes during spin-up must not reset the
    /// spin timer. KS 2.04 polls the drive aggressively while waiting
    /// for /DSKRDY; the previous behaviour pegged the timer at 0 and
    /// left the drive permanently spun-down.
    #[test]
    fn spinup_completes_under_repeated_motor_on_writes() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);

        // Poll the motor-on PRB pattern every tick while spin-up runs.
        for _ in 0..MOTOR_SPINUP_TICKS {
            drive.update_control(false, false, false, true, true);
            drive.tick();
        }
        assert!(drive.status().ready, "motor must reach spin-up under polling");
    }

    #[test]
    fn spun_up_selected_drive_emits_index_pulse_once_per_revolution() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);
        for _ in 0..MOTOR_SPINUP_TICKS {
            assert!(!drive.tick());
        }
        for _ in 0..(INDEX_PULSE_TICKS - 1) {
            assert!(!drive.tick());
        }
        assert!(drive.tick());
    }

    #[test]
    fn spun_up_deselected_drive_stops_emitting_index_pulses() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);
        for _ in 0..MOTOR_SPINUP_TICKS {
            assert!(!drive.tick());
        }
        drive.update_control(false, false, false, false, true);
        for _ in 0..INDEX_PULSE_TICKS {
            assert!(!drive.tick());
        }
    }

    #[test]
    fn motor_state_updates_while_selected() {
        let mut drive = AmigaFloppyDrive::new();
        drive.update_control(false, false, false, true, true);
        assert!(drive.motor_on());

        // Changing /MTR while the drive remains selected updates the drive.
        drive.update_control(false, false, false, true, false);
        assert!(!drive.motor_on());
    }

    #[test]
    fn spun_up_drive_keeps_status_but_hides_read_data_after_deselect() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);
        for _ in 0..MOTOR_SPINUP_TICKS {
            drive.tick();
        }
        drive.update_control(false, false, false, false, true);

        let status = drive.status();
        assert!(!status.disk_change);
        assert!(status.track0);
        assert!(status.ready);
        assert!(!drive.read_data_available());
        assert!(drive.encode_mfm_track().is_some());
    }

    #[test]
    fn disk_change_cleared_by_step() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.update_control(false, true, false, true, true);
        // CHNG active after insert — cleared by head step, matching real hardware
        assert!(drive.status().disk_change);

        drive.eject_disk();
        drive.update_control(false, true, false, true, true);
        assert!(drive.status().disk_change);

        // Insert new disk — CHNG still active until step
        let adf2 =
            Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf2);
        assert!(drive.status().disk_change);

        // Step clears change flag
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        assert!(!drive.status().disk_change);
    }

    #[test]
    fn encode_track_returns_data_with_disk() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        drive.acknowledge_disk_change();
        drive.update_control(false, false, false, true, true);
        for _ in 0..MOTOR_SPINUP_TICKS {
            drive.tick();
        }

        let mfm = drive.encode_mfm_track();
        assert!(mfm.is_some());
        assert_eq!(mfm.expect("some").len(), mfm::MFM_TRACK_BYTES);
    }

    #[test]
    fn encode_track_returns_none_without_disk() {
        let drive = AmigaFloppyDrive::new();
        assert!(drive.encode_mfm_track().is_none());
    }

    #[test]
    fn head_select() {
        let mut drive = AmigaFloppyDrive::new();
        // side_upper = true means upper head (head 1)
        drive.update_control(false, false, true, true, true);
        assert_eq!(drive.head(), 1);
        // side_upper = false means lower head (head 0)
        drive.update_control(false, false, false, true, true);
        assert_eq!(drive.head(), 0);
    }

    #[test]
    fn flush_write_capture_persists_to_adf() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_commodore_amiga_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);

        // Prepare sector data with a known pattern
        let mut sector_data = vec![0u8; 11 * 512];
        for (i, byte) in sector_data[..512].iter_mut().enumerate() {
            *byte = (i & 0xFF) as u8;
        }

        // Encode track 0 (cyl 0, head 0) and feed as MFM words
        let mfm_bytes = mfm::encode_mfm_track(&sector_data, 0, 11);
        let mfm_words: Vec<u16> = mfm_bytes
            .chunks_exact(2)
            .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
            .collect();
        for &word in &mfm_words {
            drive.note_write_mfm_word(word);
        }

        let written = drive.flush_write_capture();
        assert_eq!(written, 11, "should write all 11 sectors");

        // Verify sector 0 was persisted
        let saved = drive.save_adf().expect("disk present");
        let expected: Vec<u8> = (0..512).map(|i| (i & 0xFF) as u8).collect();
        assert_eq!(&saved[..512], &expected[..]);
    }

    #[test]
    fn flush_write_no_disk_returns_zero() {
        let mut drive = AmigaFloppyDrive::new();
        drive.note_write_mfm_word(0x4489);
        drive.note_write_mfm_word(0x4489);
        assert_eq!(drive.flush_write_capture(), 0);
    }

    #[test]
    fn save_adf_returns_none_without_disk() {
        let drive = AmigaFloppyDrive::new();
        assert!(drive.save_adf().is_none());
    }

    #[test]
    fn write_mfm_capture_records_and_clears_words() {
        let mut drive = AmigaFloppyDrive::new();
        drive.note_write_mfm_word(0x1234);
        drive.note_write_mfm_word(0xABCD);
        assert_eq!(drive.write_mfm_capture(), &[0x1234, 0xABCD]);
        drive.clear_write_mfm_capture();
        assert!(drive.write_mfm_capture().is_empty());
    }
}
