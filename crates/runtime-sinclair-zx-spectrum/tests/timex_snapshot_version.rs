//! Each profile keeps its own save version: the Timex's v18, and v19 for the
//! rest, which added the ULA's snow state.
use emu198x_shell::{MachineCore, MachineError};
use runtime_sinclair_zx_spectrum::*;
fn check<M: MachineCore>(mut r: M, expected: u32) {
    let original = r.snapshot().expect("current state");
    let (version, _) = postcard::take_from_bytes::<u32>(&original).expect("header");
    assert_eq!(version, expected, "{}", r.profile().profile_id.as_str());
    r.restore(&original).expect("matching current snapshot");
    assert_eq!(r.snapshot().expect("round trip"), original);
    // The previous Spectrum version, and the other family's current one.
    for rejected in [17, if expected == 18 { 19 } else { 18 }] {
        // A version-only byte must reject before any payload can be decoded.
        assert!(
            matches!(r.restore(&[rejected]),Err(MachineError::InvalidSnapshot{reason})
  if reason==format!("unsupported snapshot version {rejected}; expected {expected}"))
        );
        assert_eq!(r.snapshot().expect("atomic rejection"), original);
    }
}
#[test]
fn every_current_profile_keeps_its_selected_save_version() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            check(TimexTC2048Runtime::blank(), 18);
            check(TimexTC2068Runtime::blank(), 18);
            check(TimexTS2068Runtime::blank(), 18);
            check(Spectrum16kRuntime::blank(), 19);
            check(Spectrum48kRuntime::blank(), 19);
            check(Spectrum48kIssue1Runtime::blank(), 19);
            check(Spectrum48kIssue2Runtime::blank(), 19);
            check(SpectrumPlusRuntime::blank(), 19);
            check(Spectrum128kRuntime::blank(), 19);
            check(SpectrumPlus2Runtime::blank(), 19);
            check(SpectrumPlus2ARuntime::blank(), 19);
            check(SpectrumPlus2BRuntime::blank(), 19);
            check(SpectrumPlus3Runtime::blank(), 19);
            check(Pentagon128Runtime::blank(), 19);
            check(ScorpionZS256Runtime::blank(), 19);
        })
        .expect("test worker")
        .join()
        .expect("version matrix");
}
