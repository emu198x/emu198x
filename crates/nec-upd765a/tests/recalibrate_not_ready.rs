use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::Upd765a;

#[test]
fn initially_not_ready_recalibrate_returns_abnormal_seek_end_and_retains_busy() {
    for unit in 0..4 {
        let mut f = Upd765a::new();
        f.enabled = true;
        f.write_data(7);
        f.write_data(unit);
        for hc in 0..4096 {
            f.tick(hc);
        }
        assert!(f.interrupt, "bounded failed recalibrate raises IRQ");
        let completed_msr = f.read_status();
        f.write_data(8);
        assert!(!f.interrupt, "SIS acknowledges IRQ");
        let before_st0_msr = f.read_status();
        assert_eq!(
            f.read_data(),
            0x68 | unit,
            "IC01, NR and SE, not READY attention"
        );
        assert_eq!(
            completed_msr,
            0x80 | (1 << unit),
            "completion stays logically busy"
        );
        assert_eq!(
            before_st0_msr,
            0xd0 | (1 << unit),
            "unread ST0 retains busy"
        );
        assert_eq!(
            f.read_status(),
            0xd0,
            "ST0 clears busy while PCN remains unread"
        );
        assert_eq!(f.read_data(), 0, "initial PCN is preserved");
        assert_eq!(f.read_status(), 0x80);
        f.write_data(8);
        assert_eq!(f.read_data(), 0x80, "completion was drained exactly once");
        assert_eq!(f.read_status(), 0x80);
    }
}
