//! Fixed Philips VG-8235/00 board preserving the currently implemented base hardware.
pub use common_msx::{
    DateTime, FM_PAC_SRAM_SIZE, IoEvent, MapperType, MemoryMapper, MsxHardware, MsxLayout,
    MsxRegion, RamRegion, Rp5c01, Scc, SlotAddress, V9938, V9938Mode, Ym2413, detect_mapper,
};
use common_msx::{MsxBoard, MsxCore};
/// This board's fixed wiring and topology.
pub struct Board;
impl MsxBoard for Board {
    const HARDWARE: MsxHardware = MsxHardware::PhilipsVg8235;
    const REGION: MsxRegion = MsxRegion::Pal;
    const IS_MSX2: bool = true;
    fn layout() -> MsxLayout {
        MsxLayout {
            sub_rom: Some(SlotAddress::expanded(3, 0)),
            ram: Vec::new(),
            mapper: Some(MemoryMapper {
                slot: SlotAddress::expanded(3, 2),
                size_kb: 128,
            }),
            expanded: [false, false, false, true],
            ..MsxLayout::default()
        }
    }
}
/// This fixed physical machine.
pub type Msx = MsxCore<Board>;
