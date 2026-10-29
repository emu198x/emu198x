//! Concrete board boundaries must reject a state from the other physical board.
use common_commodore_vic_20::Vic20State;
use emu198x_shell::MachineCore;
use machine_commodore_vic_20_ntsc::Vic20 as Ntsc;
use machine_commodore_vic_20_pal::Vic20 as Pal;
use runtime_commodore_vic_20::{Model, Vic20Runtime};

type LegacyEnvelope = (u16, u64, String, Option<Vic20State>, Option<Vec<u8>>);

#[test]
fn each_board_accepts_its_legacy_payload_and_rejects_the_other() {
    for (model, bytes) in [
        (
            Model::Vic20Ntsc,
            include_bytes!(
                "../../../test-data/commodore/vic-20/split-v8/commodore-vic-20-ntsc.state"
            )
            .as_slice(),
        ),
        (
            Model::Vic20Pal,
            include_bytes!(
                "../../../test-data/commodore/vic-20/split-v8/commodore-vic-20-pal.state"
            )
            .as_slice(),
        ),
    ] {
        let (version, _, _, state, cartridge): LegacyEnvelope =
            postcard::from_bytes(bytes).expect("legacy envelope");
        assert_eq!(version, 8);
        assert_eq!(cartridge.expect("original cart"), vec![0x5A; 8192]);
        let encoded = postcard::to_allocvec(&state.expect("live legacy machine")).expect("state");
        match model {
            Model::Vic20Ntsc => {
                let board: Ntsc = postcard::from_bytes(&encoded).expect("NTSC legacy board");
                assert_eq!(board.framebuffer_height(), 240);
                assert!(postcard::from_bytes::<Pal>(&encoded).is_err());
            }
            Model::Vic20Pal => {
                let board: Pal = postcard::from_bytes(&encoded).expect("PAL legacy board");
                assert_eq!(board.framebuffer_height(), 288);
                assert!(postcard::from_bytes::<Ntsc>(&encoded).is_err());
            }
        }
        // A correct payload under a false profile label must fail atomically.
        let mut envelope: LegacyEnvelope = postcard::from_bytes(bytes).expect("legacy envelope");
        let labelled_model = match model {
            Model::Vic20Ntsc => Model::Vic20Pal,
            Model::Vic20Pal => Model::Vic20Ntsc,
        };
        envelope.2 = labelled_model.model_id().into();
        // The label matches the receiving runtime: only the live board is false.
        // Otherwise the older profile-name guard would reject this first.
        let mut runtime = Vic20Runtime::blank(labelled_model);
        let before = runtime.snapshot().expect("before");
        let error = runtime
            .restore(&postcard::to_allocvec(&envelope).expect("mislabelled state"))
            .expect_err("false board identity must fail");
        assert!(error.to_string().contains("snapshot board does not match"));
        assert_eq!(runtime.snapshot().expect("after"), before);
    }
}
