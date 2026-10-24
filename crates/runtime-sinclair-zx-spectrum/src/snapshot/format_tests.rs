// Synthetic Format/native evidence. No ROM loading or physical timing claim.
const FORMAT_NATIVE_IDS: [u8; 8] = [7, 8, 255, 3, 4, 5, 9, 0];

fn format_native_fixture(writable: bool, nd: bool) -> (SpectrumPlus3Runtime, Vec<u8>) {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};
    let (_, template) = two_sector_read_runtime(0x46, false);
    let mut image = format_amstrad_dsk::parse(&template).expect("authored template");
    let third = image.tracks[0][0].sectors[1].clone();
    image.tracks[0][0].sectors.push(third);
    for (index, sector) in image.tracks[0][0].sectors.iter_mut().enumerate() {
        sector.id = index as u8 + 1;
        sector.st1 = 0x20;
        sector.st2 = 0x60;
        sector.data = vec![0x21 + index as u8; 512];
    }
    let source = format_amstrad_dsk::encode_extended(&image).expect("authored EDSK");
    let mut runtime = SpectrumPlus3Runtime::blank();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &source).writable(writable));
    runtime.load_media(&media).expect("mount authored EDSK");
    send_fdc_command(&mut runtime, &[3, 0xdf, 2 | u8::from(nd)]);
    (runtime, source)
}

fn format_native_wait(runtime: &mut SpectrumPlus3Runtime, result: bool) {
    for _ in 0..1_000_000 {
        let status = runtime.machine().fdc.read_status();
        if (result && status == 0xd0) || (!result && status & 0x80 != 0) {
            return;
        }
        runtime.machine_mut().fdc.tick();
    }
    panic!("Format native bounded wait exceeded");
}

fn format_native_prefix(runtime: &mut SpectrumPlus3Runtime, count: usize, nd: bool) {
    for &byte in &FORMAT_NATIVE_IDS[..count] {
        format_native_wait(runtime, false);
        assert_eq!(
            runtime.machine().fdc.read_status(),
            if nd { 0xb0 } else { 0x90 }
        );
        assert_eq!(runtime.machine().fdc.interrupt, nd);
        runtime.machine_mut().fdc.write_data(byte);
        assert!(!runtime.machine().fdc.interrupt);
    }
}

fn format_native_live(runtime: &SpectrumPlus3Runtime, accepted: usize, success: bool) {
    let image = runtime
        .machine()
        .fdc
        .disk_for_export(0)
        .expect("inactive live image");
    let sectors = &image.tracks[0][0].sectors;
    assert_eq!(
        sectors.len(),
        if success { 2 } else { 3 },
        "Format live tail policy"
    );
    for (index, sector) in sectors.iter().enumerate() {
        if index < accepted / 4 {
            assert_eq!(
                [sector.c, sector.h, sector.id, sector.n],
                FORMAT_NATIVE_IDS[index * 4..index * 4 + 4]
            );
            assert_eq!((sector.st1, sector.st2), (0, 0));
            assert_eq!(
                sector.data,
                vec![0x6d; 128],
                "command N controls committed payload"
            );
        } else {
            assert_eq!(
                [sector.c, sector.h, sector.id, sector.n],
                [0, 0, index as u8 + 1, 2]
            );
            assert_eq!((sector.st1, sector.st2), (0x20, 0x60));
            assert_eq!(
                sector.data,
                vec![0x21 + index as u8; 512],
                "uncommitted tail intact"
            );
        }
    }
}

fn format_native_roundtrip(runtime: &SpectrumPlus3Runtime) -> SpectrumPlus3Runtime {
    let saved = runtime.snapshot().expect("Format native checkpoint");
    let mut restored = SpectrumPlus3Runtime::blank();
    restored.restore(&saved).expect("restore Format checkpoint");
    assert!(
        restored.snapshot().expect("native fixed point") == saved,
        "Format checkpoint fixed point"
    );
    assert_eq!(
        restored.machine().fdc.read_status(),
        runtime.machine().fdc.read_status(),
        "Format checkpoint MSR"
    );
    assert_eq!(
        restored.machine().fdc.interrupt,
        runtime.machine().fdc.interrupt
    );
    restored
}

fn format_native_result(runtime: &mut SpectrumPlus3Runtime) -> Vec<u8> {
    assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
    assert!(runtime.machine().fdc.interrupt);
    let mut result = Vec::new();
    for _ in 0..7 {
        assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
        result.push(runtime.machine_mut().fdc.read_data());
        assert!(
            !runtime.machine().fdc.interrupt,
            "first result byte acknowledges IRQ"
        );
    }
    assert_eq!(runtime.machine().fdc.read_status(), 0x80);
    result
}

fn format_native_result_boundaries(runtime: &SpectrumPlus3Runtime) {
    let mut baseline = format_native_roundtrip(runtime);
    let expected = format_native_result(&mut baseline);
    let terminal = postcard::to_allocvec(&baseline.machine().fdc).expect("drained controller");
    let mut original = format_native_roundtrip(runtime);
    for drained in 0..=7 {
        let mut restored = format_native_roundtrip(&original);
        assert_eq!(
            restored.machine().fdc.read_status(),
            if drained == 7 { 0x80 } else { 0xd0 }
        );
        assert_eq!(restored.machine().fdc.interrupt, drained == 0);
        for &byte in &expected[drained..] {
            assert_eq!(restored.machine_mut().fdc.read_data(), byte);
            assert!(!restored.machine().fdc.interrupt);
        }
        assert_eq!(restored.machine().fdc.read_status(), 0x80);
        assert_eq!(
            postcard::to_allocvec(&restored.machine().fdc).expect("restored drained controller"),
            terminal
        );
        assert_eq!(
            restored.disk_images()[0].bytes,
            runtime.disk_images()[0].bytes
        );
        if drained < 7 {
            assert_eq!(original.machine_mut().fdc.read_data(), expected[drained]);
        }
    }
}

fn format_native_terminal(
    runtime: &mut SpectrumPlus3Runtime,
    source: &[u8],
    accepted: usize,
    success: bool,
) -> Vec<u8> {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};
    assert_eq!(runtime.machine().fdc.read_status(), 0x80);
    assert!(!runtime.machine().fdc.interrupt);
    assert_eq!(
        runtime.disk_images()[0].bytes,
        source,
        "source cache immutable"
    );
    format_native_live(runtime, accepted, success);
    let before = runtime.snapshot().expect("terminal before export");
    let exported = runtime
        .export_media("disk-a", "edsk")
        .expect("terminal EDSK export");
    assert_eq!(runtime.snapshot().expect("export does not mutate"), before);
    let parsed = format_amstrad_dsk::parse(&exported).expect("export reparses");
    let live = runtime
        .machine()
        .fdc
        .disk_for_export(0)
        .expect("terminal media");
    assert_eq!(
        postcard::to_allocvec(&parsed).expect("parsed image"),
        postcard::to_allocvec(live).expect("live image")
    );
    let mut fresh = SpectrumPlus3Runtime::blank();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &exported));
    fresh
        .load_media(&media)
        .expect("fresh runtime reopens EDSK");
    format_native_live(&fresh, accepted, success);
    assert_eq!(
        fresh.export_media("disk-a", "edsk").expect("fresh export"),
        exported
    );
    send_fdc_command(runtime, &[0xff]);
    assert_eq!(
        runtime.machine_mut().fdc.read_data(),
        0x80,
        "next command recovers"
    );
    assert_eq!(runtime.machine().fdc.read_status(), 0x80);
    exported
}

fn format_native_continue(
    runtime: &mut SpectrumPlus3Runtime,
    action: &str,
    accepted: usize,
    nd: bool,
) -> Vec<u8> {
    if action == "reset" {
        runtime.machine_mut().fdc.reset();
        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
        assert!(!runtime.machine().fdc.interrupt);
        return Vec::new();
    }
    if action == "protect" {
        runtime.machine_mut().fdc.set_write_protect(0, true);
        if runtime.machine().fdc.read_status() & 0x80 != 0 {
            runtime.machine_mut().fdc.write_data(0xff);
        }
    } else if action == "success" {
        for &byte in &FORMAT_NATIVE_IDS[accepted..] {
            format_native_wait(runtime, false);
            assert_eq!(
                runtime.machine().fdc.read_status(),
                if nd { 0xb0 } else { 0x90 }
            );
            assert_eq!(runtime.machine().fdc.interrupt, nd);
            runtime.machine_mut().fdc.write_data(byte);
        }
    }
    // Overrun deliberately services no further requests, including one already high.
    format_native_wait(runtime, true);
    format_native_result_boundaries(runtime);
    let result = format_native_result(runtime);
    match action {
        "protect" => assert_eq!(&result[..3], [0x40, 2, 0]),
        "overrun" => assert_eq!(&result[..3], [0x40, 0x10, 0]),
        "success" => assert_eq!(result, [0, 0, 0, 4, 5, 10, 0], "Format success result"),
        _ => panic!("unknown frozen Format action"),
    }
    result
}

#[test]
fn format_native_protection_interruption_and_reopen_matrix() {
    let mut cases = Vec::new();
    let mut identities = std::collections::HashSet::new();
    for opcode in [0x0d, 0x4d] {
        for nd in [false, true] {
            for action in ["success", "protect", "overrun", "reset"] {
                let last = if action == "success" { 8 } else { 7 };
                for accepted in 0..=last {
                    for rqm_high in [false, true] {
                        let (mut original, source) = format_native_fixture(true, nd);
                        send_fdc_command(&mut original, &[opcode, 0, 0, 2, 42, 0x6d]);
                        format_native_prefix(&mut original, accepted, nd);
                        if rqm_high {
                            format_native_wait(&mut original, false);
                        }
                        let saved = original.snapshot().expect("checkpoint before refusal");
                        if accepted < 8 || !rqm_high {
                            assert!(original.export_media("disk-a", "edsk").is_err());
                            assert_eq!(original.snapshot().expect("refusal read only"), saved);
                        }
                        let mut restored = format_native_roundtrip(&original);
                        assert_eq!(
                            restored.machine().fdc.read_status(),
                            original.machine().fdc.read_status(),
                            "Format checkpoint MSR"
                        );
                        let direct = format_native_continue(&mut original, action, accepted, nd);
                        let resumed = format_native_continue(&mut restored, action, accepted, nd);
                        assert_eq!(
                            resumed, direct,
                            "aborted CHRN continuation, not silicon adjudication"
                        );
                        let completed = if action == "success" { 8 } else { accepted };
                        let exported = format_native_terminal(
                            &mut original,
                            &source,
                            completed,
                            action == "success",
                        );
                        assert_eq!(
                            format_native_terminal(
                                &mut restored,
                                &source,
                                completed,
                                action == "success"
                            ),
                            exported
                        );
                        assert_eq!(
                            original.snapshot().expect("direct final"),
                            restored.snapshot().expect("restored final")
                        );
                        let id = format!("{opcode:02x}/{nd}/{action}/{accepted}/{rqm_high}");
                        assert!(identities.insert(id.clone()));
                        cases.push(serde_json::json!({"id":id,"opcode":opcode,"nd":nd,"action":action,"accepted":accepted,"rqm_high":rqm_high,"result_observed":direct,"status_expected":match action {"protect"=>vec![0x40,2,0],"overrun"=>vec![0x40,0x10,0],"success"=>vec![0,0,0],_=>vec![]},"aborted_chrn_adjudicated":false,"native_equal":true,"source_unchanged":true,"export_reopened":true}));
                    }
                }
            }
            for writable in [false, true] {
                for drained in 0..=7 {
                    let (mut original, source) = format_native_fixture(writable, nd);
                    original.machine_mut().fdc.set_write_protect(0, writable);
                    send_fdc_command(&mut original, &[opcode, 0, 0, 2, 42, 0x6d]);
                    assert_eq!(original.machine().fdc.read_status(), 0xd0);
                    assert!(original.machine().fdc.interrupt);
                    let observed = format_native_result(&mut format_native_roundtrip(&original));
                    assert_eq!(&observed[..3], [0x40, 2, 0]);
                    for _ in 0..drained {
                        original.machine_mut().fdc.read_data();
                    }
                    let mut restored = format_native_roundtrip(&original);
                    for runtime in [&mut original, &mut restored] {
                        assert_eq!(
                            runtime.machine().fdc.read_status(),
                            if drained == 7 { 0x80 } else { 0xd0 }
                        );
                        assert_eq!(runtime.machine().fdc.interrupt, drained == 0);
                        for &expected in &observed[drained..] {
                            assert_eq!(runtime.machine_mut().fdc.read_data(), expected);
                        }
                        format_native_terminal(runtime, &source, 0, false);
                    }
                    assert_eq!(
                        original.snapshot().expect("protected direct"),
                        restored.snapshot().expect("protected restored")
                    );
                    let id = format!("{opcode:02x}/{nd}/initial-protection/{writable}/{drained}");
                    assert!(identities.insert(id.clone()));
                    cases.push(serde_json::json!({"id":id,"opcode":opcode,"nd":nd,"action":"initial-protection","writable":writable,"drained":drained,"result_observed":observed,"status_expected":[0x40,2,0],"native_equal":true,"source_unchanged":true,"export_reopened":true}));
                }
            }
        }
    }
    assert_eq!(cases.len(), 328, "frozen Format checkpoint count");
    assert_eq!(identities.len(), 328, "unique Format checkpoint identities");
    native_field_artifact("format-native.json", &serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,"qualified":false,"snapshot_version":SNAPSHOT_VERSION,"scope":"synthetic component/native continuation and EDSK reopen; no ROM or reference execution","case_count":cases.len(),"required_unresolved":["active READY sampling/status","applicable Fault","aborted CHRN hardware oracle","full Format qualification"],"cases":cases})).expect("Format evidence JSON"));
    eprintln!("FORMAT_NATIVE_CASES=328");
}
