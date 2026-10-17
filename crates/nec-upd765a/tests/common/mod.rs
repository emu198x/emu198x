use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::Upd765a;

pub fn await_read(f: &mut Upd765a) {
    for _ in 0..10_000 {
        if f.read_status() & 0x80 != 0 {
            return;
        }
        assert_eq!(f.read_status() & 0x70, 0x70, "waiting for read byte");
        f.tick(0);
    }
    panic!("read request deadline");
}
