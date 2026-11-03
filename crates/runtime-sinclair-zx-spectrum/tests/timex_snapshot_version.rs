//! Physical boards use v20; Timex retains v18 and the other profiles retain v19.
use emu198x_shell::{MachineCore, MachineError};
use runtime_sinclair_zx_spectrum::*;
fn check<M: MachineCore>(mut r: M, expected: u32) -> String {
    let original = r.snapshot().expect("current state");
    let (version, _) = postcard::take_from_bytes::<u32>(&original).expect("header");
    assert_eq!(version, expected, "{}", r.profile().profile_id.as_str());
    r.restore(&original).expect("matching current snapshot");
    assert_eq!(r.snapshot().expect("round trip"), original);
    for rejected in [17, 18, 19, 20]
        .into_iter()
        .filter(|version| *version != expected)
    {
        // A version-only prefix must reject before any payload can be decoded.
        let prefix = postcard::to_allocvec(&rejected).expect("version prefix");
        assert!(
            matches!(r.restore(&prefix), Err(MachineError::InvalidSnapshot { reason })
            if reason == format!("unsupported snapshot version {rejected}; expected {expected}"))
        );
        assert_eq!(r.snapshot().expect("atomic rejection"), original);
    }
    r.profile().profile_id.as_str().to_owned()
}
#[test]
fn every_current_profile_keeps_its_selected_save_version() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut checked = vec![
                check(SpectrumIssue1Ram16kRuntime::blank(), 20),
                check(SpectrumIssue1Ram48kRuntime::blank(), 20),
                check(SpectrumIssue2Ram16kRuntime::blank(), 20),
                check(SpectrumIssue2Ram48kRuntime::blank(), 20),
                check(SpectrumIssue3Ram16kRuntime::blank(), 20),
                check(SpectrumIssue3Ram48kRuntime::blank(), 20),
                check(SpectrumIssue3BRam16kRuntime::blank(), 20),
                check(SpectrumIssue3BRam48kRuntime::blank(), 20),
                check(SpectrumIssue4ARam16kRuntime::blank(), 20),
                check(SpectrumIssue4ARam48kRuntime::blank(), 20),
                check(SpectrumIssue4BRam16kRuntime::blank(), 20),
                check(SpectrumIssue4BRam48kRuntime::blank(), 20),
                check(SpectrumIssue5Ula6c001e6Ram16kRuntime::blank(), 20),
                check(SpectrumIssue5Ula6c001e6Ram48kRuntime::blank(), 20),
                check(SpectrumIssue5Ula6c001e7Ram16kRuntime::blank(), 20),
                check(SpectrumIssue5Ula6c001e7Ram48kRuntime::blank(), 20),
                check(SpectrumIssue6ARam16kRuntime::blank(), 20),
                check(SpectrumIssue6ARam48kRuntime::blank(), 20),
                check(TimexTC2048Runtime::blank(), 18),
                check(TimexTC2068Runtime::blank(), 18),
                check(TimexTS2068Runtime::blank(), 18),
                check(Spectrum128kRuntime::blank(), 19),
                check(SpectrumPlus2Runtime::blank(), 19),
                check(SpectrumPlus2ARuntime::blank(), 19),
                check(SpectrumPlus2BRuntime::blank(), 19),
                check(SpectrumPlus3Runtime::blank(), 19),
                check(Pentagon128Runtime::blank(), 19),
                check(ScorpionZS256Runtime::blank(), 19),
            ];
            checked.sort();
            let mut expected: Vec<_> = Model::ALL
                .into_iter()
                .map(|m| m.profile_id().to_owned())
                .collect();
            expected.sort();
            assert_eq!(checked.len(), 28);
            assert_eq!(
                checked, expected,
                "every current typed runtime is exercised once"
            );
        })
        .expect("test worker")
        .join()
        .expect("version matrix");
}
