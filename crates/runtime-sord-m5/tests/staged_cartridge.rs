//! A cartridge accepted before BIOS loading must remain staged across a save.
use emu198x_shell::{
    HostIo, MachineCore, MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink,
    NullTraceSink,
};
use runtime_sord_m5::{M5Runtime, Model};

#[test]
fn staged_cartridges_survive_until_firmware_is_installed() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for model in Model::ALL {
        for marker in [0x17, 0xa5] {
            for dirty in [false, true] {
                let mut cart = vec![0; 128];
                cart[..7].copy_from_slice(&[0xf3, 0x3e, marker, 0x32, 0x00, 0x70, 0x76]);
                let source = cart.clone();
                let mut media = MediaSet::new();
                media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &cart));
                let mut original = M5Runtime::blank(model);
                original.load_media(&media).expect("accepted cartridge");
                assert!(original.cartridge_loaded());
                assert!(original.machine().is_none());
                let state = original.snapshot().expect("staged save");
                let mut restored = M5Runtime::blank(model);
                if dirty {
                    let mut bios = vec![0; 8192];
                    bios[0] = 0x76;
                    restored.set_rom(bios);
                    restored.insert_cartridge(vec![0xee; 100]);
                    assert!(restored.machine().is_some());
                }
                restored.restore(&state).expect("restore");
                assert!(
                    restored.machine().is_none(),
                    "staged state removes stale BIOS"
                );
                assert!(restored.snapshot().expect("round trip") == state);
                assert_eq!(cart, source, "loaded bytes stay immutable");
                if !restored.cartridge_loaded() {
                    failures.push(format!(
                        "{model:?} cart {marker:02x}: accepted staged cartridge lost"
                    ));
                } else {
                    let mut bios = vec![0; 8192];
                    bios[..3].copy_from_slice(&[0xc3, 0x00, 0x20]);
                    restored.set_rom(bios);
                    restored
                        .run_frame(&mut HostIo {
                            input_events: &[],
                            frame_sink: &mut NullFrameSink,
                            audio_sink: &mut NullAudioSink,
                            trace_sink: &mut NullTraceSink,
                        })
                        .expect("native frame");
                    assert_eq!(
                        restored.machine().expect("board").peek(0x7000),
                        marker,
                        "CPU runs the restored cartridge"
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 8);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn original_v5_empty_saves_clear_stale_media_without_inventing_bytes() {
    let originals: &[&[u8]] = &[
        include_bytes!("fixtures/staged-v5/sord-m5-ntsc-17.state").as_slice(),
        include_bytes!("fixtures/staged-v5/sord-m5-ntsc-a5.state").as_slice(),
        include_bytes!("fixtures/staged-v5/sord-m5-pal-17.state").as_slice(),
        include_bytes!("fixtures/staged-v5/sord-m5-pal-a5.state").as_slice(),
    ];
    assert_eq!(originals.len(), 4);
    let mut checked = 0;
    for bytes in originals {
        let (version, time, model_id, machine): (
            u16,
            u64,
            String,
            Option<common_sord_m5::M5State>,
        ) = postcard::from_bytes(bytes).expect("original v5");
        assert_eq!(version, 5);
        assert!(machine.is_none());
        let model = Model::ALL
            .into_iter()
            .find(|m| m.model_id() == model_id)
            .expect("model");
        let mut restored = M5Runtime::blank(model);
        let mut bios = vec![0; 8192];
        bios[0] = 0x76;
        restored.set_rom(bios);
        restored.insert_cartridge(vec![0xee; 100]);
        restored.restore(bytes).expect("explicit v5 reader");
        assert!(restored.machine().is_none());
        assert!(!restored.cartridge_loaded());
        assert_eq!(restored.time().get(), time);
        let current = restored.snapshot().expect("v6");
        assert_eq!(current[0], 6);
        restored.restore(&current).expect("v6 empty reader");
        assert_eq!(restored.snapshot().expect("empty round trip"), current);
        checked += 1;
    }
    assert_eq!(checked, 4);
}

#[test]
fn raw_empty_staging_survives_and_invalid_envelopes_reject_atomically() {
    type Envelope = (
        u16,
        u64,
        String,
        Option<common_sord_m5::M5State>,
        Option<Vec<u8>>,
    );
    for model in Model::ALL {
        for image in [None, Some(vec![]), Some(vec![0xff])] {
            let mut original = M5Runtime::blank(model);
            if let Some(image) = image.clone() {
                original.insert_cartridge(image);
            }
            let saved = original.snapshot().expect("staged raw image");
            let envelope: Envelope = postcard::from_bytes(&saved).expect("v6 envelope");
            assert_eq!(envelope.4, image);
            let mut target = M5Runtime::blank(model);
            target.insert_cartridge(vec![0xaa]);
            target
                .restore(&saved)
                .expect("raw saved image is not a new load");
            assert_eq!(target.cartridge_loaded(), image.is_some());
            assert_eq!(target.snapshot().expect("round trip"), saved);
        }
        let mut target = M5Runtime::blank(model);
        let mut bios = vec![0; 8192];
        bios[0] = 0x76;
        target.set_rom(bios);
        target.insert_cartridge(vec![0xaa; 100]);
        let before = target.snapshot().expect("live save");
        let mut envelope: Envelope = postcard::from_bytes(&before).expect("v6 envelope");
        assert!(envelope.3.is_some());
        assert!(envelope.4.is_none(), "no duplicate live cartridge");
        envelope.4 = Some(vec![0xbb]);
        let contradictory = postcard::to_allocvec(&envelope).expect("contradictory save");
        let error = target
            .restore(&contradictory)
            .expect_err("two cartridge owners");
        assert!(
            error
                .to_string()
                .contains("both a live machine and a staged cartridge")
        );
        assert_eq!(target.snapshot().expect("atomic contradiction"), before);
        for bytes in [vec![6], vec![7], before[..before.len() / 2].to_vec()] {
            target.restore(&bytes).expect_err("invalid payload/version");
            assert_eq!(target.snapshot().expect("atomic rejection"), before);
        }
    }
}
