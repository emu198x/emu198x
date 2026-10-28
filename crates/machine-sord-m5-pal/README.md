# Sord M5 PAL

This private machine crate binds the shared `common-sord-m5` wiring to the
PAL board. `SordM5::new(monitor_rom, cartridge_rom)` has no region selector.
The family runtime and existing profile route remain `runtime-sord-m5` and
`sord-m5-pal`. BASIC-I is a separate cartridge, not part of the monitor ROM.

Machine tests here qualify this board. Common wiring and family runtime tests
remain shared evidence. Version-5 saves from the other board are rejected.
