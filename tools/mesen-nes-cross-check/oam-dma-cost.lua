-- Cost of the OAM DMA at $E50F in test_ppu_read_buffer.
--
--   $E50D  LDA $97
--   $E50F  STA $4014     <- starts the OAM DMA
--   $E512  JSR $E2C9   -> $E2C9
--
-- Hardware: an OAM DMA is 513 CPU cycles when the write lands on a get
-- cycle and 514 when it lands on a put cycle, so across a run the cost
-- MUST take both values. Emu198x reports the identical cost every single
-- time here, which would lock the CPU/PPU phase and is the leading
-- suspect for the 39-frame divergence. This measures Mesen2's.
--
-- ⚠ Measured from $E50F to the JSR TARGET $E2C9, not to $E512. Mesen
-- runs a pending DMA on the cycle after the write, so the exec callback
-- at $E512 fires BEFORE the transfer and reports a flat 4 cycles — the
-- STA alone. Spanning to $E2C9 puts the DMA inside the window.
--
-- Emits: DMA <frame> cycles=<count from $E50F to $E2C9>

local FIRST = 100
local LAST = 260
local at_write = nil

emu.addMemoryCallback(function()
  at_write = emu.getState()["cpu.cycleCount"]
end, emu.callbackType.exec, 0xE50F)

emu.addMemoryCallback(function()
  if at_write == nil then
    return
  end
  local st = emu.getState()
  local f = st["ppu.frameCount"]
  if f >= FIRST and f <= LAST then
    emu.log(string.format("DMA %5d cycles=%d", f, st["cpu.cycleCount"] - at_write))
  end
  at_write = nil
end, emu.callbackType.exec, 0xE2C9)
