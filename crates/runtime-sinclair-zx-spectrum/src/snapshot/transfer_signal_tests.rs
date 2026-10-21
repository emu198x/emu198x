// Programmed-mode/native IRQ tests use authored synthetic media and blank ROMs.
#[derive(Clone, Copy, Debug)]
enum TransferBoundary {
    Configured,
    CommandPartial,
    Preamble,
    FirstRequest,
    FirstAccepted,
    InteriorRequest,
    FinalRequest,
    Result(u8),
}

impl TransferBoundary {
    fn prefix(self, total: usize) -> usize {
        match self {
            Self::FirstAccepted => 1,
            Self::InteriorRequest => total / 2,
            Self::FinalRequest => total - 1,
            Self::Result(_) => total,
            _ => 0,
        }
    }

    fn msr(self, opcode: u8, nd: bool) -> u8 {
        match self {
            Self::Configured | Self::Result(7) => 0x80,
            Self::CommandPartial => 0x90,
            Self::Result(_) => 0xd0,
            _ => {
                0x10 | if transfer_is_read(opcode) { 0x40 } else { 0 }
                    | if nd { 0x20 } else { 0 }
                    | if matches!(
                        self,
                        Self::FirstRequest | Self::InteriorRequest | Self::FinalRequest
                    ) {
                        0x80
                    } else {
                        0
                    }
            }
        }
    }

    fn irq(self, nd: bool) -> bool {
        matches!(self, Self::Result(0))
            || (nd
                && matches!(
                    self,
                    Self::FirstRequest | Self::InteriorRequest | Self::FinalRequest
                ))
    }
}

fn transfer_is_read(opcode: u8) -> bool {
    matches!(opcode, 0x46 | 0x4c | 0x42)
}

fn transfer_command(opcode: u8) -> Vec<u8> {
    if opcode == 0x4d {
        vec![opcode, 0, 2, 1, 42, 0xe5]
    } else {
        vec![opcode, 0, 0, 0, 1, 2, 1, 42, 1]
    }
}

fn transfer_payload(opcode: u8) -> Vec<u8> {
    if opcode == 0x4d {
        vec![0, 0, 1, 2]
    } else {
        vec![0xe5; 512]
    }
}

fn transfer_request(runtime: &mut SpectrumPlus3Runtime) {
    transfer_request_limit(runtime, 100_000);
}

fn transfer_request_limit(runtime: &mut SpectrumPlus3Runtime, limit: u32) {
    use common_sinclair_zx_spectrum::peripheral::Peripheral;
    for _ in 0..limit {
        if runtime.machine().fdc.read_status() & 0x80 != 0 {
            return;
        }
        runtime.machine_mut().fdc.tick(0);
    }
    panic!("transfer request exceeded {limit} ticks");
}

fn transfer_consume(
    runtime: &mut SpectrumPlus3Runtime,
    opcode: u8,
    nd: bool,
    start: usize,
    end: usize,
) {
    let payload = transfer_payload(opcode);
    for (index, &byte) in payload.iter().enumerate().take(end).skip(start) {
        transfer_request(runtime);
        assert_eq!(
            runtime.machine().fdc.read_status() & 0x20 != 0,
            nd,
            "programmed native EXM"
        );
        assert_eq!(
            runtime.machine().fdc.interrupt,
            nd,
            "native payload-request IRQ"
        );
        if transfer_is_read(opcode) {
            assert_eq!(
                runtime.machine_mut().fdc.read_data(),
                byte,
                "native payload"
            );
        } else {
            runtime.machine_mut().fdc.write_data(byte);
        }
        assert_eq!(
            runtime.machine().fdc.interrupt,
            transfer_is_read(opcode) && index + 1 == payload.len(),
            "native accepted-byte/completion IRQ ordering"
        );
    }
}

fn transfer_result_ready(runtime: &mut SpectrumPlus3Runtime) {
    // Format's existing model finishes its full-track post-ID interval.
    transfer_request_limit(runtime, 1_000_000);
    assert_eq!(
        runtime.machine().fdc.read_status(),
        0xd0,
        "native result phase"
    );
    assert!(
        runtime.machine().fdc.interrupt,
        "native completion IRQ independent of ND"
    );
}

fn transfer_read_result(runtime: &mut SpectrumPlus3Runtime, count: usize) -> Vec<u8> {
    (0..count)
        .map(|_| {
            assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
            let byte = runtime.machine_mut().fdc.read_data();
            assert!(
                !runtime.machine().fdc.interrupt,
                "native first-result acknowledgement"
            );
            byte
        })
        .collect()
}

fn transfer_native_case(
    opcode: u8,
    nd: bool,
    boundary: TransferBoundary,
    control: Option<&str>,
) -> serde_json::Value {
    let (mut original, source) = writable_sector_runtime(true);
    original.machine_mut().fdc.set_ready(0, true);
    send_fdc_command(&mut original, &[3, 0xdf, 2 | u8::from(nd)]);
    send_fdc_command(&mut original, &[4, 0]);
    let _ = original.machine_mut().fdc.read_data();
    let configured = original.snapshot().expect("configured mode checkpoint");
    let command = transfer_command(opcode);
    let total = transfer_payload(opcode).len();
    match boundary {
        TransferBoundary::Configured => {}
        TransferBoundary::CommandPartial => {
            send_fdc_command(&mut original, &command[..command.len() - 1])
        }
        _ => send_fdc_command(&mut original, &command),
    }
    let prefix = boundary.prefix(total);
    if prefix != 0 {
        transfer_consume(&mut original, opcode, nd, 0, prefix);
    }
    let mut before = Vec::new();
    match boundary {
        TransferBoundary::FirstRequest
        | TransferBoundary::InteriorRequest
        | TransferBoundary::FinalRequest => transfer_request(&mut original),
        TransferBoundary::Result(drained) => {
            transfer_result_ready(&mut original);
            before = transfer_read_result(&mut original, usize::from(drained));
        }
        _ => {}
    }
    assert_eq!(
        original.machine().fdc.read_status(),
        boundary.msr(opcode, nd)
    );
    assert_eq!(original.machine().fdc.interrupt, boundary.irq(nd));
    let writing = matches!(opcode, 0x45 | 0x49 | 0x4d)
        && !matches!(
            boundary,
            TransferBoundary::Configured
                | TransferBoundary::CommandPartial
                | TransferBoundary::Result(_)
        );
    assert_eq!(
        original.machine().fdc.disk_for_export(0).is_err(),
        writing,
        "active write export remains rejected"
    );
    // Native state includes the authoritative live disk even when host export
    // correctly rejects an unfinished write. No export guard is bypassed.
    let controller_checkpoint =
        postcard::to_allocvec(&original.machine().fdc).expect("controller checkpoint");
    let saved = original.snapshot().expect("transfer checkpoint");
    let applied = if control == Some("idle-checkpoint") {
        &configured
    } else {
        &saved
    };
    let mut restored = SpectrumPlus3Runtime::blank();
    restored
        .restore(applied)
        .expect("restore transfer mode/request/result");
    assert!(
        restored.snapshot().expect("roundtrip") == *applied,
        "native byte-identical checkpoint"
    );
    let wanted_nd = if control == Some("wrong-mode") {
        !nd
    } else {
        nd
    };
    assert_eq!(
        restored.machine().fdc.read_status(),
        boundary.msr(opcode, wanted_nd),
        "native transfer checkpoint MSR"
    );
    assert_eq!(
        restored.machine().fdc.interrupt,
        boundary.irq(wanted_nd),
        "native transfer checkpoint IRQ"
    );
    assert_eq!(
        postcard::to_allocvec(&restored.machine().fdc).expect("restored controller"),
        controller_checkpoint,
        "complete controller including live disk retained"
    );
    assert_eq!(
        restored.machine().fdc.disk_for_export(0).is_err(),
        writing,
        "restored active write export remains rejected"
    );
    let finish = |runtime: &mut SpectrumPlus3Runtime| {
        match boundary {
            TransferBoundary::Configured => send_fdc_command(runtime, &command),
            TransferBoundary::CommandPartial => {
                send_fdc_command(runtime, &command[command.len() - 1..])
            }
            _ => {}
        }
        let drained = if let TransferBoundary::Result(drained) = boundary {
            usize::from(drained)
        } else {
            transfer_consume(runtime, opcode, nd, prefix, total);
            transfer_result_ready(runtime);
            0
        };
        let result = transfer_read_result(runtime, 7 - drained);
        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
        assert!(
            !runtime.machine().fdc.interrupt,
            "completion fully acknowledged"
        );
        // A following command and a changed Specify must use the opposite mode.
        send_fdc_command(runtime, &[4, 0]);
        let _ = runtime.machine_mut().fdc.read_data();
        send_fdc_command(runtime, &[3, 0xdf, 2 | u8::from(!nd)]);
        send_fdc_command(runtime, &transfer_command(0x46));
        transfer_request(runtime);
        assert_eq!(
            runtime.machine().fdc.read_status() & 0x20 != 0,
            !nd,
            "next programmed mode"
        );
        assert_eq!(runtime.machine().fdc.interrupt, !nd, "next request IRQ");
        assert_eq!(
            runtime.machine_mut().fdc.read_data(),
            0xe5,
            "next command payload"
        );
        assert!(
            !runtime.machine().fdc.interrupt,
            "next payload acknowledges IRQ"
        );
        result
    };
    let baseline = finish(&mut original);
    let observed = finish(&mut restored);
    let mut expected = baseline.clone();
    if control == Some("wrong-result") {
        expected[0] ^= 16;
    }
    assert_eq!(observed, expected, "native transfer expected result");
    for runtime in [&original, &restored] {
        assert_eq!(
            runtime.disk_images()[0].bytes,
            source,
            "immutable source bytes"
        );
        let disk = runtime
            .machine()
            .fdc
            .disk_for_export(0)
            .expect("finished write/live read");
        assert_eq!(
            disk.tracks[0][0].sectors[0].data,
            vec![0xe5; 512],
            "known final payload"
        );
        assert_eq!(
            disk.tracks[0][0].sectors[0].st2,
            if opcode == 0x49 { 0x40 } else { 0 },
            "known final data mark"
        );
    }
    assert_eq!(
        postcard::to_allocvec(
            original
                .machine()
                .fdc
                .disk_for_export(0)
                .expect("baseline disk")
        )
        .expect("baseline disk identity"),
        postcard::to_allocvec(
            restored
                .machine()
                .fdc
                .disk_for_export(0)
                .expect("continued disk")
        )
        .expect("continued disk identity"),
        "final live disks agree"
    );
    assert!(
        original.snapshot().expect("baseline state")
            == restored.snapshot().expect("continued state"),
        "final complete machine states agree"
    );
    serde_json::json!({"opcode":opcode,"non_dma":nd,"boundary":format!("{boundary:?}"),
        "checkpoint_msr":boundary.msr(opcode,nd),"checkpoint_irq":boundary.irq(nd),
        "checkpoint_bytes":saved.len(),"payload_prefix":prefix,"result_prefix":before,
        "continuation_original":baseline,"continuation_restored":observed,
        "roundtrip_equal":true,"controller_checkpoint_equal":true,"final_state_equal":true,"source_unchanged":true})
}

#[test]
fn transfer_native_preserves_programmed_mode_requests_and_results() {
    let control = std::env::var("TRANSFER_NATIVE_CONTROL").ok();
    assert!(
        matches!(
            control.as_deref(),
            None | Some("idle-checkpoint" | "wrong-mode" | "wrong-result")
        ),
        "known transfer control"
    );
    if let Some(control) = control.as_deref() {
        let _ = transfer_native_case(0x46, true, TransferBoundary::Preamble, Some(control));
        panic!("deliberate native transfer control was not rejected");
    }
    let mut boundaries = vec![
        TransferBoundary::Configured,
        TransferBoundary::CommandPartial,
        TransferBoundary::Preamble,
        TransferBoundary::FirstRequest,
        TransferBoundary::FirstAccepted,
        TransferBoundary::InteriorRequest,
        TransferBoundary::FinalRequest,
    ];
    boundaries.extend((0..=7).map(TransferBoundary::Result));
    let mut cases = Vec::new();
    for opcode in [0x46, 0x4c, 0x42, 0x45, 0x49, 0x51, 0x59, 0x5d, 0x4d] {
        for nd in [false, true] {
            for &boundary in &boundaries {
                cases.push(transfer_native_case(opcode, nd, boundary, None));
            }
        }
    }
    assert_eq!(
        cases.len(),
        270,
        "all nine engines, both modes and fifteen boundaries"
    );
    native_field_artifact("transfer-native-boundaries.json", &serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version":1,"qualified":false,"snapshot_version":SNAPSHOT_VERSION,
        "scope":"programmed ND, IRQ and native continuation on synthetic media; no DMA handshake or ROM-load claim",
        "case_count":cases.len(),"cases":cases
    })).expect("transfer native JSON"));
}
