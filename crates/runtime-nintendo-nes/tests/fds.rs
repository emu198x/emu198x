//! Famicom Disk System disks through the runtime, with a synthetic BIOS
//! and disk built here. No Nintendo BIOS or commercial disk is needed.

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HostIo, MachineCore, MachineError, MachineTime, MediaImage,
    MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_nintendo_nes::{
    FDS_BIOS_FIRMWARE_ID, FDS_DISK_SLOT, FDS_MAPPER_NUMBER, Model, NesRuntime,
};

const SIDE_LEN: usize = 65_500;
const FRAME_TICKS: u64 = 341 * 262;

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

/// A two-sided `.fds` file with the fwNES header; each side holds only a
/// disk info block whose filler byte names the side.
fn disk() -> Vec<u8> {
    let mut file = b"FDS\x1A\x02".to_vec();
    file.resize(16, 0);
    for side in 0..2u8 {
        let mut bytes = vec![0x01];
        bytes.extend_from_slice(b"*NINTENDO-HVC*");
        bytes.resize(56, 0xA0 + side);
        bytes.resize(SIDE_LEN, 0);
        file.extend(bytes);
    }
    file
}

/// A BIOS that reads the inserted side into `$0300` through the disk IRQ.
fn bios() -> Vec<u8> {
    let mut bios = vec![0xEA; 8192];
    let program = [
        0xA9, 0x40, 0x8D, 0x17, 0x40, // no APU frame IRQ
        0xA9, 0xC5, 0x8D, 0x25, 0x40, // drive: read, ready, IRQ, motor
        0xA2, 0x00, 0x58, // LDX #0, CLI
        0x4C, 0x0D, 0xE0, // idle
        0xAD, 0x31, 0x40, // IRQ: LDA $4031
        0xE0, 0x10, 0xB0, 0x04, // CPX #16, BCS +4
        0x9D, 0x00, 0x03, 0xE8, // STA $0300,X; INX
        0x40, // RTI
    ];
    bios[..program.len()].copy_from_slice(&program);
    bios[0x1FFA..].copy_from_slice(&[0x1B, 0xE0, 0x00, 0xE0, 0x10, 0xE0]);
    bios
}

fn runtime_with_bios() -> NesRuntime {
    let bios = bios();
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(FDS_BIOS_FIRMWARE_ID, &bios));
    NesRuntime::from_firmware(Model::NesNtsc, &firmware).expect("runtime")
}

fn load_disk(runtime: &mut NesRuntime, file: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new(FDS_DISK_SLOT, MediaKind::Disk, file));
    runtime.load_media(&media)
}

fn run_frames(runtime: &mut NesRuntime, frames: u64) {
    let target = runtime.time().get() + frames * FRAME_TICKS;
    runtime
        .run_until(MachineTime::new(target), &mut null_host())
        .expect("run");
}

fn ram(runtime: &NesRuntime, addr: u16) -> u8 {
    runtime.machine().expect("machine").peek(addr)
}

#[test]
fn a_disk_boots_from_side_a() {
    let mut runtime = runtime_with_bios();
    load_disk(&mut runtime, &disk()).expect("disk loads");
    assert!(runtime.has_disk());
    assert_eq!(runtime.cartridge_mapper(), Some(FDS_MAPPER_NUMBER));
    assert_eq!(runtime.disk_side_count(), 2);
    assert_eq!(runtime.inserted_disk_side(), Some(0));
    run_frames(&mut runtime, 25);
    assert_eq!(ram(&runtime, 0x0300), 0x01);
    assert_eq!(ram(&runtime, 0x030F), 0xA0, "side A's filler");
}

#[test]
fn flipping_the_disk_reads_side_b_after_a_reset() {
    let mut runtime = runtime_with_bios();
    load_disk(&mut runtime, &disk()).expect("disk loads");
    runtime.insert_disk_side(None);
    assert_eq!(runtime.inserted_disk_side(), None);
    runtime.insert_disk_side(Some(1));
    run_frames(&mut runtime, 25);
    assert_eq!(ram(&runtime, 0x030F), 0xA1, "side B's filler");
    // A reset rebuilds the adapter from the disk as it now stands, with
    // side A back in the drive.
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.inserted_disk_side(), Some(0));
    assert_eq!(runtime.disk_image(), Some(disk()));
}

#[test]
fn a_snapshot_keeps_the_disk_and_drive() {
    let mut runtime = runtime_with_bios();
    load_disk(&mut runtime, &disk()).expect("disk loads");
    run_frames(&mut runtime, 20);
    let snapshot = runtime.snapshot().expect("snapshot");
    run_frames(&mut runtime, 5);
    let expected: Vec<u8> = (0..16).map(|i| ram(&runtime, 0x0300 + i)).collect();

    let mut restored = runtime_with_bios();
    restored.restore(&snapshot).expect("restore");
    assert!(restored.has_disk());
    run_frames(&mut restored, 5);
    let actual: Vec<u8> = (0..16).map(|i| ram(&restored, 0x0300 + i)).collect();
    assert_eq!(actual, expected);
}

#[test]
fn a_disk_needs_the_bios() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let error = load_disk(&mut runtime, &disk()).expect_err("no BIOS");
    assert!(
        matches!(error, MachineError::MissingFirmware { ref id } if id == FDS_BIOS_FIRMWARE_ID)
    );
    assert!(runtime.set_fds_bios(vec![0; 100]).is_err(), "8 KiB only");
}

#[test]
fn only_the_famicom_model_has_a_disk_slot() {
    let mut runtime = NesRuntime::blank(Model::NesPal);
    let error = load_disk(&mut runtime, &disk()).expect_err("no disk slot");
    assert!(matches!(error, MachineError::UnknownMediaSlot { .. }));
    let mut runtime = runtime_with_bios();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(FDS_DISK_SLOT, MediaKind::Cartridge, &[]));
    assert!(matches!(
        runtime.load_media(&media),
        Err(MachineError::UnsupportedMediaKind { .. })
    ));
    assert!(matches!(
        load_disk(&mut runtime, &[0; 10]),
        Err(MachineError::InvalidMedia { .. })
    ));
}
