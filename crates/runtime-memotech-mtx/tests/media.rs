use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet};
use runtime_memotech_mtx::{Model, MtxRuntime};

fn runtime(model: Model) -> MtxRuntime {
    let mut rom = vec![0; 16 * 1024];
    rom[..2].copy_from_slice(&[0xF3, 0x76]); // DI / HALT, without licensed firmware.
    MtxRuntime::new(model, rom).expect("synthetic firmware should construct")
}

#[test]
fn unsupported_media_does_not_report_success_or_change_live_state() {
    for model in Model::ALL {
        for kind in [
            MediaKind::Tape,
            MediaKind::Disk,
            MediaKind::Cartridge,
            MediaKind::Optical,
            MediaKind::Snapshot,
            MediaKind::Program,
        ] {
            let mut runtime = runtime(model);
            let before = runtime.snapshot().expect("live machine should snapshot");
            let mut media = MediaSet::new();
            media.push(MediaImage::new("unsupported", kind, &[0x5A, 0xA5]));

            assert!(
                matches!(
                    runtime.load_media(&media),
                    Err(MachineError::UnsupportedMediaKind { kind: actual }) if actual == kind
                ),
                "MTX must not claim that {kind:?} media was loaded on {model:?}"
            );
            assert_eq!(
                runtime
                    .snapshot()
                    .expect("rejected media should leave a valid machine"),
                before,
                "rejection must preserve live state"
            );
        }
    }
}

#[test]
fn an_empty_media_set_preserves_the_live_machine() {
    for model in Model::ALL {
        let mut runtime = runtime(model);
        let before = runtime.snapshot().expect("live machine should snapshot");
        runtime
            .load_media(&MediaSet::new())
            .expect("firmware-only setup should still succeed");
        assert_eq!(
            runtime.snapshot().expect("live machine should snapshot"),
            before
        );
    }
}
