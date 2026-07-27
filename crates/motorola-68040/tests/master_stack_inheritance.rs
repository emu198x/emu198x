//! MC68020 master-stack inheritance regressions for `Cpu68040`.

use motorola_68040::Cpu68040;

const ISP: u32 = 0x8000;
const MSP: u32 = 0x9000;

fn assert_master_stack_is_selected(cpu: &Cpu68040) {
    assert!(
        cpu.regs.master_stack_capable(),
        "the 68040 must inherit MC68020 dual supervisor stacks"
    );
    assert_eq!(
        cpu.regs.active_sp(),
        MSP,
        "S=1 M=1 must select MSP rather than ISP"
    );
}

#[test]
fn master_stack_selection_survives_construction_and_serde_restore() {
    let mut cpu = Cpu68040::new();
    cpu.regs.ssp = ISP;
    cpu.regs.msp = MSP;
    cpu.regs.sr = 0x3000;
    assert_master_stack_is_selected(&cpu);

    let encoded = rmp_serde::to_vec_named(&cpu).expect("serialize MC68040");
    let mut restored: Cpu68040 = rmp_serde::from_slice(&encoded).expect("deserialize MC68040");
    assert_master_stack_is_selected(&restored);

    restored.regs.set_active_sp(MSP - 4);
    assert_eq!(restored.regs.msp, MSP - 4);
    assert_eq!(restored.regs.ssp, ISP);
}
