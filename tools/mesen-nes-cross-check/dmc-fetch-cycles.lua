-- Log the CPU cycle of every DMC sample fetch. The ROM's sample address is a
-- constant $E3C0, so the fetch is identifiable without needing the operation
-- type (which Lua memory callbacks do not receive).
--
-- The gap between consecutive fetches is the DMC period plus the length of the
-- DMA that serviced it, so the gaps reveal whether the reference alternates
-- 3-and-4-cycle DMAs where Emu198x is fixed at 4.
local n = 0
local function onRead(addr, value)
  n = n + 1
  if n <= 120 then
    emu.log(string.format("%d", emu.getState()["cpu.cycleCount"]))
  end
end
emu.addMemoryCallback(onRead, emu.callbackType.read, 0xE3C0, 0xE3C0)
