//! Regressions for state omitted by the original PET v4 save contract.
use crate::{Model, PetRuntime};
use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
fn runtime(model: Model) -> PetRuntime {
    let mut kernal = vec![0xea; 0x1000];
    kernal[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
    kernal[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    PetRuntime::new(
        model,
        kernal,
        vec![0; model.basic_size()],
        vec![0; 0x800],
        vec![0; 0x800],
    )
    .expect("synthetic firmware")
}
fn field(r: &mut PetRuntime) {
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut NullFrameSink,
        audio_sink: &mut NullAudioSink,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native frame");
}
fn queued_program(model: Model) {
    let mut live = runtime(model);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "program-1",
        MediaKind::Program,
        &[0, 2, 0x42],
    ));
    live.load_media(&media).expect("queue PRG");
    let saved = live.snapshot().expect("save");
    let mut restored = runtime(model);
    restored.restore(&saved).expect("restore");
    for _ in 0..121 {
        field(&mut live);
        field(&mut restored);
    }
    assert_eq!(
        live.machine().expect("live").peek(0x200),
        0x42,
        "control proves the queue reached RAM"
    );
    assert_eq!(
        restored.machine().expect("restored").peek(0x200),
        live.machine().expect("live").peek(0x200),
        "queued PRG is lost across restore"
    );
}
fn queued_audio(model: Model) {
    let mut live = runtime(model);
    let machine = live.machine_mut().expect("machine");
    for _ in 0..20 {
        machine.step_instruction();
    }
    let now = machine.master_clock();
    live.set_time(MachineTime::new(now));
    let saved = live.snapshot().expect("save");
    let mut restored = runtime(model);
    restored.restore(&saved).expect("restore");
    let before = live.machine_mut().expect("live").take_audio_buffer();
    assert!(!before.is_empty(), "control has undelivered samples");
    let after = restored
        .machine_mut()
        .expect("restored")
        .take_audio_buffer();
    assert_eq!(after, before, "undelivered audio is lost across restore");
}
#[test]
fn queued_program_all_boards() {
    for model in Model::ALL {
        queued_program(model);
    }
}
#[test]
fn queued_audio_all_boards() {
    for model in Model::ALL {
        queued_audio(model);
    }
}
#[test]
fn arbitrary_phase_continuations_preserve_first_pixels_audio_and_clocks() {
    for model in Model::ALL {
        for phase in [1, 17, 127, 1021] {
            let mut live = runtime(model);
            let m = live.machine_mut().expect("machine");
            m.press_key(common_commodore_pet::PetKey::Plus);
            m.poke(0x8000, 0x81);
            for _ in 0..phase {
                m.step_instruction();
            }
            let now = m.master_clock();
            live.set_time(MachineTime::new(now));
            let saved = live.snapshot().expect("snapshot");
            let mut restored = runtime(model);
            restored.restore(&saved).expect("restore");
            for _ in 0..3 {
                field(&mut live);
                field(&mut restored);
                assert_eq!(
                    live.snapshot().expect("live"),
                    restored.snapshot().expect("restored")
                );
                assert_eq!(
                    live.machine().expect("live").framebuffer(),
                    restored.machine().expect("restore").framebuffer()
                );
            }
        }
    }
}

#[test]
fn replacement_firmware_resets_the_clock_and_survives_restore_and_reset() {
    use emu198x_shell::ResetKind;
    for model in Model::ALL {
        let mut live = runtime(model);
        field(&mut live);
        assert!(live.time().get() > 0);
        let mut kernal = vec![0xea; 4096];
        kernal[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
        kernal[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
        live.set_roms(
            kernal,
            vec![0x42; model.basic_size()],
            vec![0x45; 2048],
            vec![0; 2048],
        )
        .expect("replace");
        assert_eq!(live.time().get(), 0);
        let saved = live.snapshot().expect("save");
        let mut restored = runtime(model);
        restored.restore(&saved).expect("restore");
        restored.reset(ResetKind::Hard);
        assert_eq!(restored.machine().expect("machine").peek(0xe000), 0x45);
        assert_eq!(restored.snapshot().expect("reset"), saved);
    }
}
