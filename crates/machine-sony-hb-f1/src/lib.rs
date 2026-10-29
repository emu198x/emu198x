//! Fixed Sony HB-F1 board preserving the currently implemented base hardware.
pub use common_msx::{
    DateTime, FM_PAC_SRAM_SIZE, IoEvent, MapperType, MemoryMapper, MsxHardware, MsxLayout,
    MsxRegion, RamRegion, Rp5c01, Scc, SlotAddress, V9938, V9938Mode, Ym2413, detect_mapper,
};
use common_msx::{MsxBoard, MsxCore};
/// This board's fixed wiring and topology.
pub struct Board;
impl MsxBoard for Board {
    const HARDWARE: MsxHardware = MsxHardware::SonyHbF1;
    const REGION: MsxRegion = MsxRegion::Ntsc;
    const IS_MSX2: bool = true;
    fn layout() -> MsxLayout {
        MsxLayout {
            sub_rom: Some(SlotAddress::expanded(3, 0)),
            ram: vec![RamRegion {
                slot: SlotAddress::expanded(3, 3),
                start: 0,
                end: 0xFFFF,
            }],
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        }
    }
}
/// This fixed physical machine.
pub type Msx = MsxCore<Board>;
