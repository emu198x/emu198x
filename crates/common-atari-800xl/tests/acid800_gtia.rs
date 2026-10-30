//! Original Acid800 GTIA probes. See tests/data/acid800-gtia.md.
#[path = "common/acid800.rs"]
mod acid800;
use common_atari_800xl::Atari800xlRegion;

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_gtia_probes_ntsc() {
    acid800::survey(
        Atari800xlRegion::Ntsc,
        "ntsc",
        "gtia",
        include_str!("data/acid800-gtia.json"),
        11,
    );
}

#[test]
#[ignore = "FIXTURE: hash-pinned Acid800 standalone XEX/symbols and Atari XL OS/BASIC ROMs"]
fn original_gtia_probes_pal() {
    acid800::survey(
        Atari800xlRegion::Pal,
        "pal",
        "gtia",
        include_str!("data/acid800-gtia.json"),
        11,
    );
}
