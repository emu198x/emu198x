//! Execute register writes through a real MC68020 guest bus cycle.
use runtime_commodore_amiga::{AmigaA1200Runtime, Model};

#[test]
fn guest_cpu_write_reaches_lisas_extended_collision_controls() {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    // MOVE.W #0,$DFF098; MOVE.W #$C3,$DFF10E; BRA.S *.
    let program = [
        0x33FCu16, 0, 0x00DF, 0xF098, 0x33FC, 0x00C3, 0x00DF, 0xF10E, 0x60FE,
    ];
    for (index, word) in program.iter().enumerate() {
        rom[8 + index * 2..10 + index * 2].copy_from_slice(&word.to_be_bytes());
    }
    let mut runtime =
        AmigaA1200Runtime::new(Model::A1200AgaPal, rom).expect("construct guest bus fixture");
    for _ in 0..10_000 {
        runtime.machine_mut().tick();
        if runtime
            .machine()
            .denise_aga()
            .as_inner()
            .as_inner()
            .diagnostic_snapshot()
            .clxcon2
            == 0x00C3
        {
            return;
        }
    }
    panic!("the MC68020's CLXCON2 write must reach Lisa through normal bus dispatch");
}

#[test]
fn pending_sprite_and_extended_plane_comparison_resume_together() {
    use commodore_denise_aga::DeniseAga;
    use common_commodore_amiga::DeniseChip;
    let mut original = DeniseAga::new();
    original.set_bplcon0(0x0410); // dual playfield, eight planes
    original.write_word(0x098, 0);
    original.write_word(0x10E, 0x00C3); // BP7=BP8=1 required
    original.write_word(0x104, 0x0024); // sprites in front
    original.begin_beam_line();
    original.write_sprite_pos(0, 0);
    original.write_sprite_ctl(0, 0);
    original.write_sprite_datb(0, 0);
    original.write_sprite_data(0, 0x8000);
    original.load_bitplane(6, 0xFFFF); // BP7 matches; BP8 stays zero
    original.queue_shift_load_from_bpl1dat();
    original.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..1 {
        original.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        original.read_clxdat();
    }
    assert_eq!(
        original.as_inner().as_inner().diagnostic_snapshot().sprites[0].shift_count,
        16
    );
    let bytes = postcard::to_allocvec(&original).expect("save pending collision state");
    let mut restored: DeniseAga =
        postcard::from_bytes(&bytes).expect("restore pending collision state");
    for x in 1..6 {
        let expected = original.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let actual = restored.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(actual, expected);
        let collision = original.read_clxdat();
        assert_eq!(restored.read_clxdat(), collision);
        assert_eq!(
            collision,
            if x == 1 { 1 << 1 } else { 0 },
            "only the visible sprite's odd-group match may latch"
        );
    }
}

#[test]
fn guest_cpu_reads_clxdat_with_bit_fifteen_set() {
    use runtime_commodore_amiga::{AmigaEcsRuntime, AmigaOcsRuntime};
    // With no comparisons enabled, both groups match and bit 0 relatches.
    // Requiring BP1=1 with blank source data prevents a new match; the first
    // read clears any collision from before CLXCON changed. Neither case
    // may clear CLXDAT's fixed bit 15. Registered drawing.cpp::expand_colmask
    // explicitly selects bplalwayson when the enable mask is zero.
    for (clxcon, expected) in [(0, 0x8001), (0x0041, 0x8000)] {
        // MOVE.W #clxcon,$DFF098; MOVE.W $DFF00E,D1;
        // MOVE.W $DFF00E,D0; BRA.S *.
        let words = [
            0x33FCu16, clxcon, 0x00DF, 0xF098, 0x3239, 0x00DF, 0xF00E, 0x3039, 0x00DF, 0xF00E,
            0x60FE,
        ];
        let rom = |size| {
            let mut rom = vec![0; size];
            rom[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
            rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
            for (index, word) in words.iter().enumerate() {
                rom[8 + index * 2..10 + index * 2].copy_from_slice(&word.to_be_bytes());
            }
            rom
        };
        let mut ocs = AmigaOcsRuntime::new(Model::A500OcsPal, rom(256 * 1024)).expect("OCS guest");
        let mut ecs =
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, rom(512 * 1024)).expect("ECS guest");
        let mut aga =
            AmigaA1200Runtime::new(Model::A1200AgaPal, rom(512 * 1024)).expect("AGA guest");
        for _ in 0..2000 {
            ocs.machine_mut().tick();
            ecs.machine_mut().tick();
            aga.machine_mut().tick();
        }
        for (name, registers) in [
            ("OCS", &ocs.machine().cpu().regs.d),
            ("ECS", &ecs.machine().cpu().regs.d),
            ("AGA", &aga.machine().cpu().regs.d),
        ] {
            assert_eq!(
                registers[1] & 0xFFFF,
                0x8001,
                "{name}: the first read retains the earlier match"
            );
            assert_eq!(
                registers[0] & 0xFFFF,
                expected,
                "{name}: CLXCON={clxcon:#06x}"
            );
        }
    }
}

#[test]
fn guest_cpu_write_reaches_bpl8dat_holding_register() {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    // MOVE.W #$8000,$DFF11E; BRA.S *.
    let program = [0x33FCu16, 0x8000, 0x00DF, 0xF11E, 0x60FE];
    for (index, word) in program.iter().enumerate() {
        rom[8 + index * 2..10 + index * 2].copy_from_slice(&word.to_be_bytes());
    }
    let mut runtime = AmigaA1200Runtime::new(Model::A1200AgaPal, rom).expect("AGA guest");
    for _ in 0..2000 {
        runtime.machine_mut().tick();
    }
    assert_eq!(
        runtime
            .machine()
            .denise_aga()
            .as_inner()
            .as_inner()
            .diagnostic_snapshot()
            .bitplanes
            .holding_data[7],
        0x8000
    );
}
