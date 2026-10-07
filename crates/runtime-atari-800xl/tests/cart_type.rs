//! A caller can name a cartridge's banking scheme on every load path.
//!
//! A headerless 16 KB image is a flat cartridge unless something says
//! otherwise (#1401). These use an image the CRC32 table cannot know, so the
//! only thing that can make it an OSS board is the caller.

use emu198x_shell::{
    HeadlessSession, MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind,
    SessionError,
};
use runtime_atari_800xl::{Atari800xlRuntime, CartridgeKind, Model};

/// Four 4 KB banks, each filled with its own number.
fn four_banks() -> Vec<u8> {
    (0..4u8)
        .flat_map(|bank| std::iter::repeat_n(bank, 0x1000))
        .collect()
}

fn session() -> HeadlessSession<Atari800xlRuntime> {
    HeadlessSession::new(
        Atari800xlRuntime::blank(Model::A800xlNtsc),
        Model::A800xlNtsc.frame_ticks(),
    )
}

fn load(
    session: &mut HeadlessSession<Atari800xlRuntime>,
    image: &[u8],
    cart_type: Option<&'static str>,
) -> Result<(), SessionError> {
    let mut cart = MediaImage::new("cartridge-1", MediaKind::Cartridge, image);
    if let Some(name) = cart_type {
        cart = cart.cart_type(name);
    }
    let mut media = MediaSet::new();
    media.push(cart);
    session.load_media(&media)
}

fn kind(runtime: &Atari800xlRuntime) -> Option<CartridgeKind> {
    runtime
        .machine()
        .and_then(|machine| machine.cartridge())
        .map(machine_atari_800xl::Cartridge::kind)
}

fn invalid_media_reason(err: SessionError) -> String {
    match err {
        SessionError::Machine(MachineError::InvalidMedia { slot, reason }) => {
            assert_eq!(slot, "cartridge-1");
            reason
        }
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

#[test]
fn the_runtime_lists_every_cartridge_type_by_name() {
    let runtime = Atari800xlRuntime::blank(Model::A800xlNtsc);
    assert_eq!(
        runtime.cartridge_types(),
        [
            "standard", "oss-m091", "oss-043m", "oss-034m", "oss-8k", "xegs", "mega"
        ]
    );
}

#[test]
fn a_script_or_mcp_cart_type_picks_the_board_and_survives_a_reset() {
    let mut session = session();
    load(&mut session, &four_banks(), None).expect("plain load");
    assert_eq!(kind(session.machine()), Some(CartridgeKind::Standard));

    load(&mut session, &four_banks(), Some("oss-m091")).expect("typed load");
    assert_eq!(kind(session.machine()), Some(CartridgeKind::OssOneChip));
    // M091 keeps bank 0 at $B000; a flat 16 KB image would show bank 3.
    let machine = session.machine().machine().expect("machine");
    assert_eq!(machine.peek(0xB000), 0);

    session.reset(ResetKind::Hard).expect("reset");
    assert_eq!(kind(session.machine()), Some(CartridgeKind::OssOneChip));
}

#[test]
fn an_unknown_cart_type_names_the_choices() {
    let mut session = session();
    let reason =
        invalid_media_reason(load(&mut session, &four_banks(), Some("megarom")).expect_err("no"));
    assert_eq!(
        reason,
        "unknown cartridge type `megarom`; expected standard | oss-m091 | oss-043m | oss-034m \
         | oss-8k | xegs | mega"
    );
    assert!(session.machine().machine().is_none(), "nothing was loaded");
}

#[test]
fn a_cart_type_the_image_cannot_be_is_refused() {
    let mut session = session();
    let reason =
        invalid_media_reason(load(&mut session, &four_banks(), Some("xegs")).expect_err("16 KB"));
    assert!(reason.contains("16384 bytes"), "{reason}");
}

#[test]
fn insert_cartridge_as_overrides_the_size_guess() {
    let mut runtime = Atari800xlRuntime::blank(Model::A800xlNtsc);
    runtime
        .insert_cartridge_as(Some(four_banks()), Some(CartridgeKind::OssTwoChip))
        .expect("043M");
    assert_eq!(kind(&runtime), Some(CartridgeKind::OssTwoChip));
    runtime.insert_cartridge(Some(four_banks())).expect("plain");
    assert_eq!(kind(&runtime), Some(CartridgeKind::Standard));
}
