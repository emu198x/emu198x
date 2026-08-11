-- CPU cycles per frame, measured at endFrame.
--
-- ⚠ Why this matters. In test_ppu_read_buffer's long sub-test loop, the
-- CPU work between VBlank waits is IDENTICAL in both emulators, cycle
-- for cycle (24666, 30699, 28122, ...). An iteration costs exactly
-- 357366 CPU cycles in Emu198x and in Mesen2's slow iterations alike.
-- So the only thing that can move the code's arrival PPU dot — and push
-- a wait past the threshold where it costs a whole extra frame — is how
-- many dots elapse per CPU cycle, i.e. frame length.
--
-- Emu198x alternates 29781/29780 strictly, giving a period that repeats
-- exactly every 12 frames. Mesen2's loop does not repeat that way. This
-- reports its per-frame counts so the two can be compared directly.
local FIRST = 150
local LAST = 200
local prev = nil
emu.addEventCallback(function()
  local st = emu.getState()
  local f = st["ppu.frameCount"]
  local c = st["cpu.cycleCount"]
  if prev and f >= FIRST and f <= LAST then
    emu.log(string.format("FRAME %5d cycles=%d", f, c - prev))
  end
  prev = c
end, emu.eventType.endFrame)
