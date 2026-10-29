//! Concrete board boundaries must reject a state from the other physical board.
use common_coleco_colecovision::CvState;
use emu198x_shell::MachineCore;
use machine_coleco_colecovision_ntsc::ColecoVision as Ntsc;
use machine_coleco_colecovision_pal::ColecoVision as Pal;
use runtime_coleco_colecovision::{CvRuntime, Model};

type LegacyEnvelope = (u16, u64, String, Option<CvState>);

#[test]
fn each_board_accepts_its_legacy_payload_and_rejects_the_other() {
    for (model, bytes) in [
        (
            Model::CvNtsc,
            include_bytes!(
                "../../../test-data/colecovision/split-v5/coleco-colecovision-ntsc.state"
            )
            .as_slice(),
        ),
        (
            Model::CvPal,
            include_bytes!(
                "../../../test-data/colecovision/split-v5/coleco-colecovision-pal.state"
            )
            .as_slice(),
        ),
    ] {
        let (version, _, _, state): LegacyEnvelope =
            postcard::from_bytes(bytes).expect("legacy envelope");
        assert_eq!(version, 5);
        let encoded = postcard::to_allocvec(&state.expect("live legacy machine")).expect("state");
        match model {
            Model::CvNtsc => {
                let board: Ntsc = postcard::from_bytes(&encoded).expect("NTSC legacy board");
                assert_eq!(board.framebuffer_height(), 240);
                assert!(postcard::from_bytes::<Pal>(&encoded).is_err());
            }
            Model::CvPal => {
                let board: Pal = postcard::from_bytes(&encoded).expect("PAL legacy board");
                assert_eq!(board.framebuffer_height(), 288);
                assert!(postcard::from_bytes::<Ntsc>(&encoded).is_err());
            }
        }
        // A correct payload under a false profile label must fail atomically.
        let mut envelope: LegacyEnvelope = postcard::from_bytes(bytes).expect("legacy envelope");
        let labelled_model = match model {
            Model::CvNtsc => Model::CvPal,
            Model::CvPal => Model::CvNtsc,
        };
        envelope.2 = labelled_model.model_id().into();
        // The label matches the receiving runtime: only the live board is false.
        // Otherwise the older profile-name guard would reject this first.
        let mut runtime = CvRuntime::blank(labelled_model);
        let before = runtime.snapshot().expect("before");
        let error = runtime
            .restore(&postcard::to_allocvec(&envelope).expect("mislabelled state"))
            .expect_err("false board identity must fail");
        assert!(error.to_string().contains("snapshot board does not match"));
        assert_eq!(runtime.snapshot().expect("after"), before);
    }
}
