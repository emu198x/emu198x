//! Master System cartridges run through the Power Base Converter: chosen
//! by the image's headers or by its cartridge type, kept across resets,
//! model switches and snapshots.

use emu198x_shell::{
    FirmwareSet, HeadlessSession, MachineCore, MediaImage, MediaKind, MediaSet, ResetKind,
};
use runtime_sega_mega_drive::{MASTER_SYSTEM, MEGA_DRIVE, MegaDriveRuntime, Model, blank};

/// A 32 KB Master System cartridge that sets Mode 4 and counts frames at
/// `$C000` by polling the VDP's status port.
fn sms_cartridge(header: bool) -> Vec<u8> {
    let mut rom = vec![0xFF; 0x8000];
    let program = [
        0xF3, // DI
        0x3E, 0x04, 0xD3, 0xBF, 0x3E, 0x80, 0xD3, 0xBF, // register 0: Mode 4
        0x3E, 0x40, 0xD3, 0xBF, 0x3E, 0x81, 0xD3, 0xBF, // register 1: display on
        0xDB, 0xBF, // wait: IN A,($BF)
        0x07, // RLCA
        0x30, 0xFB, // JR NC,wait
        0x21, 0x00, 0xC0, // LD HL,$C000
        0x34, // INC (HL)
        0x18, 0xF5, // JR wait
    ];
    rom[..program.len()].copy_from_slice(&program);
    if header {
        rom[0x7FF0..0x7FF8].copy_from_slice(b"TMR SEGA");
    }
    rom
}

fn frames_counted(runtime: &MegaDriveRuntime) -> u8 {
    runtime.machine().expect("machine").z80_ram()[0]
}

fn load(
    session: &mut HeadlessSession<MegaDriveRuntime>,
    image: &[u8],
    cart_type: Option<&'static str>,
) {
    let mut cart = MediaImage::new("cartridge-1", MediaKind::Cartridge, image);
    if let Some(name) = cart_type {
        cart = cart.cart_type(name);
    }
    let mut media = MediaSet::new();
    media.push(cart);
    session.load_media(&media).expect("load");
}

#[test]
fn a_master_system_header_selects_the_converter() {
    let mut runtime = MegaDriveRuntime::new(Model::Genesis, sms_cartridge(true));
    assert!(runtime.power_base());
    let machine = runtime.machine_mut().expect("machine");
    assert!(machine.master_system_mode());
    for _ in 0..5 {
        machine.run_frame();
    }
    let counted = frames_counted(&runtime);
    assert!((4..=5).contains(&counted), "{counted} frames seen");
}

#[test]
fn a_cartridge_type_chooses_for_a_headerless_image() {
    let mut session = HeadlessSession::new(blank(Model::Genesis), Model::Genesis.frame_ticks());
    assert_eq!(
        session.machine().cartridge_types(),
        [MEGA_DRIVE, MASTER_SYSTEM]
    );
    load(&mut session, &sms_cartridge(false), None);
    assert!(
        !session.machine().power_base(),
        "nothing says Master System"
    );
    load(&mut session, &sms_cartridge(false), Some(MASTER_SYSTEM));
    assert!(session.machine().power_base());
    session.run_frames(3).expect("run");
    assert!(frames_counted(session.machine()) >= 2);
    load(&mut session, &sms_cartridge(true), Some(MEGA_DRIVE));
    assert!(
        !session.machine().power_base(),
        "the type overrides the header"
    );
}

#[test]
fn the_converter_survives_resets_switches_and_snapshots() {
    let runtime = MegaDriveRuntime::new(Model::Genesis, sms_cartridge(true));
    let mut session = HeadlessSession::new(runtime, Model::Genesis.frame_ticks());
    session.run_frames(3).expect("run");
    session.reset(ResetKind::Soft).expect("soft reset");
    assert!(session.machine().power_base());
    session.reset(ResetKind::Hard).expect("hard reset");
    assert!(session.machine().power_base());
    session
        .swap_machine(Model::MegaDriveEurope, &FirmwareSet::new())
        .expect("switch");
    assert!(session.machine().power_base());
    session.run_frames(3).expect("run");

    let snapshot = session.machine().snapshot().expect("snapshot");
    let mut restored = blank(Model::MegaDriveEurope);
    restored.restore(&snapshot).expect("restore");
    assert!(restored.power_base());
    assert_eq!(frames_counted(&restored), frames_counted(session.machine()));
}
