//! Catalogue switches cold-boot the machine on the new model with the same
//! TOS and disks; resets, media and input reach the machine; EmuTOS boots
//! through the whole runtime when it is cached.

mod common;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineCore, MachineError,
    MachineTime, MediaImage, MediaKind, MediaSet, ResetKind,
};
use runtime_atari_st::{AtariStRuntime, Model, Monitor, TOS_FIRMWARE_ID};

fn firmware(tos: &[u8]) -> FirmwareSet<'_> {
    let mut set = FirmwareSet::new();
    set.push(FirmwareImage::new(TOS_FIRMWARE_ID, tos));
    set
}

#[test]
fn every_model_round_trips_and_switches_to_its_actual_hardware() {
    let tos = common::tos();
    let mut runtime = AtariStRuntime::new(Model::St520Pal, tos.clone()).expect("TOS");
    runtime.insert_disk(1, &[0u8; 368_640]).expect("360 KB");
    let mut session = HeadlessSession::new(runtime, Model::St520Pal.frame_ticks(Monitor::Colour));
    for model in Model::ALL {
        session.run_frames(2).expect("run");
        session
            .machine_mut()
            .machine_mut()
            .expect("machine")
            .poke(0x2_0000, 0x73);
        session
            .swap_machine(model, &firmware(&tos))
            .expect("switch");
        assert_eq!(session.machine().model(), model);
        assert_eq!(session.time(), MachineTime::default());
        let machine = session.machine().machine().expect("TOS");
        assert_eq!(machine.model(), model.machine_model());
        assert_eq!(machine.standard(), model.standard());
        assert_eq!(machine.cpu_hz(), model.cpu_hz());
        assert_eq!(machine.rom(), tos);
        assert!(machine.disk(1).is_some(), "drive B kept its disk");
        assert_ne!(machine.peek(0x2_0000), 0x73, "a cold boot");
        session.reset(ResetKind::Hard).expect("reset");
        assert!(
            session
                .machine()
                .machine()
                .expect("machine")
                .disk(1)
                .is_some()
        );
    }
}

/// A soft reset is the reset button: RAM survives, the 68000 starts over.
#[test]
fn a_soft_reset_keeps_ram_and_a_hard_one_clears_it() {
    let mut runtime = AtariStRuntime::new(Model::St1040Pal, common::tos()).expect("TOS");
    let machine = runtime.machine_mut().expect("machine");
    machine.run_frame();
    assert!(
        common::word(machine, common::COUNTER) > 0,
        "the program runs"
    );
    machine.poke(0x2_0000, 0x5A);
    runtime.reset(ResetKind::Soft);
    let machine = runtime.machine().expect("machine");
    assert_eq!(machine.peek(0x2_0000), 0x5A);
    assert_eq!(machine.cpu.regs.pc, 0xFC_0008, "back at the vector");
    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.machine().expect("machine").peek(0x2_0000), 0);
}

#[test]
fn tos_must_be_192_or_256_kb() {
    let error = AtariStRuntime::new(Model::St520Pal, vec![0; 1000])
        .err()
        .expect("refused");
    assert!(
        matches!(error, MachineError::InvalidFirmware { ref id, .. } if id == TOS_FIRMWARE_ID),
        "{error:?}"
    );
}

#[test]
fn disks_load_into_either_drive_and_bad_media_is_refused() {
    let mut runtime = AtariStRuntime::new(Model::St520Pal, common::tos()).expect("TOS");
    let st = vec![0u8; 737_280];
    let load = |runtime: &mut AtariStRuntime, slot: &'static str, kind, bytes: &[u8]| {
        runtime.load_media(&MediaSet {
            images: vec![MediaImage::new(slot, kind, bytes)],
        })
    };
    load(&mut runtime, "floppy-0", MediaKind::Disk, &st).expect("drive A");
    load(&mut runtime, "floppy-1", MediaKind::Disk, &st).expect("drive B");
    let machine = runtime.machine().expect("machine");
    assert!(machine.disk(0).is_some() && machine.disk(1).is_some());
    assert!(matches!(
        load(&mut runtime, "floppy-2", MediaKind::Disk, &st),
        Err(MachineError::UnknownMediaSlot { .. })
    ));
    assert!(matches!(
        load(&mut runtime, "floppy-0", MediaKind::Tape, &st),
        Err(MachineError::UnsupportedMediaKind { .. })
    ));
    assert!(matches!(
        load(&mut runtime, "floppy-0", MediaKind::Disk, &[0; 100]),
        Err(MachineError::InvalidMedia { .. })
    ));

    let exported = runtime.export_media("floppy-0", "st").expect("st");
    assert_eq!(exported, st);
    let msa = runtime.export_media("floppy-0", "msa").expect("msa");
    assert_eq!(&msa[..2], &[0x0E, 0x0F]);
    assert!(runtime.export_media("floppy-0", "adf").is_err());

    runtime.eject_media("floppy-0").expect("eject");
    assert!(runtime.machine().expect("machine").disk(0).is_none());
    assert!(runtime.export_media("floppy-0", "st").is_err());
}

/// Input events reach the IKBD, whose bytes the keyboard ACIA then
/// delivers.
#[test]
fn input_reaches_the_ikbd() {
    let mut runtime = AtariStRuntime::new(Model::St520Pal, common::tos()).expect("TOS");
    // Out of master reset, ÷64 and 8N1, as TOS sets the keyboard ACIA up.
    runtime
        .machine_mut()
        .expect("machine")
        .acia_keyboard
        .write(false, 0x96);
    let events = [
        InputEvent::Key {
            name: "a".into(),
            pressed: true,
        },
        InputEvent::Button {
            port: 1,
            name: "fire".into(),
            pressed: true,
        },
        InputEvent::PointerButton {
            device: "mouse-1".into(),
            button: "right".into(),
            pressed: true,
        },
    ];
    let mut session = HeadlessSession::new(runtime, Model::St520Pal.frame_ticks(Monitor::Colour));
    session.queue_inputs(events);
    session.run_frames(1).expect("run");
    runtime = session.into_machine();
    let machine = runtime.machine_mut().expect("machine");
    assert_eq!(machine.ikbd.stick(1).0, 0x80, "fire on the joystick port");
    // The key byte went out over the serial line into the keyboard ACIA.
    assert_eq!(
        machine.acia_keyboard.read(true),
        0x1E,
        "the A key's make code"
    );
}

#[test]
fn emutos_boots_through_the_runtime() {
    let Some(tos) = common::emutos() else {
        emu198x_test_skip::skip!("EmuTOS 192K image not in ~/.emu198x/test-data/emutos");
    };
    let runtime = AtariStRuntime::new(Model::St520Pal, tos).expect("EmuTOS");
    let ticks = Model::St520Pal.frame_ticks(Monitor::Colour);
    let mut session = HeadlessSession::new(runtime, ticks);
    session.run_frames(50 * 12).expect("run");
    let frame = session.latest_frame().expect("a frame");
    assert_eq!((frame.width, frame.height), (768, 258));
    // The desktop's green covers much of the picture.
    let green = frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] == [0, 0xFF, 0])
        .count();
    assert!(green > 768 * 258 / 5, "{green} green pixels");
}
