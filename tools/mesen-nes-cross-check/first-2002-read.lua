-- Position of the FIRST $2002 read in each frame.
--
-- ⚠ Used to bisect where two emulators' phase first diverges. Both run
-- the same ROM deterministically, so if every behaviour matched they
-- would stay in the same phase; they do not, and the first frame whose
-- first-read position differs is where to look.
--
-- ⚠ Position labels are NOT directly comparable between emulators.
-- Mesen processes cycle N then exposes _cycle == N; Emu198x exposes the
-- dot it is ABOUT to process. Emu198x dot D is the same physical moment
-- as Mesen cycle D-1. Compare accordingly — this cost a wrong
-- "suppression window is off by one" reading.
local FIRST = 40
local LAST = 140
local seen = {}
emu.addMemoryCallback(function()
  local st = emu.getState()
  local f = st["ppu.frameCount"]
  if f < FIRST or f > LAST or seen[f] then
    return
  end
  seen[f] = true
  emu.log(string.format("FIRST2002 %5d sl=%3d cyc=%3d", f, st["ppu.scanline"], st["ppu.cycle"]))
end, emu.callbackType.read, 0x2002)
