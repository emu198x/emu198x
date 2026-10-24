// Included by snapshot.rs tests. This is a finite direct-peripheral/native
// proof consumer, not firmware/ROM loading or a physical timing oracle.
fn read_data_native_pattern(seed: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| seed.wrapping_add(index as u8) ^ (index >> 8) as u8)
        .collect()
}

#[derive(Clone)]
struct ReadDataNativeCase {
    name: &'static str,
    command: [u8; 9],
    data: Vec<u8>,
    result: [u8; 7],
    hidden: bool,
}

fn read_data_native_fixture(
    name: &'static str,
    selected: u8,
) -> (SpectrumPlus3Runtime, Vec<u8>, Vec<u8>, ReadDataNativeCase) {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};
    let (_, template) = two_sector_read_runtime(0x46, false);
    let mut image = format_amstrad_dsk::parse(&template).expect("source template");
    for (index, sector) in image.tracks[0][0].sectors.iter_mut().enumerate() {
        sector.data = read_data_native_pattern(0x21 + index as u8, 512);
    }
    let mut command = [0x46, selected, 0, 0, 1, 2, 1, 0x2a, 0xff];
    let mut result = [0x40 | selected, 0x80, 0, 0, 0, 1, 2];
    let mut data = read_data_native_pattern(0x21, 512);
    let mut hidden = false;
    match name {
        "clean-single" => {}
        "clean-multiple" => {
            command[6] = 2;
            result[5] = 2;
            data.extend(read_data_native_pattern(0x22, 512));
        }
        "missing-first" => {
            image.tracks[0][0].sectors.remove(0);
            data.clear();
            result[1] = 4;
        }
        "missing-later" => {
            image.tracks[0][0].sectors.truncate(1);
            command[6] = 2;
            result[1] = 4;
            result[5] = 2;
        }
        "wrong-c" | "bad-c" => {
            image.tracks[0][0].sectors[0].c = if name == "bad-c" { 255 } else { 7 };
            data.clear();
            result[1] = 4;
            result[2] = if name == "bad-c" { 0x12 } else { 0x10 };
        }
        "wrong-h" => {
            image.tracks[0][0].sectors[0].h = 1;
            data.clear();
            result[1] = 4;
        }
        "wrong-n" => {
            image.tracks[0][0].sectors[0].n = 1;
            data.clear();
            result[1] = 4;
        }
        "id-crc" => {
            image.tracks[0][0].sectors[0].st1 = 0x20;
            data.clear();
            result[1] = 0x20;
        }
        "data-crc" => {
            image.tracks[0][0].sectors[0].st1 = 0x20;
            image.tracks[0][0].sectors[0].st2 = 0x20;
            result[1] = 0x20;
            result[2] = 0x20;
        }
        "deleted-mark" => {
            image.tracks[0][0].sectors[0].st2 = 0x40;
            result[1] = 0;
            result[2] = 0x40;
        }
        "skip-mark" => {
            image.tracks[0][0].sectors[0].st2 = 0x40;
            command[0] = 0x66;
            command[6] = 2;
            data = read_data_native_pattern(0x22, 512);
            result[5] = 2;
        }
        "all-skipped" => {
            image.tracks[0][0].sectors[0].st2 = 0x40;
            command[0] = 0x66;
            data.clear();
            result[1] = 4;
        }
        "dtl2" | "dtl127" => {
            command[0] = 6;
            command[5] = 0;
            command[8] = if name == "dtl2" { 2 } else { 127 };
            let sector = &mut image.tracks[0][0].sectors[0];
            sector.n = 0;
            sector.data = read_data_native_pattern(0x21, 128);
            data = read_data_native_pattern(0x21, usize::from(command[8]));
            result[6] = 0;
            hidden = true;
        }
        "multitrack" | "missing-side" => {
            command[0] = 0xc6;
            command[5] = 1;
            result = [
                0x44 | selected,
                if name == "missing-side" { 4 } else { 0x80 },
                0,
                0,
                1,
                1,
                1,
            ];
            data = read_data_native_pattern(0x21, 256);
            image.tracks[0][0].sectors.truncate(1);
            image.tracks[0][0].sectors[0].n = 1;
            image.tracks[0][0].sectors[0].data.truncate(256);
            image.sides = 2;
            let mut side = image.tracks[0].clone();
            side[0].sectors[0].h = 1;
            side[0].sectors[0].data = read_data_native_pattern(0x31, 256);
            if name == "missing-side" {
                side[0].sectors.clear();
            } else {
                data.extend(read_data_native_pattern(0x31, 256));
            }
            image.tracks.push(side);
        }
        "preserved-copies" => {
            command[5] = 1;
            result = [0x40 | selected, 0x20, 0x20, 0, 0, 1, 1];
            data = read_data_native_pattern(0x31, 256);
            let sector = &mut image.tracks[0][0].sectors[0];
            sector.n = 1;
            sector.st1 = 0x20;
            sector.st2 = 0x20;
            sector.data = [
                read_data_native_pattern(0x21, 256),
                read_data_native_pattern(0x31, 256),
                read_data_native_pattern(0x41, 256),
            ]
            .concat();
        }
        "not-ready" => {
            data.clear();
            result[0] |= 8;
            result[1] = 0;
        }
        "missing-dam" => {
            let sector = &mut image.tracks[0][0].sectors[0];
            sector.st1 = 1;
            sector.st2 = 1;
            data.clear();
            result[1] = 1;
            result[2] = 1;
        }
        _ => panic!("unknown Read Data kind"),
    }
    let source = format_amstrad_dsk::encode_extended(&image).expect("authored EDSK");
    let mut runtime = SpectrumPlus3Runtime::blank();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &source));
    runtime.load_media(&media).expect("mount fixture");
    send_fdc_command(&mut runtime, &[3, 0xdf, 3]); // explicit SPECIFY ND=1
    runtime.machine_mut().fdc.set_ready(0, name != "not-ready");
    runtime
        .machine_mut()
        .fdc
        .set_two_side(0, matches!(name, "multitrack" | "missing-side"));
    runtime.machine_mut().fdc.set_track_zero(0, true);
    runtime.machine_mut().fdc.set_write_protect(0, true);
    runtime.machine_mut().fdc.set_fault(0, false);
    if name == "preserved-copies" {
        send_fdc_command(&mut runtime, &command);
        read_data_native_data(&mut runtime, &read_data_native_pattern(0x21, 256));
        read_data_native_result(&mut runtime, &result, None);
    }
    let live = postcard::to_allocvec(runtime.machine().fdc.disk_for_export(0).expect("live disk"))
        .expect("live identity");
    (
        runtime,
        source,
        live,
        ReadDataNativeCase {
            name,
            command,
            data,
            result,
            hidden,
        },
    )
}

fn read_data_native_data(runtime: &mut SpectrumPlus3Runtime, expected: &[u8]) -> Vec<u8> {
    let mut observed = Vec::with_capacity(expected.len());
    for &byte in expected {
        await_fdc_read_request(runtime);
        assert!(runtime.machine().fdc.interrupt, "Read Data ND request IRQ");
        let actual = runtime.machine_mut().fdc.read_data();
        assert_eq!(actual, byte, "Read Data expected payload");
        observed.push(actual);
        assert_eq!(
            runtime.machine().fdc.interrupt,
            runtime.machine().fdc.read_status() == 0xd0,
            "accepted byte acknowledges request, final byte may complete"
        );
    }
    observed
}
fn read_data_native_result(
    runtime: &mut SpectrumPlus3Runtime,
    expected: &[u8],
    control: Option<&str>,
) -> Vec<u8> {
    for _ in 0..50_000 {
        if runtime.machine().fdc.read_status() == 0xd0 {
            break;
        }
        assert_eq!(
            runtime.machine().fdc.read_status(),
            0x70,
            "Read Data expected execution end without extra host bytes"
        );
        assert!(!runtime.machine().fdc.interrupt);
        runtime.machine_mut().fdc.tick();
    }
    let mut observed = Vec::new();
    for (index, &byte) in expected.iter().enumerate() {
        assert_eq!(
            runtime.machine().fdc.read_status(),
            0xd0,
            "Read Data result framing"
        );
        let actual = runtime.machine_mut().fdc.read_data();
        let wanted = if control == Some("wrong-result") && index == 0 {
            byte ^ 0x10
        } else {
            byte
        };
        assert_eq!(actual, wanted, "Read Data expected result");
        assert!(
            !runtime.machine().fdc.interrupt,
            "first result acknowledges completion"
        );
        observed.push(actual);
    }
    assert_eq!(
        runtime.machine().fdc.read_status(),
        0x80,
        "seven bytes drain to idle"
    );
    assert_eq!(runtime.machine_mut().fdc.read_data(), 0xff);
    observed
}

#[derive(Clone, Debug)]
struct ReadDataNativePoint {
    name: String,
    command_prefix: usize,
    payload_prefix: usize,
    request: bool,
    result_prefix: Option<usize>,
    hidden_ticks: u32,
}
fn read_data_native_points(case: &ReadDataNativeCase) -> Vec<ReadDataNativePoint> {
    let point =
        |name: String, command_prefix, payload_prefix, request, result_prefix, hidden_ticks| {
            ReadDataNativePoint {
                name,
                command_prefix,
                payload_prefix,
                request,
                result_prefix,
                hidden_ticks,
            }
        };
    let mut points = vec![
        point("idle".into(), 0, 0, false, None, 0),
        point("opcode".into(), 1, 0, false, None, 0),
        point("partial-command".into(), 4, 0, false, None, 0),
    ];
    if !case.data.is_empty() {
        let mut prefixes = vec![0, 1, case.data.len() - 1];
        if case.name == "clean-multiple" {
            prefixes.extend([511, 512, 513]);
        }
        if case.name == "multitrack" {
            prefixes.extend([255, 256, 257]);
        }
        prefixes.sort_unstable();
        prefixes.dedup();
        for prefix in prefixes {
            for request in [false, true] {
                points.push(point(
                    format!("payload-{prefix}-{}", if request { "high" } else { "low" }),
                    9,
                    prefix,
                    request,
                    None,
                    0,
                ));
            }
        }
        if case.hidden {
            for ticks in {
                let boundary = |slot: u64| (slot * 64 * 3_546_895).div_ceil(1_000_000);
                let remaining = boundary(19 + 127) - boundary(19 + case.data.len() as u64 - 1);
                [0, remaining as u32 / 2, remaining as u32 - 1]
            } {
                points.push(point(
                    format!("hidden-{ticks}"),
                    9,
                    case.data.len(),
                    false,
                    None,
                    ticks,
                ));
            }
        }
    }
    if case.name == "clean-multiple" {
        for prefix in [0, 1, 511, 512, 513, 1023] {
            let boundary = |slot: u64| (slot * 32 * 3_546_895).div_ceil(1_000_000);
            let slot = 39 + (prefix % 512) as u64;
            let elapsed = (boundary(slot + 1) - boundary(slot) - 1) as u32;
            points.push(point(
                format!("overrun-{prefix}"),
                9,
                prefix,
                true,
                None,
                elapsed,
            ));
        }
    }
    for prefix in 0..=7 {
        points.push(point(
            format!("result-{prefix}"),
            9,
            case.data.len(),
            false,
            Some(prefix),
            0,
        ));
    }
    points
}
fn read_data_native_case(
    kind: &'static str,
    selected: u8,
    point: &ReadDataNativePoint,
    control: Option<&str>,
) -> serde_json::Value {
    eprintln!(
        "Read Data native case kind={kind} selected={selected} checkpoint={}",
        point.name
    );
    let (mut original, source, live, mut case) = read_data_native_fixture(kind, selected);
    let overrun = point.name.starts_with("overrun-");
    if overrun {
        case.result = [
            0x40 | selected,
            0x10,
            0,
            0,
            0,
            if point.payload_prefix >= 512 { 2 } else { 1 },
            2,
        ];
    }
    let idle = original.snapshot().expect("idle checkpoint");
    send_fdc_command(&mut original, &case.command[..point.command_prefix]);
    let mut before_data = Vec::new();
    let mut before_result = Vec::new();
    if point.command_prefix == 9 {
        before_data = read_data_native_data(&mut original, &case.data[..point.payload_prefix]);
        if point.request {
            await_fdc_read_request(&mut original);
        }
        for _ in 0..point.hidden_ticks {
            assert_eq!(
                original.machine().fdc.read_status(),
                if overrun { 0xf0 } else { 0x70 }
            );
            assert_eq!(original.machine().fdc.interrupt, overrun);
            original.machine_mut().fdc.tick();
        }
        if let Some(prefix) = point.result_prefix {
            for _ in 0..50_000 {
                if original.machine().fdc.read_status() == 0xd0 {
                    break;
                }
                assert_eq!(original.machine().fdc.read_status(), 0x70);
                original.machine_mut().fdc.tick();
            }
            assert!(original.machine().fdc.interrupt, "Read Data completion IRQ");
            for &byte in &case.result[..prefix] {
                assert_eq!(original.machine().fdc.read_status(), 0xd0);
                let actual = original.machine_mut().fdc.read_data();
                assert_eq!(actual, byte);
                before_result.push(actual);
                assert!(!original.machine().fdc.interrupt);
            }
        }
    }
    let msr = if point.command_prefix == 0 || point.result_prefix == Some(7) {
        0x80
    } else if point.command_prefix < 9 {
        0x90
    } else if point.result_prefix.is_some() {
        0xd0
    } else if point.request {
        0xf0
    } else {
        0x70
    };
    let irq = point.request || point.result_prefix == Some(0);
    assert_eq!(
        original.machine().fdc.read_status(),
        msr,
        "Read Data checkpoint MSR"
    );
    assert_eq!(
        original.machine().fdc.interrupt,
        irq,
        "Read Data checkpoint IRQ"
    );
    let saved = original.snapshot().expect("Read Data checkpoint");
    let applied = if control == Some("idle-checkpoint") {
        &idle
    } else {
        &saved
    };
    let mut restored = SpectrumPlus3Runtime::blank();
    restored.restore(applied).expect("restore Read Data");
    assert_eq!(restored.snapshot().expect("roundtrip"), *applied);
    assert_eq!(
        restored.machine().fdc.read_status(),
        msr,
        "Read Data restored checkpoint MSR"
    );
    assert_eq!(restored.machine().fdc.interrupt, irq);
    for runtime in [&original, &restored] {
        assert_eq!(
            runtime.disk_images()[0].bytes,
            source,
            "Read Data immutable source at checkpoint"
        );
        assert_eq!(
            postcard::to_allocvec(
                runtime
                    .machine()
                    .fdc
                    .disk_for_export(0)
                    .expect("checkpoint live image")
            )
            .expect("checkpoint live identity"),
            live,
            "Read Data immutable live bytes at checkpoint"
        );
    }
    let continue_case = |runtime: &mut SpectrumPlus3Runtime| {
        if point.command_prefix < 9 {
            send_fdc_command(runtime, &case.command[point.command_prefix..]);
        }
        let data = if overrun {
            assert_eq!(runtime.machine().fdc.read_status(), 0xf0);
            runtime.machine_mut().fdc.tick();
            assert_eq!(
                runtime.machine().fdc.read_status(),
                0xd0,
                "remaining overrun deadline is one tick"
            );
            assert!(runtime.machine().fdc.interrupt, "overrun completion IRQ");
            vec![]
        } else if point.result_prefix.is_none() {
            read_data_native_data(runtime, &case.data[point.payload_prefix..])
        } else {
            vec![]
        };
        let result = if point.result_prefix == Some(7) {
            vec![]
        } else {
            read_data_native_result(
                runtime,
                &case.result[point.result_prefix.unwrap_or(0)..],
                control,
            )
        };
        let next_preserved = if case.name == "preserved-copies" {
            send_fdc_command(runtime, &case.command);
            let data = read_data_native_data(runtime, &read_data_native_pattern(0x41, 256));
            let result = read_data_native_result(runtime, &case.result, None);
            Some(serde_json::json!({"data":data,"result":result}))
        } else {
            None
        };
        send_fdc_command(runtime, &[0xff]);
        assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
        assert_eq!(
            runtime.machine_mut().fdc.read_data(),
            0x80,
            "invalid-command recovery"
        );
        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
        assert!(!runtime.machine().fdc.interrupt);
        serde_json::json!({"data":data,"result":result,"next_preserved":next_preserved})
    };
    let baseline = continue_case(&mut original);
    let observed = continue_case(&mut restored);
    assert_eq!(baseline, observed, "Read Data continuation equality");
    for runtime in [&original, &restored] {
        assert_eq!(runtime.disk_images()[0].bytes, source);
        assert_eq!(
            postcard::to_allocvec(runtime.machine().fdc.disk_for_export(0).expect("live"))
                .expect("live identity"),
            live
        );
    }
    assert_eq!(
        original.snapshot().expect("final"),
        restored.snapshot().expect("restored final")
    );
    serde_json::json!({"kind":kind,"selected":selected,"checkpoint":point.name,"command":case.command,"specify":[3,223,3],"inputs":{"physical_drive":0,"select_mask":1,"ready":kind!="not-ready","two_side":matches!(kind,"multitrack"|"missing-side"),"track_zero":true,"write_protect":true,"fault":false,"non_dma":true,"physical_track":0},"source":format!("read-data-{kind}.edsk"),"live":format!("read-data-{kind}-live.postcard"),"checkpoint_bytes":saved.len(),"precheckpoint_data":before_data,"precheckpoint_result":before_result,"command_prefix":point.command_prefix,"payload_prefix":point.payload_prefix,"result_prefix":point.result_prefix,"hidden_ticks":point.hidden_ticks,"checkpoint_msr":msr,"checkpoint_irq":irq,"expected_data":if overrun {&case.data[..point.payload_prefix]} else {&case.data},"overrun":overrun,"expected_result":case.result,"result_oracle":if kind=="all-skipped" {"retained sector-model convention; independent final-status oracle unresolved"}else{"declared digital model with primary-supported error/data semantics; no-TC final-field convention"},"expected_next_preserved":if kind=="preserved-copies"{Some(serde_json::json!({"data":read_data_native_pattern(0x41,256),"result":case.result}))}else{None},"continuation_original":baseline,"continuation_restored":observed,"roundtrip_equal":true,"final_state_equal":true,"source_unchanged":true,"live_unchanged":true,"recovery_msr":128})
}

#[test]
fn read_data_native_boundaries_preserve_payload_result_irq_and_media() {
    const {
        assert!(
            SNAPSHOT_VERSION >= 10,
            "Read Data native proof requires approved SPECIFY ND state"
        );
    }
    let control = std::env::var("READ_DATA_NATIVE_CONTROL").ok();
    assert!(
        matches!(
            control.as_deref(),
            None | Some("idle-checkpoint" | "wrong-result")
        ),
        "known Read Data control"
    );
    if let Some(control) = control.as_deref() {
        let (_, _, _, case) = read_data_native_fixture("clean-single", 0);
        let point = read_data_native_points(&case)
            .into_iter()
            .find(|p| p.name == "payload-1-high")
            .expect("active control point");
        read_data_native_case("clean-single", 0, &point, Some(control));
        panic!("deliberate control was not rejected");
    }
    let kinds = [
        "clean-single",
        "clean-multiple",
        "missing-first",
        "missing-later",
        "wrong-c",
        "bad-c",
        "wrong-h",
        "wrong-n",
        "id-crc",
        "data-crc",
        "deleted-mark",
        "skip-mark",
        "all-skipped",
        "dtl2",
        "dtl127",
        "multitrack",
        "missing-side",
        "preserved-copies",
        "not-ready",
        "missing-dam",
    ];
    assert_eq!(
        kinds.iter().collect::<std::collections::HashSet<_>>().len(),
        20,
        "all Read Data kinds unique"
    );
    let mut cases = Vec::new();
    let mut identities = std::collections::HashSet::new();
    let mut expected_count = 0;
    for kind in kinds {
        for selected in [0, 2] {
            let (_, source, live, case) = read_data_native_fixture(kind, selected);
            let points = read_data_native_points(&case);
            expected_count += points.len();
            for point in points {
                assert!(
                    identities.insert(format!("{kind}/{selected}/{}", point.name)),
                    "each Read Data native checkpoint executes once"
                );
                cases.push(read_data_native_case(kind, selected, &point, None));
            }
            if selected == 0 {
                native_field_artifact(&format!("read-data-{kind}.edsk"), &source);
                native_field_artifact(&format!("read-data-{kind}-live.postcard"), &live);
            }
        }
    }
    assert_eq!(cases.len(), expected_count);
    assert_eq!(
        identities.len(),
        616,
        "all planned Read Data identities observed"
    );
    assert_eq!(
        expected_count, 616,
        "all planned Read Data native cases enumerated"
    );
    assert_eq!(
        cases.len(),
        616,
        "all planned Read Data native cases executed"
    );
    native_field_artifact("read-data-native-boundaries.json",&serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,"qualified":false,"snapshot_version":SNAPSHOT_VERSION,"scope":"synthetic Read Data digital native continuation, not ROM loading or physical timing","case_count":cases.len(),"kind_count":20,"required_blocked_research":["N0 DTL0 exact host length/result oracle","all-skipped final status/CHRN independent oracle"],"cases":cases})).expect("Read Data evidence"));
}
