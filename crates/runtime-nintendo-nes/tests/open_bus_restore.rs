//! CPU-visible save fidelity and fail-before-decode compatibility policy.
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{Model, NesRuntime};
use std::path::Path;

fn loaded(model: Model) -> NesRuntime {
    let mut rom = vec![0; 16 + 16384];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    rom[16..24].copy_from_slice(&[0xad, 0, 0x40, 0x85, 0, 0x4c, 5, 0x80]);
    rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
    let mut runtime = NesRuntime::blank(model);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("project-written guest");
    runtime
}
#[test]
fn mid_instruction_guest_reads_resume_identically_for_both_boards() {
    for model in Model::ALL {
        let mut live = loaded(model);
        let m = live.machine_mut().expect("loaded");
        let mut reached = false;
        for _ in 0..10_000 {
            if m.cpu.addr == 0x4000 && m.cpu.rw {
                reached = true;
                break;
            }
            m.tick();
        }
        assert!(reached, "guest must reach unmapped read");
        let saved = live.snapshot().expect("v3 native save");
        let mut restored = NesRuntime::blank(model);
        restored.restore(&saved).expect("restore");
        assert_eq!(restored.snapshot().expect("reencoded"), saved);
        for _ in 0..100 {
            live.machine_mut().expect("live").tick();
            restored.machine_mut().expect("restored").tick();
        }
        assert_eq!(live.machine().expect("live").peek(0), 0x40);
        assert_eq!(restored.machine().expect("restored").peek(0), 0x40);
        assert_eq!(
            restored.snapshot().expect("final restored"),
            live.snapshot().expect("final direct")
        );
    }
}
#[test]
fn old_and_future_prefixes_are_rejected_before_payload_decode_atomically() {
    for model in Model::ALL {
        let mut target = loaded(model);
        let before = target.snapshot().expect("before");
        for version in [1u16, 2, 4, 99] {
            let prefix = postcard::to_allocvec(&version).expect("prefix");
            let error = target
                .restore(&prefix)
                .expect_err("no payload required to reject version");
            assert!(
                error
                    .to_string()
                    .contains(&format!("unsupported NES snapshot version {version}")),
                "{error}"
            );
            assert_eq!(target.snapshot().expect("unchanged"), before);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/nes/board-split");
        let mut count = 0;
        for entry in std::fs::read_dir(root).expect("legacy fixtures") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|s| s == "state") {
                let error = target
                    .restore(&std::fs::read(path).expect("v2 file"))
                    .expect_err("complete v2 save rejected");
                assert!(
                    error
                        .to_string()
                        .contains("unsupported NES snapshot version 2"),
                    "{error}"
                );
                assert_eq!(target.snapshot().expect("unchanged"), before);
                count += 1;
            }
        }
        assert_eq!(count, 6, "all complete original saves must be checked");
    }
}
