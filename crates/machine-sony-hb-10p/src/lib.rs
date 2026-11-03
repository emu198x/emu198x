//! Fixed Sony HB-10P PU-37 board with S3527 memory and I/O decoding.
pub use common_msx::{
    DateTime, FM_PAC_SRAM_SIZE, IoEvent, MapperType, MemoryMapper, MsxHardware, MsxLayout,
    MsxRegion, RamRegion, Rp5c01, Scc, SlotAddress, V9938, V9938Mode, Ym2413, detect_mapper,
};
use common_msx::{MsxBoard, MsxCore};
/// This board's fixed wiring and topology.
pub struct Board;
impl MsxBoard for Board {
    const HARDWARE: MsxHardware = MsxHardware::SonyHb10p;
    const REGION: MsxRegion = MsxRegion::Pal;
    const IS_MSX2: bool = false;
    const S3527_IO: bool = true;
    fn layout() -> MsxLayout {
        let mut layout = MsxLayout::default();
        // Sony PU-37 schematic 5-12 + Yamaha S3527 pin table, p. 4:
        // CAS3 selects the same RAM through slot-0 page 3.
        layout.ram.push(RamRegion {
            slot: SlotAddress::primary(0),
            start: 0xc000,
            end: 0xffff,
        });
        layout
    }
}
/// This fixed physical machine.
pub type Msx = MsxCore<Board>;
