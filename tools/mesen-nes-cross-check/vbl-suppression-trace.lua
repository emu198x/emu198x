-- Every $2002 read landing near the VBlank set dot, with what it read.
--
-- ⚠ This decides between two very different explanations for the
-- 39-frame divergence in test_ppu_read_buffer.
--
-- The wait loop at $EBD5 polls $2002 every 7 CPU cycles = 21 dots. In
-- Emu198x one of those polls lands exactly on scanline 241 dot 1 — the
-- dot the VBlank flag sets on — which suppresses the flag, so the whole
-- frame's VBlank passes unseen and the wait costs an extra frame. That
-- happens on a fixed 12-frame period because our loop is phase-locked.
--
-- Either:
--   (a) PHASE — Mesen2's polls land elsewhere, and it would suppress too
--       if they landed on dot 1; or
--   (b) BEHAVIOUR — Mesen2's polls DO land on dot 1 and it still reports
--       the flag, meaning the suppression window differs.
--
-- (a) means our phase lock is the thing to explain. (b) means a real
-- behavioural difference in $2002-vs-VBL timing.
--
-- Emits: READ <frame> sl=<scanline> cyc=<cycle> value=<byte>

local FIRST = 150
local LAST = 200

emu.addMemoryCallback(function(address, value)
  local st = emu.getState()
  local f = st["ppu.frameCount"]
  if f < FIRST or f > LAST then
    return
  end
  local sl = st["ppu.scanline"]
  local cyc = st["ppu.cycle"]
  -- Only the neighbourhood of the set dot matters.
  if sl == 241 and cyc <= 6 then
    emu.log(string.format("READ %5d sl=%3d cyc=%3d value=%02X", f, sl, cyc, value or 0))
  end
end, emu.callbackType.read, 0x2002)
