use common_nintendo_nes::NesSnapshot;
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_nes::{InputDevices, Model, NesRuntime};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u16,
    time: u64,
    cartridge_mapper: Option<u16>,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<NesSnapshot>,
}
fn loaded(model: Model) -> NesRuntime {
    let mut rom = vec![0xea; 16 + 16384];
    rom[..16].fill(0);
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
    let mut r = NesRuntime::blank(model);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    r.load_media(&media).expect("load");
    r
}
#[test]
fn physical_identity_matches_the_selected_profile_and_chip_clock() {
    for model in Model::ALL {
        let r = loaded(model);
        let m = r.machine().expect("loaded");
        assert_eq!(r.profile().profile_id.as_str(), model.model_id());
        assert_eq!(r.profile().machine_id.as_str(), m.board_id());
        assert_eq!(m.board_id(), model.model_id());
        assert_eq!(m.region(), model.machine_region());
        assert_eq!(m.ppu().master_divider(), model.machine_region().dot_units());
        assert_eq!(m.apu().region(), model.machine_region().apu_region());
    }
}
#[test]
fn wrong_region_rejection_keeps_every_live_snapshot_byte() {
    for (source, target) in [
        (Model::NesNtsc, Model::NesPal),
        (Model::NesPal, Model::NesNtsc),
    ] {
        let source = loaded(source).snapshot().expect("foreign");
        let mut target = loaded(target);
        target.set_input_devices(InputDevices::FourScore);
        target.machine_mut().expect("loaded").set_controller3(0xa5);
        target.machine_mut().expect("loaded").poke(0x125, 0x5a);
        let before = target.snapshot().expect("before");
        assert!(target.restore(&source).is_err());
        assert_eq!(target.snapshot().expect("unchanged"), before);
    }
}
#[test]
fn mismatched_chip_geometry_and_clock_phase_are_rejected_atomically() {
    for model in Model::ALL {
        let mut target = loaded(model);
        let before = target.snapshot().expect("before");
        for bad in ["ppu-divider", "ppu-lines", "apu-region", "cpu-phase"] {
            let mut envelope: Envelope = postcard::from_bytes(&before).expect("envelope");
            let mut machine =
                serde_json::to_value(envelope.machine.take().expect("machine")).expect("value");
            match bad {
                "ppu-divider" => machine["ppu"]["master_divider"] = serde_json::json!(99),
                "ppu-lines" => machine["ppu"]["pre_render_line"] = serde_json::json!(777),
                "apu-region" => {
                    machine["apu"]["region"] = serde_json::json!(if model == Model::NesNtsc {
                        "Pal"
                    } else {
                        "Ntsc"
                    })
                }
                "cpu-phase" => machine["cpu_phase"] = serde_json::json!(999),
                _ => unreachable!(),
            }
            envelope.machine =
                Some(serde_json::from_value(machine).expect("valid serialized field types"));
            let bytes = postcard::to_allocvec(&envelope).expect("malformed board");
            assert!(target.restore(&bytes).is_err(), "{bad}");
            assert_eq!(target.snapshot().expect("unchanged"), before, "{bad}");
        }
    }
}
#[test]
fn empty_state_keeps_the_selected_profile_without_inventing_a_board() {
    let blank = NesRuntime::blank(Model::NesNtsc)
        .snapshot()
        .expect("empty save");
    let mut selected = NesRuntime::blank(Model::NesPal);
    selected.restore(&blank).expect("no hardware in save");
    assert_eq!(selected.model(), Model::NesPal);
    assert!(selected.machine().is_none());
    assert_eq!(selected.profile().machine_id.as_str(), "nintendo-nes-pal");
}
