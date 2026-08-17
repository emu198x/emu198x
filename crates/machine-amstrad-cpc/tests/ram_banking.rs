//! The 6128's PAL RAM configurations.
//!
//! A 464 has 64 KB and no PAL; a 6128 has 128 KB and eight configurations
//! selected by a `%11xxxxxx` write to the Gate Array's port. The table is in
//! `reference/by-system/amstrad-cpc/cpc-reference.md` §3 and matches
//! Caprice32's `ga_init_banking`.
//!
//! These run without firmware: `with_model` only needs 32 KB of *something*,
//! and nothing here executes.

use machine_amstrad_cpc::{AmstradCpc, CpcModel};

/// Blank firmware. Nothing here runs code.
fn stub() -> Vec<u8> {
    vec![0; 0x8000]
}

fn cpc(model: CpcModel) -> AmstradCpc {
    let mut m = AmstradCpc::with_model(&stub(), model).expect("32 KB stub");
    // Page both ROMs out, so `peek` reads RAM across the whole address space.
    // Gate Array function `10`: bit 2 disables the lower ROM, bit 3 the upper.
    m.out(0x7F00, 0b1000_1100);
    m
}

/// Select a RAM configuration the way a program does.
fn select(cpc: &mut AmstradCpc, config: u8) {
    // A15 = 0, A14 = 1 reaches the Gate Array's port; the PAL shares it.
    cpc.out(0x7F00, 0xC0 | config);
}

/// Which bank each of the four blocks reads, per configuration. Banks 0-3 are
/// the base 64 KB, 4-7 the second.
const EXPECTED: [[usize; 4]; 8] = [
    [0, 1, 2, 3],
    [0, 1, 2, 7],
    [4, 5, 6, 7],
    [0, 3, 2, 7],
    [0, 4, 2, 3],
    [0, 5, 2, 3],
    [0, 6, 2, 3],
    [0, 7, 2, 3],
];

/// Every configuration maps every block to the bank the PAL says it should.
///
/// Each of the eight physical banks is stamped with its own number first, so a
/// read through the Z80's address space names the bank it landed in.
#[test]
fn every_configuration_maps_its_four_blocks() {
    let mut m = cpc(CpcModel::Cpc6128);
    for bank in 0..8usize {
        // Reach each bank to stamp it: configuration 2 exposes banks 4-7 and
        // configuration 0 exposes 0-3, so between them every bank is writable.
        let (config, block) = if bank < 4 { (0, bank) } else { (2, bank - 4) };
        select(&mut m, config);
        let addr = u16::try_from(block * 0x4000).expect("block base");
        m.poke(addr, u8::try_from(bank).expect("0..8"));
    }

    for (config, blocks) in EXPECTED.iter().enumerate() {
        select(&mut m, u8::try_from(config).expect("0..8"));
        for (block, &want) in blocks.iter().enumerate() {
            let addr = u16::try_from(block * 0x4000).expect("block base");
            assert_eq!(
                u32::from(m.peek(addr)),
                u32::try_from(want).expect("bank"),
                "configuration {config}, block {block} should read bank {want}"
            );
        }
    }
}

/// The identity configuration is what a 6128 powers up in, so a program that
/// never banks sees exactly the 464's memory.
#[test]
fn configuration_zero_is_the_identity() {
    let mut m = cpc(CpcModel::Cpc6128);
    assert_eq!(m.ram_config(), 0, "a 6128 powers up unbanked");
    for block in 0..4u16 {
        m.poke(block * 0x4000, 0xA0 + u8::try_from(block).expect("0..4"));
    }
    for block in 0..4u16 {
        assert_eq!(
            m.ram_byte_at(usize::from(block) * 0x4000),
            0xA0 + u8::try_from(block).expect("0..4"),
            "block {block} should sit in its own bank"
        );
    }
}

/// A 464 has no PAL. The write reaches the Gate Array, which ignores it, and
/// nothing else catches it — so `$4000` stays main RAM.
///
/// This is the whole of why SHAKER cannot report its interrupt measurements on
/// a 464: it banks in expansion RAM to save the screen, and on this machine the
/// save lands on its own data. See #968.
#[test]
fn a_464_ignores_a_configuration_write() {
    let mut m = cpc(CpcModel::Cpc464);
    m.poke(0x4000, 0x5A);
    select(&mut m, 4);
    assert_eq!(m.ram_config(), 0, "a 464 has no PAL to select a bank");
    assert_eq!(
        m.peek(0x4000),
        0x5A,
        "`$4000` should still be main RAM after a configuration write"
    );
}

/// Banking moves the Z80's view, not the display. The CRTC drives RAM directly
/// rather than through the PAL, so the screen always shows the base 64 KB —
/// which is why `ram_byte` deliberately ignores banking.
#[test]
fn the_display_always_reads_the_base_ram() {
    let mut m = cpc(CpcModel::Cpc6128);
    m.poke(0x4000, 0x11);
    select(&mut m, 4);
    m.poke(0x4000, 0x22);
    assert_eq!(m.peek(0x4000), 0x22, "the Z80 sees the banked-in RAM");
    assert_eq!(
        m.ram_byte(0x4000),
        0x11,
        "the display should still see base RAM"
    );
}

/// A 6128 fits twice the RAM, and the second half is only reachable banked.
#[test]
fn a_6128_has_twice_the_ram() {
    let mut m = cpc(CpcModel::Cpc6128);
    select(&mut m, 4);
    m.poke(0x4000, 0x7E);
    // Bank 4 starts at physical `$10000` — past everything a 464 has.
    assert_eq!(m.ram_byte_at(0x1_0000), 0x7E);
}
