# Spectravideo SVI-328 NTSC

This private crate binds `common-spectravideo-svi-328` to the NTSC board.
`Svi328::new(system_rom)` has no region selector. The family runtime/binary and
existing profile `spectravideo-svi-328-ntsc` remain the entry points. Tests here
qualify this board; common and runtime tests remain shared evidence. A state
from the opposite board is rejected. SVI-318 is separate work under #259.
