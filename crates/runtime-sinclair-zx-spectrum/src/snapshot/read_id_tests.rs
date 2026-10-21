// Included in snapshot.rs's test module to reuse the native envelope helpers.
// Synthetic sector images and input levels; no firmware or save-state artifacts.
fn read_id_native_fixture(kind: &str) -> (SpectrumPlus3Runtime, Vec<u8>, Vec<u8>) {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};
    let (_, template) = writable_sector_runtime(false);
    let mut image = format_amstrad_dsk::parse(&template).expect("source template");
    let sector = image.tracks[0][0].sectors[0].clone();
    image.tracks[0][0].sectors = if kind == "missing-id" {
        vec![]
    } else {
        [7, 9]
            .into_iter()
            .map(|id| {
                let mut next = sector.clone();
                next.c = 7;
                next.h = 3;
                next.id = id;
                next.n = 1;
                next.st1 = if matches!(kind, "id-crc" | "data-crc") {
                    0x20
                } else {
                    0
                };
                next.st2 = if kind == "data-crc" { 0x20 } else { 0 };
                next.data = vec![id; 256];
                next
            })
            .collect()
    };
    image.sides = 2;
    image.tracks.push(image.tracks[0].clone());
    let source = format_amstrad_dsk::encode_extended(&image).expect("authored EDSK");
    let mut runtime = SpectrumPlus3Runtime::blank();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &source));
    runtime.load_media(&media).expect("mount authored fixture");
    runtime.machine_mut().fdc.set_ready(0, kind != "not-ready");
    let live = postcard::to_allocvec(
        runtime
            .machine()
            .fdc
            .disk_for_export(0)
            .expect("live image"),
    )
    .expect("live identity");
    (runtime, source, live)
}

fn read_id_native_expected(kind: &str, selected: u8, id: u8) -> [u8; 7] {
    match kind {
        "missing-id" => [selected | 0x40, 5, 0, 0, selected >> 2, 1, 2],
        "not-ready" => [selected | 0x48, 0, 0, 0, selected >> 2, 1, 2],
        _ => [
            selected | if kind == "id-crc" { 0x40 } else { 0 },
            if kind == "id-crc" { 0x24 } else { 0 },
            0,
            7,
            3,
            id,
            1,
        ],
    }
}

fn read_id_native_drain(runtime: &mut SpectrumPlus3Runtime, expected: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(expected.len());
    for &byte in expected {
        assert_eq!(
            runtime.machine().fdc.read_status(),
            0xd0,
            "Read ID result available"
        );
        let observed = runtime.machine_mut().fdc.read_data();
        assert_eq!(observed, byte, "Read ID expected result");
        assert!(
            !runtime.machine().fdc.interrupt,
            "Read ID result read acknowledges IRQ"
        );
        result.push(observed);
    }
    assert_eq!(
        runtime.machine().fdc.read_status(),
        0x80,
        "Read ID result drained"
    );
    assert_eq!(
        runtime.machine_mut().fdc.read_data(),
        0xff,
        "no duplicate result"
    );
    result
}

fn read_id_native_case(
    kind: &str,
    selected: u8,
    boundary: i8,
    control: Option<&str>,
) -> serde_json::Value {
    let (mut original, source, live) = read_id_native_fixture(kind);
    let idle = original.snapshot().expect("idle fixture snapshot");
    let expected = read_id_native_expected(kind, selected, 7);
    if boundary >= -1 {
        send_fdc_command(&mut original, &[0x4a]);
    }
    if boundary >= 0 {
        send_fdc_command(&mut original, &[selected]);
    }
    let prefix = usize::try_from(boundary.max(0)).expect("nonnegative prefix");
    let mut before = Vec::with_capacity(prefix);
    for &byte in &expected[..prefix] {
        before.push(original.machine_mut().fdc.read_data());
        assert_eq!(before.last(), Some(&byte), "Read ID precheckpoint result");
    }
    let msr = match boundary {
        -2 | 7 => 0x80,
        -1 => 0x90,
        _ => 0xd0,
    };
    assert_eq!(original.machine().fdc.read_status(), msr);
    assert_eq!(
        original.machine().fdc.interrupt,
        boundary == 0,
        "Read ID checkpoint IRQ"
    );
    let saved = original.snapshot().expect("Read ID checkpoint");
    let applied = if control == Some("idle-checkpoint") {
        &idle
    } else {
        &saved
    };
    let mut restored = SpectrumPlus3Runtime::blank();
    restored.restore(applied).expect("restore Read ID boundary");
    assert!(
        restored.snapshot().expect("roundtrip") == *applied,
        "Read ID byte-identical roundtrip"
    );
    assert_eq!(
        restored.machine().fdc.read_status(),
        msr,
        "Read ID checkpoint MSR"
    );
    assert_eq!(
        restored.machine().fdc.interrupt,
        boundary == 0,
        "restored Read ID IRQ"
    );
    let continue_case = |runtime: &mut SpectrumPlus3Runtime| {
        if boundary == -2 {
            send_fdc_command(runtime, &[0x4a]);
        }
        if boundary < 0 {
            send_fdc_command(runtime, &[selected]);
        }
        let mut wanted = expected;
        if control == Some("wrong-result") {
            wanted[0] ^= 0x10;
        }
        let result = if boundary < 7 {
            read_id_native_drain(runtime, &wanted[prefix..])
        } else {
            vec![]
        };
        send_fdc_command(runtime, &[0x4a, selected]);
        let next_id = if matches!(kind, "missing-id" | "not-ready") {
            7
        } else {
            9
        };
        let next = read_id_native_drain(runtime, &read_id_native_expected(kind, selected, next_id));
        (result, next)
    };
    let baseline = continue_case(&mut original);
    let observed = continue_case(&mut restored);
    assert_eq!(
        observed, baseline,
        "Read ID restored continuation matches baseline"
    );
    for runtime in [&original, &restored] {
        assert_eq!(
            runtime.disk_images()[0].bytes,
            source,
            "Read ID immutable source"
        );
        assert_eq!(
            postcard::to_allocvec(runtime.machine().fdc.disk_for_export(0).expect("live disk"))
                .expect("live identity"),
            live,
            "Read ID immutable live image"
        );
    }
    assert!(
        original.snapshot().expect("baseline state") == restored.snapshot().expect("final state"),
        "Read ID final state equality"
    );
    serde_json::json!({"kind":kind,"selected":selected,"boundary":boundary,
        "checkpoint_msr":msr,"checkpoint_irq":boundary==0,"checkpoint_bytes":saved.len(),
        "precheckpoint_result":before,"expected":expected,"continuation_original":baseline,
        "continuation_restored":observed,"roundtrip_equal":true,"final_state_equal":true,
        "source_unchanged":true,"live_unchanged":true})
}

#[test]
fn read_id_native_boundaries_preserve_results_irq_and_next_id() {
    let control = std::env::var("READ_ID_NATIVE_CONTROL").ok();
    assert!(
        matches!(
            control.as_deref(),
            None | Some("idle-checkpoint" | "wrong-result")
        ),
        "known Read ID control"
    );
    if let Some(control) = control.as_deref() {
        let _ = read_id_native_case("good", 0, -1, Some(control));
        panic!("deliberate Read ID control was not rejected");
    }
    let mut cases = Vec::with_capacity(200);
    for kind in ["good", "id-crc", "data-crc", "missing-id", "not-ready"] {
        for selected in [0, 2, 4, 6] {
            for boundary in -2..=7 {
                cases.push(read_id_native_case(kind, selected, boundary, None));
            }
        }
        let (_, source, live) = read_id_native_fixture(kind);
        native_field_artifact(&format!("read-id-native-{kind}.edsk"), &source);
        native_field_artifact(&format!("read-id-native-{kind}-live.postcard"), &live);
    }
    assert_eq!(cases.len(), 200, "all Read ID native cases executed");
    native_field_artifact("read-id-native-boundaries.json", &serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version":1,"qualified":false,"snapshot_version":SNAPSHOT_VERSION,
        "scope":"synthetic digital Read ID native state continuation, not ROM loading or physical timing",
        "case_count":cases.len(),"cases":cases
    })).expect("Read ID native evidence"));
}
