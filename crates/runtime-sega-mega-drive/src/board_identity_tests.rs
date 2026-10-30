use super::*;
use crate::board_baseline_tests::specimen;
use emu198x_shell::MachineCore;

#[test]
fn foreign_market_payload_under_matching_header_rejects_atomically() {
    #[derive(serde::Serialize)]
    struct Forged<'a, T: serde::Serialize> {
        version: u16,
        time: u64,
        model_id: &'a str,
        machine: Option<&'a T>,
    }
    for target in Model::ALL {
        for source in Model::ALL {
            if target == source {
                continue;
            }
            for pbc in [false, true] {
                let foreign = specimen(source, pbc);
                let mut own = specimen(target, !pbc);
                let before = own.snapshot().expect("before");
                let bytes = postcard::to_allocvec(&Forged {
                    version: 2,
                    time: 123,
                    model_id: target.model_id(),
                    machine: foreign.machine(),
                })
                .expect("forged header");
                assert!(
                    own.restore(&bytes).is_err(),
                    "{source:?} under {target:?} header (PBC {pbc}) accepted"
                );
                assert!(
                    own.snapshot().expect("after") == before,
                    "rejection changed live state/time/media"
                );
            }
        }
    }
}

#[test]
fn releasing_one_button_after_restore_preserves_the_other_held_button() {
    use emu198x_shell::{HostIo, InputEvent, NullAudioSink, NullFrameSink, NullTraceSink};
    for model in Model::ALL {
        for pbc in [false, true] {
            let mut original = specimen(model, pbc);
            let saved = original.snapshot().expect("save");
            let mut restored = MegaDriveRuntime::blank(model);
            restored.restore(&saved).expect("restore");
            let event = InputEvent::Button {
                port: 1,
                name: "a".into(),
                pressed: false,
            };
            for r in [&mut original, &mut restored] {
                r.run_frame(&mut HostIo {
                    input_events: std::slice::from_ref(&event),
                    frame_sink: &mut NullFrameSink,
                    audio_sink: &mut NullAudioSink,
                    trace_sink: &mut NullTraceSink,
                })
                .expect("frame");
                let buttons = r.machine().expect("board").ports()[0].buttons;
                assert!(buttons.up);
                assert!(!buttons.a);
            }
            assert!(
                original.snapshot().expect("original") == restored.snapshot().expect("restored")
            );
        }
    }
}
