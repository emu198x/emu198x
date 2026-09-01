//! The Atari SIO bus and the disk drives on it.
//!
//! SIO is two independent serial lines — DATA OUT from the computer, DATA IN
//! back — plus a command line the computer drops to say a command frame is
//! coming. Every device hears every byte; the device ID in the frame picks one
//! to answer.
//!
//! This crate models the bus at byte granularity. The bit timing is POKEY's,
//! and the machine hands whole bytes across: [`SioBus::send`] for what the
//! computer put on DATA OUT, [`SioBus::poll_response`] for what a device has
//! ready to put on DATA IN.
//!
//! # The command protocol
//!
//! From the *Altirra Hardware Reference Manual* §9.1:
//!
//! 1. The computer drops the command line, sends five bytes — device ID,
//!    command, two auxiliary bytes, checksum — and raises it again.
//! 2. The addressed device answers `A` if the frame is good, `N` if not.
//! 3. For a write, the computer now sends the data frame, which the device
//!    answers `A` or `N` in turn.
//! 4. The device does the work.
//! 5. It reports `C` for complete or `E` for error, after at least 250µs of
//!    silence.
//! 6. For a read, the data frame follows *after* that report — not before it.
//!
//! Step 6 is the part worth stating plainly, because a data frame appears on
//! both sides of the result byte depending on which way the data is going.
//!
//! # Checksums
//!
//! Every frame ends in an eight-bit sum with the carry folded back in, which
//! is why [`checksum`] adds into a `u16` and folds rather than wrapping.

use format_atari_8bit_atr::AtrImage;
use serde::{Deserialize, Serialize};

/// The bytes the protocol spells out.
const ACK: u8 = 0x41; // 'A'
const NAK: u8 = 0x4E; // 'N'
const COMPLETE: u8 = 0x43; // 'C'
const ERROR: u8 = 0x45; // 'E'

/// A command frame is five bytes: device, command, two auxiliary, checksum.
const COMMAND_FRAME_LEN: usize = 5;

/// Disk drives answer to `$31` upwards, one per drive.
const DISK_DEVICE_BASE: u8 = 0x31;

/// Commands a stock 810 understands.
const CMD_STATUS: u8 = 0x53; // 'S'
const CMD_READ: u8 = 0x52; // 'R'
const CMD_PUT: u8 = 0x50; // 'P'
const CMD_WRITE: u8 = 0x57; // 'W'

/// How long the drive takes to notice the command line has risen and answer.
/// The protocol allows up to 16ms; a real 810 is far quicker, and so is this.
const ACK_DELAY_CYCLES: u32 = 1_000;

/// Silence between the acknowledgement and the result byte. The protocol asks
/// for at least 250µs; this is about twice that at 1.79MHz.
const RESULT_DELAY_CYCLES: u32 = 1_000;

/// How long a read takes once the drive has acknowledged it. A real drive
/// spends up to a disk revolution here; nothing depends on that, and a long
/// wait only slows a boot down.
const SEEK_DELAY_CYCLES: u32 = 2_000;

/// The format timeout a drive reports, in units of 64 frames. `$E0` is about
/// four minutes. Reporting less is known to break DOS when a real drive is
/// also on the bus, so drives are expected to report at least this.
const FORMAT_TIMEOUT: u8 = 0xE0;

/// The eight-bit checksum every SIO frame ends with: a sum with the carry
/// folded back into the low byte rather than discarded.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    let mut sum = 0u16;
    for &b in bytes {
        sum += u16::from(b);
        if sum > 0xFF {
            sum = (sum & 0xFF) + 1;
        }
    }
    sum as u8
}

/// What a drive is in the middle of doing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
enum State {
    /// Nothing in progress.
    #[default]
    Idle,
    /// Collecting a command frame while the command line is low.
    ReceivingCommand,
    /// Collecting a data frame the computer is sending for a write.
    ReceivingData {
        sector: u16,
        expected: usize,
        buffer: Vec<u8>,
        verify: bool,
    },
}

/// One disk drive on the bus.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskDrive {
    device_id: u8,
    disk: Option<AtrImage>,
    state: State,
    frame: Vec<u8>,
    /// Bytes waiting to go out on DATA IN, and the cycles left before the
    /// first of them may.
    outbox: Vec<u8>,
    delay: u32,
    /// Set when the last command frame or data frame arrived corrupt, which
    /// the Status command reports.
    last_command_bad: bool,
    last_data_bad: bool,
    write_failed: bool,
}

impl DiskDrive {
    /// A drive answering as D`n`, numbered from 1, with no disk in it.
    #[must_use]
    pub fn new(drive: u8) -> Self {
        Self {
            device_id: DISK_DEVICE_BASE + drive.saturating_sub(1),
            disk: None,
            state: State::Idle,
            frame: Vec::with_capacity(COMMAND_FRAME_LEN),
            outbox: Vec::new(),
            delay: 0,
            last_command_bad: false,
            last_data_bad: false,
            write_failed: false,
        }
    }

    /// Put a disk in, or take one out with `None`.
    pub fn insert(&mut self, disk: Option<AtrImage>) {
        self.disk = disk;
        self.state = State::Idle;
        self.frame.clear();
        self.outbox.clear();
        self.delay = 0;
    }

    /// Take the disk out, with whatever was written to it while it was in.
    pub fn eject(&mut self) -> Option<AtrImage> {
        let disk = self.disk.take();
        self.insert(None);
        disk
    }

    /// Whether a disk is in the drive.
    #[must_use]
    pub fn has_disk(&self) -> bool {
        self.disk.is_some()
    }

    /// The disk currently in the drive, for writing it back out.
    #[must_use]
    pub fn disk(&self) -> Option<&AtrImage> {
        self.disk.as_ref()
    }

    /// The command line changed. Falling starts a frame; rising says the
    /// computer has finished sending it and is listening for the answer.
    fn set_command_line(&mut self, asserted: bool) {
        if asserted {
            self.state = State::ReceivingCommand;
            self.frame.clear();
            self.outbox.clear();
        } else if matches!(self.state, State::ReceivingCommand) {
            self.state = State::Idle;
            self.handle_command_frame();
        }
    }

    /// A byte arrived on DATA OUT.
    fn receive(&mut self, byte: u8) {
        match &mut self.state {
            State::ReceivingCommand => {
                // A drive's firmware reads five bytes and then waits for the
                // command line, so anything further while the line is still
                // down is not part of the frame. Some OS send loops push a
                // byte or two past the end of one.
                if self.frame.len() < COMMAND_FRAME_LEN {
                    self.frame.push(byte);
                }
            }
            State::ReceivingData {
                buffer, expected, ..
            } => {
                buffer.push(byte);
                if buffer.len() == *expected {
                    self.finish_data_frame();
                }
            }
            State::Idle => {}
        }
    }

    /// Decide what to do with a completed command frame.
    fn handle_command_frame(&mut self) {
        if self.frame.len() != COMMAND_FRAME_LEN {
            return; // Not a frame this drive can make sense of; stay quiet.
        }
        let device = self.frame[0];
        if device != self.device_id {
            return; // Addressed to somebody else.
        }
        if checksum(&self.frame[..4]) != self.frame[4] {
            // A corrupt frame is ignored rather than NAKed: the drive cannot
            // trust the device ID either, so answering could talk over another
            // drive. The Status command reports it afterwards.
            self.last_command_bad = true;
            return;
        }

        let command = self.frame[1];
        let sector = u16::from(self.frame[2]) | (u16::from(self.frame[3]) << 8);

        match command {
            CMD_STATUS => self.answer_status(),
            CMD_READ => self.answer_read(sector),
            CMD_PUT | CMD_WRITE => self.begin_write(sector, command == CMD_WRITE),
            // Format and the extended command sets are not modelled. A NAK
            // fails the command cleanly rather than leaving the OS to time out.
            _ => self.reply(&[NAK]),
        }
    }

    fn answer_status(&mut self) {
        let Some(disk) = &self.disk else {
            // No disk: the drive is there and answers, but the controller
            // reports itself not ready.
            let frame = [0x00, 0x7F, FORMAT_TIMEOUT, 0x00];
            let mut bytes = vec![ACK];
            bytes.extend(frame);
            bytes.push(checksum(&frame));
            self.reply_with_result(bytes, COMPLETE);
            return;
        };

        let mut drive_status = 0x10; // motor running
        if disk.sector_size() == 256 {
            drive_status |= 0x20; // double density
        }
        if disk.sector_count() > 720 {
            drive_status |= 0x80; // enhanced density
        }
        if disk.write_protected() {
            drive_status |= 0x08;
        }
        if self.write_failed {
            drive_status |= 0x04;
        }
        if self.last_data_bad {
            drive_status |= 0x02;
        }
        if self.last_command_bad {
            drive_status |= 0x01;
        }
        // The flags latch until they are reported. Clearing them when a good
        // frame arrives would make them unobservable, since asking about them
        // takes a frame of its own.
        self.last_command_bad = false;
        self.last_data_bad = false;
        self.write_failed = false;

        // The second byte is *inverted* FDC status, so all-ones is a healthy
        // controller with nothing to report.
        let frame = [drive_status, 0xFF, FORMAT_TIMEOUT, 0x00];
        let mut bytes = vec![ACK];
        bytes.extend(frame);
        bytes.push(checksum(&frame));
        self.reply_with_result(bytes, COMPLETE);
    }

    fn answer_read(&mut self, sector: u16) {
        let Some(disk) = &self.disk else {
            self.reply(&[NAK]);
            return;
        };
        let Some(data) = disk.sector_as_read(sector) else {
            // A sector number the disk does not have is a fault the drive can
            // see in the command itself, so it NAKs rather than accepting and
            // reporting an error later.
            self.reply(&[NAK]);
            return;
        };
        let data = data.to_vec();
        let sum = checksum(&data);
        let mut bytes = vec![ACK];
        bytes.extend(data);
        bytes.push(sum);
        self.reply_with_result(bytes, COMPLETE);
    }

    fn begin_write(&mut self, sector: u16, verify: bool) {
        let Some(disk) = &self.disk else {
            self.reply(&[NAK]);
            return;
        };
        let Some(expected) = disk.sector_as_read(sector).map(<[u8]>::len) else {
            self.reply(&[NAK]);
            return;
        };
        self.reply(&[ACK]);
        self.state = State::ReceivingData {
            sector,
            expected: expected + 1, // the data frame carries its checksum
            buffer: Vec::with_capacity(expected + 1),
            verify,
        };
    }

    fn finish_data_frame(&mut self) {
        let State::ReceivingData {
            sector,
            buffer,
            verify,
            ..
        } = std::mem::take(&mut self.state)
        else {
            return;
        };
        let _ = verify; // Verification costs a disk revolution, nothing else.

        let (data, sum) = buffer.split_at(buffer.len() - 1);
        if checksum(data) != sum[0] {
            self.last_data_bad = true;
            self.reply(&[NAK]);
            return;
        }

        let write_protected = self.disk.as_ref().is_some_and(AtrImage::write_protected);
        if write_protected {
            self.write_failed = true;
            self.reply_with_result(vec![ACK], ERROR);
            return;
        }

        let written = self
            .disk
            .as_mut()
            .map(|disk| disk.write_sector(sector, data));
        self.write_failed = !matches!(written, Some(Ok(())));
        let result = if self.write_failed { ERROR } else { COMPLETE };
        self.reply_with_result(vec![ACK], result);
    }

    /// Answer with these bytes after the usual acknowledgement delay.
    fn reply(&mut self, bytes: &[u8]) {
        self.outbox = bytes.to_vec();
        self.delay = ACK_DELAY_CYCLES;
    }

    /// Answer `bytes[0]` as the acknowledgement, then the result byte and
    /// whatever follows it. The gap between them is the protocol's dead time.
    fn reply_with_result(&mut self, mut bytes: Vec<u8>, result: u8) {
        let ack = bytes.remove(0);
        self.outbox = vec![ack, result];
        self.outbox.extend(bytes);
        self.delay = ACK_DELAY_CYCLES;
    }

    /// Advance the drive's own clock by one CPU cycle.
    fn tick(&mut self) {
        self.delay = self.delay.saturating_sub(1);
    }

    /// The next byte for DATA IN, if the drive is ready to send one.
    fn poll_response(&mut self) -> Option<u8> {
        if self.delay > 0 || self.outbox.is_empty() {
            return None;
        }
        let byte = self.outbox.remove(0);
        // The gap the protocol asks for sits between the acknowledgement and
        // the result byte; the rest of a data frame goes out back to back.
        self.delay = if byte == ACK && !self.outbox.is_empty() {
            RESULT_DELAY_CYCLES
        } else if matches!(byte, COMPLETE | ERROR) && !self.outbox.is_empty() {
            SEEK_DELAY_CYCLES
        } else {
            0
        };
        Some(byte)
    }
}

/// The SIO bus and whatever is plugged into it.
///
/// A bus with nothing attached answers nothing, which is what an Atari with no
/// peripherals sees: the OS sends its command frames, hears silence, times out
/// and carries on to whatever else it can boot. Attaching an *empty* drive is
/// a different thing again — the drive answers, and says it has no disk.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SioBus {
    drives: Vec<Option<DiskDrive>>,
    command_asserted: bool,
}

impl Default for SioBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SioBus {
    /// A bus with nothing on it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            drives: vec![None; 4],
            command_asserted: false,
        }
    }

    /// Plug in drive `n`, numbered from 1, with no disk in it. Attaching a
    /// drive changes what the OS sees at boot even when it is empty, so this
    /// is deliberate rather than the default.
    pub fn attach_drive(&mut self, drive: u8) {
        if let Some(slot) = self.drives.get_mut(usize::from(drive.saturating_sub(1))) {
            slot.get_or_insert_with(|| DiskDrive::new(drive));
        }
    }

    /// Unplug drive `n`, disk and all.
    pub fn detach_drive(&mut self, drive: u8) {
        if let Some(slot) = self.drives.get_mut(usize::from(drive.saturating_sub(1))) {
            *slot = None;
        }
    }

    /// Put a disk in drive `n`, plugging the drive in if it is not there.
    pub fn insert_disk(&mut self, drive: u8, disk: AtrImage) {
        self.attach_drive(drive);
        if let Some(Some(d)) = self.drives.get_mut(usize::from(drive.saturating_sub(1))) {
            d.insert(Some(disk));
        }
    }

    /// Take the disk out of drive `n`, leaving the drive on the bus. `None`
    /// when the drive is not there or is empty.
    pub fn eject_disk(&mut self, drive: u8) -> Option<AtrImage> {
        self.drive_mut(drive)?.eject()
    }

    /// One drive, numbered from 1, if it is plugged in.
    #[must_use]
    pub fn drive(&self, drive: u8) -> Option<&DiskDrive> {
        self.drives
            .get(usize::from(drive.saturating_sub(1)))?
            .as_ref()
    }

    /// One drive, numbered from 1, mutably.
    pub fn drive_mut(&mut self, drive: u8) -> Option<&mut DiskDrive> {
        self.drives
            .get_mut(usize::from(drive.saturating_sub(1)))?
            .as_mut()
    }

    /// The command line, as the PIA drives it. It is active low on the bus;
    /// `asserted` here means the line is *down* and a frame is coming.
    pub fn set_command_line(&mut self, asserted: bool) {
        if asserted == self.command_asserted {
            return;
        }
        self.command_asserted = asserted;
        for drive in self.drives.iter_mut().flatten() {
            drive.set_command_line(asserted);
        }
    }

    /// A byte the computer put on DATA OUT. Every device hears it.
    pub fn send(&mut self, byte: u8) {
        for drive in self.drives.iter_mut().flatten() {
            drive.receive(byte);
        }
    }

    /// Advance every device by one CPU cycle.
    pub fn tick(&mut self) {
        for drive in self.drives.iter_mut().flatten() {
            drive.tick();
        }
    }

    /// The next byte for DATA IN. Only one device can send at a time, so the
    /// first with something ready wins — which on a real bus is enforced by
    /// only one device having been addressed.
    pub fn poll_response(&mut self) -> Option<u8> {
        self.drives
            .iter_mut()
            .flatten()
            .find_map(DiskDrive::poll_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(sectors: u16) -> AtrImage {
        let data_len = usize::from(sectors) * 128;
        let mut image = vec![0u8; 16];
        image[0..2].copy_from_slice(&0x0296u16.to_le_bytes());
        image[2..4].copy_from_slice(&((data_len / 16) as u16).to_le_bytes());
        image[4..6].copy_from_slice(&128u16.to_le_bytes());
        for sector in 0..sectors {
            image.extend(std::iter::repeat_n((sector % 251) as u8, 128));
        }
        AtrImage::parse(&image).expect("test disk parses")
    }

    fn bus_with_disk() -> SioBus {
        let mut bus = SioBus::new();
        bus.insert_disk(1, disk(720));
        bus
    }

    /// Drive a whole command through the bus and collect what comes back.
    fn issue(bus: &mut SioBus, frame: [u8; 4], data: &[u8], reads: usize) -> Vec<u8> {
        bus.set_command_line(true);
        for &b in &frame {
            bus.send(b);
        }
        bus.send(checksum(&frame));
        bus.set_command_line(false);

        let mut out = Vec::new();
        let mut sent_data = false;
        for _ in 0..1_000_000 {
            bus.tick();
            if let Some(b) = bus.poll_response() {
                out.push(b);
                if out.len() >= reads {
                    break;
                }
                // A write's data frame follows the acknowledgement.
                if !data.is_empty() && !sent_data && b == ACK {
                    sent_data = true;
                    for &d in data {
                        bus.send(d);
                    }
                    bus.send(checksum(data));
                }
            }
        }
        out
    }

    fn command(device: u8, cmd: u8, sector: u16) -> [u8; 4] {
        [device, cmd, sector as u8, (sector >> 8) as u8]
    }

    /// The carry folds back into the low byte rather than being discarded.
    /// A bus with nothing plugged in answers nothing, so an Atari with no
    /// peripherals hears the silence it should and moves on.
    #[test]
    fn an_empty_bus_answers_nothing() {
        let mut bus = SioBus::new();
        let out = issue(&mut bus, command(0x31, CMD_STATUS, 0), &[], 1);
        assert!(out.is_empty(), "no drive is plugged in");
    }

    #[test]
    fn the_checksum_wraps_the_carry_round() {
        assert_eq!(checksum(&[]), 0);
        assert_eq!(checksum(&[0x01, 0x02]), 0x03);
        assert_eq!(checksum(&[0xFF, 0xFF]), 0xFF, "0x1FE folds to 0xFF");
        assert_eq!(checksum(&[0x80, 0x80, 0x01]), 0x02);
    }

    #[test]
    fn a_status_command_describes_the_disk() {
        let mut bus = bus_with_disk();
        let out = issue(&mut bus, command(0x31, CMD_STATUS, 0), &[], 7);

        assert_eq!(out[0], ACK);
        assert_eq!(out[1], COMPLETE);
        let frame = &out[2..6];
        assert_eq!(frame[0] & 0x10, 0x10, "motor running");
        assert_eq!(frame[0] & 0x20, 0, "single density");
        assert_eq!(frame[0] & 0x08, 0, "not write protected");
        assert_eq!(frame[1], 0xFF, "inverted FDC status, nothing wrong");
        assert_eq!(frame[2], FORMAT_TIMEOUT);
        assert_eq!(out[6], checksum(frame));
    }

    /// The data frame for a read comes *after* the completion byte, not
    /// before it.
    #[test]
    fn a_read_returns_the_sector_after_the_completion_byte() {
        let mut bus = bus_with_disk();
        let out = issue(&mut bus, command(0x31, CMD_READ, 1), &[], 3 + 128);

        assert_eq!(out[0], ACK);
        assert_eq!(out[1], COMPLETE);
        let data = &out[2..130];
        assert_eq!(data, &[0u8; 128], "sector 1 of the test disk");
        assert_eq!(out[130], checksum(data));
    }

    #[test]
    fn a_sector_the_disk_does_not_have_is_refused_outright() {
        let mut bus = bus_with_disk();
        for sector in [0u16, 721, 9999] {
            let out = issue(&mut bus, command(0x31, CMD_READ, sector), &[], 1);
            assert_eq!(out, vec![NAK], "sector {sector}");
        }
    }

    /// Every device hears every byte, so a drive has to keep quiet when the
    /// frame names somebody else.
    #[test]
    fn a_command_for_another_device_goes_unanswered() {
        let mut bus = bus_with_disk();
        let out = issue(&mut bus, command(0x40, CMD_STATUS, 0), &[], 1);
        assert!(out.is_empty(), "$40 is a printer, and there is none here");
    }

    /// A corrupt command frame is ignored rather than answered — the drive
    /// cannot trust the device ID either, so replying could talk over another
    /// drive. The Status command reports it afterwards.
    #[test]
    fn a_bad_checksum_is_ignored_and_then_reported() {
        let mut bus = bus_with_disk();
        bus.set_command_line(true);
        for b in [0x31, CMD_STATUS, 0x00, 0x00, 0x00] {
            bus.send(b);
        }
        bus.set_command_line(false);
        for _ in 0..10_000 {
            bus.tick();
            assert_eq!(bus.poll_response(), None, "the drive stays quiet");
        }

        let out = issue(&mut bus, command(0x31, CMD_STATUS, 0), &[], 7);
        assert_eq!(
            out[2] & 0x01,
            0x01,
            "the status frame reports the bad command frame"
        );
    }

    /// A write is acknowledged twice: once for the command frame and once for
    /// the data frame, and only then is the result reported.
    #[test]
    fn a_written_sector_reads_back() {
        let mut bus = bus_with_disk();
        let written = [0x5Au8; 128];
        let out = issue(&mut bus, command(0x31, CMD_PUT, 100), &written, 3);
        assert_eq!(out, vec![ACK, ACK, COMPLETE]);

        let read = issue(&mut bus, command(0x31, CMD_READ, 100), &[], 3 + 128);
        assert_eq!(&read[2..130], &written);
    }

    /// The disk that comes out carries what was written to it, and the drive
    /// it came out of is still on the bus and answers as an empty one.
    #[test]
    fn an_ejected_disk_keeps_its_writes_and_leaves_the_drive_attached() {
        let mut bus = bus_with_disk();
        let written = [0x5Au8; 128];
        issue(&mut bus, command(0x31, CMD_PUT, 100), &written, 3);

        let disk = bus.eject_disk(1).expect("a disk was in D1");
        assert_eq!(disk.sector(100), Some(&written[..]));
        assert!(bus.eject_disk(1).is_none());
        assert!(bus.drive(1).is_some_and(|d| !d.has_disk()));
        let out = issue(&mut bus, command(0x31, CMD_READ, 1), &[], 1);
        assert_eq!(out, vec![NAK]);
    }

    #[test]
    fn a_write_with_a_bad_checksum_is_refused() {
        let mut bus = bus_with_disk();
        bus.set_command_line(true);
        let frame = command(0x31, CMD_PUT, 100);
        for &b in &frame {
            bus.send(b);
        }
        bus.send(checksum(&frame));
        bus.set_command_line(false);

        let mut out = Vec::new();
        for _ in 0..100_000 {
            bus.tick();
            if let Some(b) = bus.poll_response() {
                out.push(b);
                if out.len() == 1 {
                    for _ in 0..128 {
                        bus.send(0x11);
                    }
                    bus.send(0x00); // wrong checksum
                }
                if out.len() == 2 {
                    break;
                }
            }
        }
        assert_eq!(out, vec![ACK, NAK]);
    }

    #[test]
    fn an_empty_drive_answers_status_but_refuses_a_read() {
        let mut bus = SioBus::new();
        bus.attach_drive(1);
        let status = issue(&mut bus, command(0x31, CMD_STATUS, 0), &[], 7);
        assert_eq!(status[0], ACK);
        assert_eq!(status[2] & 0x10, 0, "no motor without a disk");
        assert_eq!(status[3], 0x7F, "the controller reports itself not ready");

        let read = issue(&mut bus, command(0x31, CMD_READ, 1), &[], 1);
        assert_eq!(read, vec![NAK]);
    }

    /// The result byte follows the acknowledgement after a gap. The protocol
    /// asks for at least 250µs of dead time, which is about 450 cycles.
    #[test]
    fn the_result_byte_waits_after_the_acknowledgement() {
        let mut bus = bus_with_disk();
        let frame = command(0x31, CMD_STATUS, 0);
        bus.set_command_line(true);
        for &b in &frame {
            bus.send(b);
        }
        bus.send(checksum(&frame));
        bus.set_command_line(false);

        let mut ack_at = None;
        let mut result_at = None;
        for cycle in 0..100_000u32 {
            bus.tick();
            match bus.poll_response() {
                Some(ACK) => ack_at = Some(cycle),
                Some(COMPLETE) => {
                    result_at = Some(cycle);
                    break;
                }
                _ => {}
            }
        }
        let gap = result_at.expect("complete") - ack_at.expect("ack");
        assert!(gap >= 450, "only {gap} cycles between ACK and Complete");
    }

    #[test]
    fn a_command_that_is_not_modelled_is_refused_rather_than_ignored() {
        let mut bus = bus_with_disk();
        let out = issue(&mut bus, command(0x31, 0x21, 0), &[], 1);
        assert_eq!(out, vec![NAK], "format is not modelled");
    }
}
