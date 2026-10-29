//! Legacy board identities and atomic rejection of forged family envelopes.
use common_acorn_bbc_micro::BbcState;
use emu198x_shell::MachineCore;
use machine_acorn_bbc_master_128::BbcMicro as Master;
use machine_acorn_bbc_micro::BbcMicro as ModelB;
use machine_acorn_bbc_micro_b_plus::BbcMicro as BPlus;
use runtime_acorn_bbc_micro::{BbcMicroRuntime, Model};
type Envelope = (u16, u64, String, Option<BbcState>);
fn fixture(model: Model) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../test-data/acorn/bbc-micro/split-v10/{}.state",
            model.model_id()
        )),
    )
    .expect("frozen state")
}
// Separate frames keep repeated large serde results off the caller's stack.
#[inline(never)]
fn payload(model: Model) -> Vec<u8> {
    let (version, _, _, state): Envelope = postcard::from_bytes(&fixture(model)).expect("envelope");
    assert_eq!(version, 10);
    postcard::to_allocvec(&state.expect("machine")).expect("payload")
}
#[inline(never)]
fn decodes<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> bool {
    postcard::from_bytes::<T>(bytes).is_ok()
}
#[test]
fn each_legacy_board_payload_has_one_concrete_owner() {
    for model in Model::ALL {
        let bytes = payload(model);
        assert_eq!(decodes::<ModelB>(&bytes), model == Model::BbcModelB);
        assert_eq!(decodes::<BPlus>(&bytes), model == Model::BbcBPlus);
        assert_eq!(decodes::<Master>(&bytes), model == Model::Master128);
    }
}
#[inline(never)]
fn rejects_false_label(source: Model, target: Model) {
    let mut envelope: Envelope = postcard::from_bytes(&fixture(source)).expect("envelope");
    envelope.2 = target.model_id().into();
    let forged = postcard::to_allocvec(&envelope).expect("forged envelope");
    let mut receiver = BbcMicroRuntime::blank(target);
    let before = receiver.snapshot().expect("before");
    let error = receiver.restore(&forged).expect_err("wrong board");
    assert!(error.to_string().contains("snapshot board does not match"));
    assert_eq!(receiver.snapshot().expect("unchanged"), before);
}
#[test]
fn forged_profile_labels_reject_without_changing_the_receiver() {
    for source in Model::ALL {
        for target in Model::ALL.into_iter().filter(|&target| target != source) {
            rejects_false_label(source, target);
        }
    }
}
