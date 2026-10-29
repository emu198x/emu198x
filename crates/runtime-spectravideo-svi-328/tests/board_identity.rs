//! Concrete board boundaries must reject a state from the other physical board.
use common_spectravideo_svi_328::SviState;
use emu198x_shell::MachineCore;
use machine_spectravideo_svi_328_ntsc::Svi328 as Ntsc;
use machine_spectravideo_svi_328_pal::Svi328 as Pal;
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

type LegacyEnvelope = (u16, u64, String, Option<SviState>);

#[test]
fn each_board_accepts_its_legacy_payload_and_rejects_the_other() {
    for (model, bytes) in [
        (
            Model::Svi328Ntsc,
            include_bytes!(
                "../../../test-data/spectravideo/split-v7/spectravideo-svi-328-ntsc.state"
            )
            .as_slice(),
        ),
        (
            Model::Svi328Pal,
            include_bytes!(
                "../../../test-data/spectravideo/split-v7/spectravideo-svi-328-pal.state"
            )
            .as_slice(),
        ),
    ] {
        let (version, _, _, state): LegacyEnvelope =
            postcard::from_bytes(bytes).expect("legacy envelope");
        assert_eq!(version, 7);
        let encoded = postcard::to_allocvec(&state.expect("live legacy machine")).expect("state");
        match model {
            Model::Svi328Ntsc => {
                let board: Ntsc = postcard::from_bytes(&encoded).expect("NTSC legacy board");
                assert_eq!(board.vdp().framebuffer_height(), 240);
                assert!(postcard::from_bytes::<Pal>(&encoded).is_err());
            }
            Model::Svi328Pal => {
                let board: Pal = postcard::from_bytes(&encoded).expect("PAL legacy board");
                assert_eq!(board.vdp().framebuffer_height(), 288);
                assert!(postcard::from_bytes::<Ntsc>(&encoded).is_err());
            }
        }
        // A correct payload under a false profile label must fail atomically.
        let mut envelope: LegacyEnvelope = postcard::from_bytes(bytes).expect("legacy envelope");
        let labelled_model = match model {
            Model::Svi328Ntsc => Model::Svi328Pal,
            Model::Svi328Pal => Model::Svi328Ntsc,
        };
        envelope.2 = labelled_model.model_id().into();
        // The label matches the receiving runtime: only the live board is false.
        // Otherwise the older profile-name guard would reject this first.
        let mut runtime = Svi328Runtime::blank(labelled_model);
        let before = runtime.snapshot().expect("before");
        let error = runtime
            .restore(&postcard::to_allocvec(&envelope).expect("mislabelled state"))
            .expect_err("false board identity must fail");
        assert!(error.to_string().contains("snapshot board does not match"));
        assert_eq!(runtime.snapshot().expect("after"), before);
    }
}
