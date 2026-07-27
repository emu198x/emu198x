//! A1200 runtime coverage for an in-flight 68020 interrupt acknowledge.
//!
//! The CPU crates prove the inherited MC68010+ continuation in isolation.
//! This test proves that the composed A1200 machine supplies its autovector
//! response and that the runtime envelope preserves the nested `Cpu68020`
//! at the post-acknowledge, pre-frame boundary.

use std::error::Error;

use emu198x_shell::MachineCore;
use motorola_68000::cpu::{State, TAG_EXC_IACK_COMPLETE};
use motorola_68000::microcode::MicroOp;
use runtime_commodore_amiga::{AmigaA1200Runtime, Model};

const INITIAL_SSP: u32 = 0x0018_0000;
const INITIAL_MSP: u32 = 0x0017_0000;
const LEVEL_3_AUTOVECTOR: u8 = 27;
const LEVEL_3_FORMAT_VECTOR: u16 = 0x006C;

fn interrupt_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 512 * 1024];
    kickstart[0..4].copy_from_slice(&INITIAL_SSP.to_be_bytes());
    kickstart[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    kickstart[8..10].copy_from_slice(&0x46FCu16.to_be_bytes()); // MOVE.W #$2000,SR
    kickstart[10..12].copy_from_slice(&0x2000u16.to_be_bytes());
    kickstart[12..14].copy_from_slice(&0x60FEu16.to_be_bytes()); // BRA.S *
    kickstart[0x20..0x22].copy_from_slice(&0x60FEu16.to_be_bytes()); // handler: BRA.S *

    let vector_offset = usize::from(LEVEL_3_AUTOVECTOR) * 4;
    kickstart[vector_offset..vector_offset + 4].copy_from_slice(&0x00F8_0020u32.to_be_bytes());
    kickstart
}

fn master_stack_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 512 * 1024];
    kickstart[0..4].copy_from_slice(&INITIAL_SSP.to_be_bytes());
    kickstart[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    kickstart[8..10].copy_from_slice(&0x203Cu16.to_be_bytes()); // MOVE.L #MSP,D0
    kickstart[10..14].copy_from_slice(&INITIAL_MSP.to_be_bytes());
    kickstart[14..16].copy_from_slice(&0x4E7Bu16.to_be_bytes()); // MOVEC D0,MSP
    kickstart[16..18].copy_from_slice(&0x0803u16.to_be_bytes());
    kickstart[18..20].copy_from_slice(&0x46FCu16.to_be_bytes()); // MOVE.W #$3000,SR
    kickstart[20..22].copy_from_slice(&0x3000u16.to_be_bytes());
    kickstart[22..24].copy_from_slice(&0x60FEu16.to_be_bytes()); // BRA.S *
    kickstart
}

fn is_post_acknowledge_boundary(runtime: &AmigaA1200Runtime) -> bool {
    let cpu = runtime.machine().cpu();
    matches!(cpu.state, State::Idle)
        && cpu.followup_tag == TAG_EXC_IACK_COMPLETE
        && cpu.micro_ops.front() == Some(MicroOp::Execute)
        && cpu.data == u32::from(LEVEL_3_AUTOVECTOR)
        && cpu.exc_vector.is_none()
}

#[test]
fn a1200_post_acknowledge_state_survives_snapshot_and_forward_run() -> Result<(), Box<dyn Error>> {
    let kickstart = interrupt_kickstart();
    let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart.clone())?;
    original.machine_mut().poke_word(0x00DF_F09A, 0xC040); // INTEN | BLIT
    original.machine_mut().poke_word(0x00DF_F09C, 0x8040); // request BLIT

    for _ in 0..20_000 {
        original.machine_mut().tick();
        if is_post_acknowledge_boundary(&original) {
            break;
        }
    }
    assert!(
        is_post_acknowledge_boundary(&original),
        "A1200 did not reach the retained level-3 autovector boundary",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart)?;
    restored.restore(&snapshot)?;
    assert!(
        is_post_acknowledge_boundary(&restored),
        "restored Cpu68020 lost the retained acknowledge continuation",
    );
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the A1200 post-acknowledge boundary must round-trip byte-identically",
    );

    for _ in 0..128 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "the restored A1200 must finish the interrupt frame deterministically",
    );
    assert_eq!(original.machine().cpu().regs.ssp, INITIAL_SSP - 8);
    assert_eq!(
        u16::from_be_bytes([
            original.machine().read_chip_ram_byte(INITIAL_SSP - 2),
            original.machine().read_chip_ram_byte(INITIAL_SSP - 1),
        ]),
        LEVEL_3_FORMAT_VECTOR,
        "the A1200 frame must contain the acknowledged autovector offset",
    );
    Ok(())
}

#[test]
fn a1200_restore_reinstalls_master_stack_capability() -> Result<(), Box<dyn Error>> {
    let kickstart = master_stack_kickstart();
    let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart.clone())?;

    let mut master_mode_ready = false;
    for _ in 0..20_000 {
        original.machine_mut().tick();
        let regs = &original.machine().cpu().regs;
        if original.machine().cpu().ir == 0x60FE && regs.sr == 0x3000 && regs.msp == INITIAL_MSP {
            master_mode_ready = true;
            break;
        }
    }
    assert!(
        master_mode_ready,
        "the synthetic Kickstart must enter a stable S=1/M=1 loop",
    );

    let original_regs = &original.machine().cpu().regs;
    assert!(
        original_regs.master_stack_capable(),
        "the composed A1200 CPU must expose MC68020 supervisor-stack selection",
    );
    assert_eq!(original_regs.ssp, INITIAL_SSP);
    assert_eq!(original_regs.msp, INITIAL_MSP);
    assert_eq!(
        original_regs.active_sp(),
        INITIAL_MSP,
        "S=1/M=1 must select MSP before the runtime snapshot",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart)?;
    restored.restore(&snapshot)?;

    let restored_regs = &restored.machine().cpu().regs;
    assert!(
        restored_regs.master_stack_capable(),
        "runtime restore must reinstall the nested Cpu68020 capability",
    );
    assert_eq!(restored_regs.sr, 0x3000);
    assert_eq!(restored_regs.ssp, INITIAL_SSP);
    assert_eq!(restored_regs.msp, INITIAL_MSP);
    assert_eq!(
        restored_regs.active_sp(),
        INITIAL_MSP,
        "restored S=1/M=1 must continue to select MSP as A7",
    );
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the composed master-stack state must round-trip byte-identically",
    );
    Ok(())
}
