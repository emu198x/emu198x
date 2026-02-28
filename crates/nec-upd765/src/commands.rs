//! NEC uPD765 command parsing and execution.
//!
//! Each command has a fixed number of parameter bytes. After all bytes are
//! received, the command executes immediately (no cycle-level timing — the
//! Spectrum +3 polls MSR, so instant execution is sufficient).

#![allow(clippy::cast_possible_truncation)]

use crate::dsk::DskImage;
use crate::FdcPhase;

/// Command IDs (low 5 bits of the first command byte).
const CMD_SPECIFY: u8 = 0x03;
const CMD_SENSE_DRIVE: u8 = 0x04;
const CMD_WRITE_DATA: u8 = 0x05;
const CMD_READ_DATA: u8 = 0x06;
const CMD_RECALIBRATE: u8 = 0x07;
const CMD_SENSE_INTERRUPT: u8 = 0x08;
const CMD_READ_ID: u8 = 0x0A;
const CMD_FORMAT_TRACK: u8 = 0x0D;
const CMD_SEEK: u8 = 0x0F;

/// How many parameter bytes each command expects (total including the command byte).
pub fn command_length(cmd_byte: u8) -> usize {
    match cmd_byte & 0x1F {
        CMD_SPECIFY => 3,
        CMD_SENSE_DRIVE => 2,
        CMD_WRITE_DATA => 9,
        CMD_READ_DATA => 9,
        CMD_RECALIBRATE => 2,
        CMD_SENSE_INTERRUPT => 1,
        CMD_READ_ID => 2,
        CMD_FORMAT_TRACK => 6,
        CMD_SEEK => 3,
        _ => 1, // Invalid command — consume just the one byte
    }
}

/// Execute a fully-received command. Returns (result_bytes, interrupt_pending).
pub fn execute(
    cmd_buf: &[u8],
    st0: &mut u8,
    st1: &mut u8,
    st2: &mut u8,
    pcn: &mut [u8; 4],
    disks: &mut [Option<DskImage>; 2],
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let cmd_id = cmd_buf[0] & 0x1F;

    match cmd_id {
        CMD_SPECIFY => exec_specify(cmd_buf, phase),
        CMD_SENSE_DRIVE => exec_sense_drive(cmd_buf, pcn, disks, st0, phase),
        CMD_RECALIBRATE => exec_recalibrate(cmd_buf, pcn, st0, phase),
        CMD_SENSE_INTERRUPT => exec_sense_interrupt(st0, pcn, cmd_buf, phase),
        CMD_SEEK => exec_seek(cmd_buf, pcn, st0, phase),
        CMD_READ_DATA => exec_read_data(cmd_buf, disks, pcn, st0, st1, st2, phase),
        CMD_WRITE_DATA => exec_write_data(cmd_buf, disks, pcn, st0, st1, st2, phase),
        CMD_READ_ID => exec_read_id(cmd_buf, disks, pcn, st0, st1, st2, phase),
        CMD_FORMAT_TRACK => exec_format_track(cmd_buf, disks, pcn, st0, st1, st2, phase),
        _ => exec_invalid(st0, phase),
    }
}

// ---------------------------------------------------------------------------
// SPECIFY (0x03) — set timing parameters, no result phase
// ---------------------------------------------------------------------------

fn exec_specify(
    _cmd_buf: &[u8],
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    // SRT, HUT, HLT, NDMA stored but not used in this emulation level.
    // The +3 sends SPECIFY early in boot — we just accept it silently.
    *phase = FdcPhase::Idle;
    (Vec::new(), false)
}

// ---------------------------------------------------------------------------
// SENSE DRIVE STATUS (0x04) — read ST3
// ---------------------------------------------------------------------------

fn exec_sense_drive(
    cmd_buf: &[u8],
    pcn: &[u8; 4],
    disks: &[Option<DskImage>; 2],
    st0: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let head = (cmd_buf[1] >> 2) & 0x01;

    // ST3: bits 0-1 = drive, bit 2 = head, bit 3 = two-side, bit 4 = track 0,
    //       bit 5 = ready, bit 6 = write protect (always 0)
    let mut st3 = (drive as u8) | (head << 2);
    if disks.get(drive).is_some_and(|d| d.is_some()) {
        st3 |= 0x20; // Ready
        st3 |= 0x08; // Two-side (assume double-sided)
    }
    if pcn[drive] == 0 {
        st3 |= 0x10; // Track 0
    }

    *st0 = drive as u8 | (head << 2);
    *phase = FdcPhase::Result;
    (vec![st3], false)
}

// ---------------------------------------------------------------------------
// RECALIBRATE (0x07) — seek to track 0
// ---------------------------------------------------------------------------

fn exec_recalibrate(
    cmd_buf: &[u8],
    pcn: &mut [u8; 4],
    st0: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    pcn[drive] = 0;
    // ST0: seek end (bit 5), drive number
    *st0 = 0x20 | drive as u8;
    *phase = FdcPhase::Idle;
    (Vec::new(), true) // Interrupt pending
}

// ---------------------------------------------------------------------------
// SENSE INTERRUPT STATUS (0x08) — read ST0 + PCN after seek/recalibrate
// ---------------------------------------------------------------------------

fn exec_sense_interrupt(
    st0: &mut u8,
    pcn: &[u8; 4],
    _cmd_buf: &[u8],
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (*st0 & 0x03) as usize;
    let result = vec![*st0, pcn[drive]];
    *phase = FdcPhase::Result;
    (result, false)
}

// ---------------------------------------------------------------------------
// SEEK (0x0F) — move head to specified cylinder
// ---------------------------------------------------------------------------

fn exec_seek(
    cmd_buf: &[u8],
    pcn: &mut [u8; 4],
    st0: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let ncn = cmd_buf[2];
    pcn[drive] = ncn;
    *st0 = 0x20 | drive as u8;
    *phase = FdcPhase::Idle;
    (Vec::new(), true) // Interrupt pending
}

// ---------------------------------------------------------------------------
// READ DATA (0x06) — read sector(s) from disk
// ---------------------------------------------------------------------------

fn exec_read_data(
    cmd_buf: &[u8],
    disks: &[Option<DskImage>; 2],
    pcn: &[u8; 4],
    st0: &mut u8,
    st1: &mut u8,
    st2: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let head = (cmd_buf[1] >> 2) & 0x01;
    let _c = cmd_buf[2];
    let _h = cmd_buf[3];
    let r = cmd_buf[4];
    let n = cmd_buf[5];
    let eot = cmd_buf[6];
    // cmd_buf[7] = GPL, cmd_buf[8] = DTL

    *st0 = drive as u8 | ((head as u8) << 2);
    *st1 = 0;
    *st2 = 0;

    let track = pcn[drive];

    let Some(Some(disk)) = disks.get(drive) else {
        // No disk — set error
        *st0 |= 0x40; // Abnormal termination
        *st1 |= 0x01; // Missing address mark
        *phase = FdcPhase::Result;
        return (make_read_write_result(*st0, *st1, *st2, track, head, r, n), true);
    };

    // Collect sector data for the result data transfer
    let mut data = Vec::new();
    let mut current_r = r;
    loop {
        if let Some(sec_data) = disk.read_sector(track, head, current_r) {
            data.extend_from_slice(sec_data);
        } else {
            *st0 |= 0x40; // Abnormal termination
            *st1 |= 0x04; // No data
            break;
        }
        if current_r >= eot {
            break;
        }
        current_r += 1;
    }

    // Result phase: 7 bytes
    let mut result = make_read_write_result(*st0, *st1, *st2, track, head, current_r, n);
    // Prepend sector data for the execution/transfer phase
    let mut full = data;
    full.extend_from_slice(&result);
    result = full;

    *phase = FdcPhase::Result;
    (result, true)
}

fn make_read_write_result(st0: u8, st1: u8, st2: u8, c: u8, h: u8, r: u8, n: u8) -> Vec<u8> {
    vec![st0, st1, st2, c, h, r, n]
}

// ---------------------------------------------------------------------------
// WRITE DATA (0x05) — write sector(s) to disk
// ---------------------------------------------------------------------------

fn exec_write_data(
    cmd_buf: &[u8],
    disks: &mut [Option<DskImage>; 2],
    pcn: &[u8; 4],
    st0: &mut u8,
    st1: &mut u8,
    st2: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let head = (cmd_buf[1] >> 2) & 0x01;
    let _c = cmd_buf[2];
    let _h = cmd_buf[3];
    let r = cmd_buf[4];
    let n = cmd_buf[5];
    // eot, gpl, dtl in remaining bytes

    *st0 = drive as u8 | ((head as u8) << 2);
    *st1 = 0;
    *st2 = 0;

    let track = pcn[drive];

    let Some(Some(_disk)) = disks.get_mut(drive) else {
        *st0 |= 0x40;
        *st1 |= 0x01;
        *phase = FdcPhase::Result;
        return (make_read_write_result(*st0, *st1, *st2, track, head, r, n), true);
    };

    // For now, write data is accepted but the actual data transfer from CPU
    // happens byte-by-byte via the data register in the main FDC loop.
    // This stub sets up the result for after the transfer completes.
    // The sector size for write is 128 << n bytes.

    let _sector_size = 128usize << (n as u32);
    // Write will be handled by the data register write path.
    // For now, just signal success.

    *phase = FdcPhase::Result;
    (make_read_write_result(*st0, *st1, *st2, track, head, r, n), true)
}

// ---------------------------------------------------------------------------
// READ ID (0x0A) — read the next sector header
// ---------------------------------------------------------------------------

fn exec_read_id(
    cmd_buf: &[u8],
    disks: &[Option<DskImage>; 2],
    pcn: &[u8; 4],
    st0: &mut u8,
    st1: &mut u8,
    st2: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let head = (cmd_buf[1] >> 2) & 0x01;

    *st0 = drive as u8 | ((head as u8) << 2);
    *st1 = 0;
    *st2 = 0;

    let track = pcn[drive];

    let Some(Some(disk)) = disks.get(drive) else {
        *st0 |= 0x40;
        *st1 |= 0x01;
        *phase = FdcPhase::Result;
        return (vec![*st0, *st1, *st2, 0, 0, 0, 0], true);
    };

    // Return the first sector ID on this track
    let ids = disk.track_ids(track, head);
    let (c, h, r, n) = ids.first().copied().unwrap_or((track, head, 1, 2));

    *phase = FdcPhase::Result;
    (vec![*st0, *st1, *st2, c, h, r, n], false)
}

// ---------------------------------------------------------------------------
// FORMAT TRACK (0x0D) — format a track with specified parameters
// ---------------------------------------------------------------------------

fn exec_format_track(
    cmd_buf: &[u8],
    _disks: &mut [Option<DskImage>; 2],
    pcn: &[u8; 4],
    st0: &mut u8,
    st1: &mut u8,
    st2: &mut u8,
    phase: &mut FdcPhase,
) -> (Vec<u8>, bool) {
    let drive = (cmd_buf[1] & 0x03) as usize;
    let head = (cmd_buf[1] >> 2) & 0x01;
    let n = cmd_buf[2];
    // cmd_buf[3] = sectors per track, cmd_buf[4] = gap length, cmd_buf[5] = filler

    let track = pcn[drive];

    *st0 = drive as u8 | ((head as u8) << 2);
    *st1 = 0;
    *st2 = 0;

    // FORMAT is accepted but not implemented (no new sectors created in the image).
    // +3DOS FORMAT command sends this, but disk images are pre-formatted.

    *phase = FdcPhase::Result;
    (vec![*st0, *st1, *st2, track, head, 0, n], true)
}

// ---------------------------------------------------------------------------
// Invalid command
// ---------------------------------------------------------------------------

fn exec_invalid(st0: &mut u8, phase: &mut FdcPhase) -> (Vec<u8>, bool) {
    *st0 = 0x80; // Invalid command
    *phase = FdcPhase::Result;
    (vec![*st0], false)
}
