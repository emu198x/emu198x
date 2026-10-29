//! A restored live machine owns the immutable media used by subsequent resets.
use emu198x_shell::{HostIo, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind};
use format198x_spectravideo_svi_cas::{CasImage, encode};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

fn firmware(marker: u8) -> Vec<u8> {
    let mut rom = vec![marker; 32768];
    // DI; SP; PPI A/B input, C output; motor on; select cartridge overlay.
    let mut code = vec![0xF3, 0x31, 0xF0, 0xFF];
    for (port, value) in [(0x97, 0x92), (0x96, 0), (0x88, 15), (0x8C, 0xFE)] {
        code.extend([0x3E, value, 0xD3, port]);
    }
    let again = u16::try_from(code.len()).expect("guest address");
    code.extend([0xDB, 0x98, 0x32, 0, 0xC0]); // tape input -> high RAM
    code.extend([0x3A, 0, 0x80, 0x32, 1, 0xC0]); // cart byte -> high RAM
    code.push(0xC3);
    code.extend(again.to_le_bytes());
    rom[..code.len()].copy_from_slice(&code);
    rom
}
fn tape(marker: u8) -> Vec<u8> {
    encode(&CasImage::new(vec![vec![marker, marker ^ 0xFF]]).expect("CAS blocks")).expect("CAS")
}
fn loaded(model: Model, marker: u8, with_media: bool) -> Svi328Runtime {
    let mut r = Svi328Runtime::new(model, firmware(marker)).expect("firmware");
    if with_media {
        r.insert_cartridge(vec![marker; 16384]).expect("cart");
        r.insert_cassette(&tape(marker)).expect("tape");
    }
    r
}
fn frame(r: &mut Svi328Runtime) {
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut NullFrameSink,
        audio_sink: &mut NullAudioSink,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native frame");
}

#[test]
fn live_restore_keeps_exact_transport_then_resets_with_the_restored_media() {
    for model in Model::ALL {
        for kind in [ResetKind::Soft, ResetKind::Hard] {
            let mut source = loaded(model, 0x5A, true);
            frame(&mut source);
            frame(&mut source); // active leader, beyond the initial silence
            let live = source.snapshot().expect("live state");
            let expected_cold = loaded(model, 0x5A, true).snapshot().expect("cold state");
            assert_ne!(live, expected_cold);
            for occupied in [false, true] {
                let mut restored = if occupied {
                    loaded(model, 0x66, true)
                } else {
                    Svi328Runtime::blank(model)
                };
                restored.restore(&live).expect("restore");
                assert!(
                    restored.cartridge_loaded(),
                    "live cartridge must refresh host queries"
                );
                assert_eq!(
                    restored
                        .snapshot()
                        .expect("capture must not rewind live deck"),
                    live
                );
                let mut control = Svi328Runtime::blank(model);
                control.restore(&live).expect("control restore");
                frame(&mut control);
                frame(&mut restored);
                assert_eq!(
                    restored.snapshot().expect("continuation"),
                    control.snapshot().expect("control")
                );
                restored.reset(kind);
                assert!(
                    restored.machine().is_some(),
                    "reset must preserve restored firmware"
                );
                assert_eq!(
                    restored.snapshot().expect("cold restored media"),
                    expected_cold,
                    "reset must rewind restored tape, stop motor and discard stale receiver images"
                );
                frame(&mut restored);
                assert_eq!(restored.machine().expect("machine").peek(0xC001), 0x5A);
            }
        }
    }
}

#[test]
fn missing_media_in_live_or_blank_states_clears_occupied_receiver_caches() {
    for model in Model::ALL {
        for blank in [false, true] {
            let source = if blank {
                Svi328Runtime::blank(model)
            } else {
                loaded(model, 0x5A, false)
            };
            let state = source.snapshot().expect("source");
            let mut restored = loaded(model, 0x66, true);
            restored.restore(&state).expect("restore without media");
            assert!(
                !restored.cartridge_loaded(),
                "absent cart must clear stale cache"
            );
            restored.reset(ResetKind::Hard);
            assert_eq!(
                restored.snapshot().expect("reset"),
                state,
                "old BIOS/cart/cassette must not reappear after reset"
            );
        }
    }
}

#[test]
fn invalid_restore_does_not_replace_media_used_by_reset() {
    for model in Model::ALL {
        let mut r = loaded(model, 0x5A, true);
        let cold = r.snapshot().expect("cold state");
        frame(&mut r);
        let live = r.snapshot().expect("live state");
        assert!(r.restore(&[0xFF]).is_err());
        assert_eq!(r.snapshot().expect("unchanged"), live);
        r.reset(ResetKind::Hard);
        assert_eq!(r.snapshot().expect("same images"), cold);
    }
}

#[test]
fn staged_media_survives_loading_firmware_with_a_stopped_deck() {
    for model in Model::ALL {
        let mut staged = Svi328Runtime::blank(model);
        staged
            .insert_cartridge(vec![0x5A; 16384])
            .expect("stage cart");
        staged.insert_cassette(&tape(0x5A)).expect("stage tape");
        staged.set_bios(firmware(0x5A)).expect("load firmware");
        let expected = loaded(model, 0x5A, true);
        assert_eq!(
            staged.snapshot().expect("staged boot"),
            expected.snapshot().expect("direct boot")
        );
    }
}
