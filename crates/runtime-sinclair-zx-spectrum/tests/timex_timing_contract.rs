//! Metadata, actual native fields and restored SCLD selection must agree.
use common_sinclair_zx_spectrum::{SpectrumDriver, timing::FrameTiming, ula::Ula};
use emu198x_shell::{HeadlessSession, MachineCore};
use runtime_sinclair_zx_spectrum::{
    SpectrumRuntimeKind, TimexTC2048Runtime, TimexTC2068Runtime, TimexTS2068Runtime,
};
fn timing(t: &FrameTiming) -> (u64, u32, u32) {
    (t.master_hz, t.halfcycles_per_frame, t.lines_per_frame)
}
fn clocks(runtime: &SpectrumRuntimeKind) -> ((u64, u32, u32), (u64, u32, u32)) {
    match runtime {
        SpectrumRuntimeKind::TimexTC2048(r) => (
            timing(r.machine().frame_timing()),
            timing(r.machine().ula.frame_timing()),
        ),
        SpectrumRuntimeKind::TimexTC2068(r) => (
            timing(r.machine().frame_timing()),
            timing(r.machine().ula().frame_timing()),
        ),
        SpectrumRuntimeKind::TimexTS2068(r) => (
            timing(r.machine().frame_timing()),
            timing(r.machine().ula().frame_timing()),
        ),
        _ => panic!("Timex only"),
    }
}
#[test]
fn native_fields_and_restored_metadata_agree_on_all_three_timex_boards() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for runtime in [
                SpectrumRuntimeKind::TimexTC2048(Box::new(TimexTC2048Runtime::blank())),
                SpectrumRuntimeKind::TimexTC2068(Box::new(TimexTC2068Runtime::blank())),
                SpectrumRuntimeKind::TimexTS2068(Box::new(TimexTS2068Runtime::blank())),
            ] {
                let (machine, scld) = clocks(&runtime);
                assert_eq!(machine, scld);
                let mut session = HeadlessSession::new(runtime, 1);
                for field in 1..=3u64 {
                    session.run_frames(1).expect("real native field");
                    assert_eq!(session.machine().time().get(), field * u64::from(machine.1));
                    assert_eq!(
                        session.latest_frame().expect("frame").timestamp.get(),
                        field * u64::from(machine.1)
                    );
                    let saved = session.machine().snapshot().expect("snapshot");
                    session.machine_mut().restore(&saved).expect("restore");
                    assert_eq!(clocks(session.machine()), (machine, machine));
                }
            }
        })
        .expect("test worker")
        .join()
        .expect("clock matrix");
}
