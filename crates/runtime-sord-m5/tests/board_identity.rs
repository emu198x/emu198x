//! Concrete board boundaries must reject a state from the other physical board.
use common_sord_m5::M5State;
use emu198x_shell::MachineCore;
use machine_sord_m5_ntsc::SordM5 as Ntsc;
use machine_sord_m5_pal::SordM5 as Pal;
use runtime_sord_m5::{M5Runtime, Model};

type LegacyEnvelope = (u16, u64, String, Option<M5State>);

#[test]
fn each_board_accepts_its_legacy_payload_and_rejects_the_other() {
    for (model, bytes) in [
        (
            Model::M5Ntsc,
            include_bytes!("../../../test-data/sord-m5/split-v5/sord-m5-ntsc.state").as_slice(),
        ),
        (
            Model::M5Pal,
            include_bytes!("../../../test-data/sord-m5/split-v5/sord-m5-pal.state").as_slice(),
        ),
    ] {
        let (version, _, _, state): LegacyEnvelope =
            postcard::from_bytes(bytes).expect("legacy envelope");
        assert_eq!(version, 5);
        let encoded = postcard::to_allocvec(&state.expect("live legacy machine")).expect("state");
        match model {
            Model::M5Ntsc => {
                let board: Ntsc = postcard::from_bytes(&encoded).expect("NTSC legacy board");
                assert_eq!(board.framebuffer_height(), 240);
                assert!(postcard::from_bytes::<Pal>(&encoded).is_err());
            }
            Model::M5Pal => {
                let board: Pal = postcard::from_bytes(&encoded).expect("PAL legacy board");
                assert_eq!(board.framebuffer_height(), 288);
                assert!(postcard::from_bytes::<Ntsc>(&encoded).is_err());
            }
        }
        // A correct payload under a false profile label must fail atomically.
        let mut envelope: LegacyEnvelope = postcard::from_bytes(bytes).expect("legacy envelope");
        envelope.2 = match model {
            Model::M5Ntsc => "sord-m5-pal",
            Model::M5Pal => "sord-m5-ntsc",
        }
        .into();
        let mut runtime = M5Runtime::blank(model);
        let before = runtime.snapshot().expect("before");
        assert!(
            runtime
                .restore(&postcard::to_allocvec(&envelope).expect("mislabelled state"))
                .is_err()
        );
        assert_eq!(runtime.snapshot().expect("after"), before);
    }
}
