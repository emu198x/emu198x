// Native continuation of programmed digital motion. Synthetic media only;
// no ROM-loader, firmware or mechanical timing qualification is claimed.
#[test]
fn programmed_motion_and_fractional_boundaries_survive_native_restore() {
    use common_sinclair_zx_spectrum::timing::CPU_HZ_128K;
    for srt in [1u8, 15] {
        let interval = 2_000 * u64::from(16 - srt) * CPU_HZ_128K;
        let one = interval.div_ceil(1_000_000);
        let two = (2 * interval).div_ceil(1_000_000);
        let end = (3 * interval).div_ceil(1_000_000);
        for before in [0, one - 1, one, two - 1, two, end - 1, end] {
            for kind in ["seek", "recalibrate"] {
                let (mut original, source) = writable_sector_runtime(false);
                let live_before = postcard::to_allocvec(
                    original
                        .machine()
                        .fdc
                        .disk_for_export(0)
                        .expect("mounted disk"),
                )
                .expect("live identity");
                for unit in 0..4 {
                    original.machine_mut().fdc.set_ready(unit, false);
                }
                original.machine_mut().fdc.set_ready(0, true);
                for _ in 0..100_000 {
                    original.machine_mut().fdc.tick();
                }
                send_fdc_command(&mut original, &[8]);
                assert_eq!(
                    [
                        original.machine_mut().fdc.read_data(),
                        original.machine_mut().fdc.read_data()
                    ],
                    [0xc0, 0]
                );
                send_fdc_command(&mut original, &[3, srt << 4, 3]);
                // Move outward using the same programmed rate before testing
                // a three-step recalibration. T0 is an independent input.
                if kind == "recalibrate" {
                    send_fdc_command(&mut original, &[15, 0, 3]);
                    for _ in 0..end {
                        original.machine_mut().fdc.tick();
                    }
                    send_fdc_command(&mut original, &[8]);
                    assert_eq!(
                        [
                            original.machine_mut().fdc.read_data(),
                            original.machine_mut().fdc.read_data()
                        ],
                        [0x20, 3]
                    );
                    original.machine_mut().fdc.set_track_zero(0, false);
                    send_fdc_command(&mut original, &[7, 0]);
                } else {
                    send_fdc_command(&mut original, &[15, 0, 3]);
                }
                for _ in 1..=before {
                    if kind == "recalibrate" {
                        let t0 = original.machine().fdc.head_at_track_zero(0);
                        original.machine_mut().fdc.set_track_zero(0, t0);
                    }
                    original.machine_mut().fdc.tick();
                }
                let saved = original.snapshot().expect("motion checkpoint");
                let mut restored = SpectrumPlus3Runtime::blank();
                restored.restore(&saved).expect("motion restore");
                assert_eq!(restored.snapshot().expect("native fixed point"), saved);
                for runtime in [&mut original, &mut restored] {
                    for _ in before + 1..=end {
                        if kind == "recalibrate" {
                            let t0 = runtime.machine().fdc.head_at_track_zero(0);
                            runtime.machine_mut().fdc.set_track_zero(0, t0);
                        }
                        runtime.machine_mut().fdc.tick();
                    }
                    assert!(runtime.machine().fdc.interrupt, "completion still occurs");
                    assert_eq!(runtime.machine().fdc.read_status(), 0x81);
                    send_fdc_command(runtime, &[8]);
                    assert_eq!(runtime.machine_mut().fdc.read_data(), 0x20);
                    assert_eq!(
                        runtime.machine_mut().fdc.read_data(),
                        if kind == "seek" { 3 } else { 0 }
                    );
                    assert_eq!(runtime.machine().fdc.read_status(), 0x80);
                    assert_eq!(
                        runtime.disk_images()[0].bytes,
                        source,
                        "original media identity"
                    );
                    assert_eq!(
                        postcard::to_allocvec(
                            runtime.machine().fdc.disk_for_export(0).expect("live disk")
                        )
                        .expect("live identity"),
                        live_before,
                        "motion leaves live media unchanged"
                    );
                }
                assert_eq!(
                    original.snapshot().expect("baseline continuation"),
                    restored.snapshot().expect("restored continuation")
                );
            }
        }
    }
}

#[test]
fn version_ten_without_motion_state_is_rejected_before_payload_decode() {
    let mut runtime = SpectrumPlus3Runtime::blank();
    let bytes = postcard::to_allocvec(&10_u32).expect("old version prefix");
    assert!(
        matches!(runtime.restore(&bytes),Err(MachineError::InvalidSnapshot {reason}) if reason=="unsupported snapshot version 10; expected 15")
    );
}

#[test]
fn partial_motion_and_specify_commands_preserve_parameters_across_restore() {
    for command in [&[3, 0xf0, 3][..], &[15, 0, 3][..], &[7, 0][..]] {
        for prefix in 0..command.len() {
            let mut original = SpectrumPlus3Runtime::blank();
            original.machine_mut().fdc.set_ready(0, true);
            for _ in 0..100_000 {
                original.machine_mut().fdc.tick();
            }
            send_fdc_command(&mut original, &[8]);
            assert_eq!(
                [
                    original.machine_mut().fdc.read_data(),
                    original.machine_mut().fdc.read_data()
                ],
                [0xc0, 0]
            );
            send_fdc_command(&mut original, &[3, 0xf0, 3]);
            send_fdc_command(&mut original, &command[..prefix]);
            let checkpoint = original.snapshot().expect("partial command checkpoint");
            let mut restored = SpectrumPlus3Runtime::blank();
            restored
                .restore(&checkpoint)
                .expect("partial command restore");
            for runtime in [&mut original, &mut restored] {
                send_fdc_command(runtime, &command[prefix..]);
                for _ in 0..25_000 {
                    runtime.machine_mut().fdc.tick();
                }
            }
            assert_eq!(
                original.snapshot().expect("uninterrupted"),
                restored.snapshot().expect("resumed")
            );
        }
    }
}
