//! Fixed Sony HB-10 board preserving the currently implemented base hardware.
pub use common_msx::{
    DateTime, FM_PAC_SRAM_SIZE, IoEvent, MapperType, MemoryMapper, MsxHardware, MsxLayout,
    MsxRegion, RamRegion, Rp5c01, Scc, SlotAddress, V9938, V9938Mode, Ym2413, detect_mapper,
};
use common_msx::{MsxBoard, MsxCore};
/// This board's fixed wiring and topology.
pub struct Board;
impl MsxBoard for Board {
    const HARDWARE: MsxHardware = MsxHardware::SonyHb10;
    const REGION: MsxRegion = MsxRegion::Ntsc;
    const IS_MSX2: bool = false;
    fn layout() -> MsxLayout {
        MsxLayout {
            ram: vec![RamRegion {
                slot: SlotAddress::primary(0),
                start: 0xC000,
                end: 0xFFFF,
            }],
            ..MsxLayout::default()
        }
    }
}
/// This fixed physical machine.
pub type Msx = MsxCore<Board>;
