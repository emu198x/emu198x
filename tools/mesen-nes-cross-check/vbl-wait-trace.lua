-- Trace every entry to test_ppu_read_buffer's VBlank-wait subroutine.
--
--   $EBCE  BIT $8E      ; skip the wait entirely if the flag is set
--   $EBD0  BMI $EBDA
--   $EBD2  BIT $2002    ; clear the VBL flag
--   $EBD5  BIT $2002    ; wait for it to be set again
--   $EBD8  BPL $EBD5
--   $EBDA  RTS
--
-- The sub-test loop spends 92% of its time here. Emu198x runs the loop
-- on a flat 12-frame period with 10 wait calls, at PPU positions that
-- repeat EXACTLY every iteration; Mesen2 runs it on a repeating
-- 12,10,10. This logs the same events on the Mesen side so the two are
-- directly comparable rather than inferred from frame numbers.
--
-- Emits: WAIT <frame> sl=<scanline> cyc=<cycle>
--
-- ⚠ Mesen's script log is a 500-row ring buffer. The window is bounded
-- so the interesting frames are not scrolled away by later ones.

local FIRST = 150
local LAST = 200

-- ⚠ Count frames locally, as palette-phases.lua and
-- nametable-phases.lua do. `ppu.frameCount` counts from power-on and a
-- script's own counter starts at script load — they differ by 2 on this
-- ROM, and mixing them silently offsets every frame number in a
-- cross-script comparison.
local frames = 0
emu.addEventCallback(function()
  frames = frames + 1
end, emu.eventType.endFrame)

local function log_at(tag)
  return function()
    local st = emu.getState()
    local f = frames
    if f < FIRST or f > LAST then
      return
    end
    emu.log(string.format("%s %5d sl=%3d cyc=%3d cpu=%d", tag, f, st["ppu.scanline"], st["ppu.cycle"], st["cpu.cycleCount"]))
  end
end

-- $EBDA is the RTS: logging entry AND exit gives each wait's duration,
-- which is what separates a normal one-frame wait from one that lost a
-- frame to a suppressed VBL flag.
emu.addMemoryCallback(log_at("EXIT"), emu.callbackType.exec, 0xEBDA)

emu.addMemoryCallback(function()
  -- ⚠ emu.getState() returns a FLAT table with dotted key names —
  -- st["ppu.scanline"], not st.ppu.scanline. The nested form indexes nil.
  local st = emu.getState()
  local f = frames
  if f < FIRST or f > LAST then
    return
  end
  emu.log(string.format("WAIT %5d sl=%3d cyc=%3d cpu=%d", f, st["ppu.scanline"], st["ppu.cycle"], st["cpu.cycleCount"]))
end, emu.callbackType.exec, 0xEBD2)

emu.addEventCallback(function()
  if frames == LAST then
    emu.log("DONE")
  end
end, emu.eventType.endFrame)
