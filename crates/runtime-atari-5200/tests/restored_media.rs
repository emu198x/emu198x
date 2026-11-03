//! The live machine owns media bytes and positive cartridge-decode evidence.
use common_atari_5200::CartLayout;
use emu198x_shell::{MachineCore, ResetKind, SessionQueryProvider};
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider, Model};

fn typed_cart(kind: u32, size: usize) -> Vec<u8> {
    let mut image = vec![0; 16];
    image[..4].copy_from_slice(b"CART");
    image[4..8].copy_from_slice(&kind.to_be_bytes());
    image.extend((0..size).map(|n| (n / 4096) as u8));
    image
}

#[test]
fn restored_media_survives_reset_and_firmware_replacement() {
    for (kind, size, layout) in [
        (6, 16384, CartLayout::TwoChip16K),
        (7, 40960, CartLayout::BountyBob),
        (4, 32768, CartLayout::Linear),
    ] {
        let original =
            Atari5200Runtime::new(Model::Ca018087, typed_cart(kind, size), vec![0xaa; 2048])
                .expect("typed cart");
        let snapshot = original.snapshot().expect("save media");
        let mut restored = Atari5200Runtime::blank(Model::Ca018087);
        restored
            .restore(&snapshot)
            .expect("restore into blank runtime");
        for path in ["bios.loaded", "cartridge.loaded"] {
            let before = Atari5200SessionQueryProvider
                .query(&original, path)
                .expect("query");
            let after = Atari5200SessionQueryProvider
                .query(&restored, path)
                .expect("query");
            assert_eq!(after, before, "restored {path}");
        }
        restored.reset(ResetKind::Hard);
        let machine = restored
            .machine()
            .expect("restored cartridge survives reset");
        assert_eq!(
            machine.cart().layout(),
            layout,
            "positive header evidence survives"
        );
        assert_eq!(machine.peek(0xf800), 0xaa, "saved BIOS survives reset");
        restored
            .set_bios(vec![0xbb; 2048])
            .expect("replace firmware");
        let machine = restored
            .machine()
            .expect("cartridge survives firmware replacement");
        assert_eq!(machine.cart().layout(), layout);
        assert_eq!(machine.peek(0xf800), 0xbb);
        // A replacement cartridge must be detected afresh, not inherit the
        // previous header's two-chip or banked layout.
        restored
            .insert_cartridge(vec![0xea; 8192])
            .expect("replace cartridge");
        assert_eq!(
            restored.machine().expect("new cart").cart().layout(),
            CartLayout::Linear
        );
    }
}
