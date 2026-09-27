//! Original Acid800 ANTIC probes. See tests/data/acid800-antic.md.
#[path = "common/acid800.rs"]
mod acid800;
use machine_atari_800xl::Atari800xlRegion;

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_antic_probes_ntsc() {
    acid800::survey(
        Atari800xlRegion::Ntsc,
        "ntsc",
        "antic",
        include_str!("data/acid800-antic.json"),
        20,
    );
}

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_antic_probes_pal() {
    acid800::survey(
        Atari800xlRegion::Pal,
        "pal",
        "antic",
        include_str!("data/acid800-antic.json"),
        20,
    );
}
