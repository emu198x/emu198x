-- Where in the PPU frame does endFrame actually fire?
--
-- ⚠ The frame-boundary comparison failed because "the palette at end of
-- frame" is not well defined: these ROMs rewrite palette RAM many times
-- per frame. But the two emulators may simply be sampling at different
-- PPU positions. Measure Mesen's, rather than assuming it matches
-- run_frame's scanline wrap.
local n = 0
emu.addEventCallback(function()
  n = n + 1
  if n == 600 then
    local st = emu.getState()
    emu.log("ENDFRAME scanline=" .. tostring(st["ppu.scanline"]) ..
            " cycle=" .. tostring(st["ppu.cycle"]) ..
            " frame=" .. tostring(st["ppu.frameCount"]))
    emu.log("DONE")
  end
end, emu.eventType.endFrame)
