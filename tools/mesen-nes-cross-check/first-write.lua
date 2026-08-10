-- Report the first frame at which the ROM writes anything to either
-- nametable, and what the screen holds well after that.
--
-- ⚠ Written because a screen-state capture at frame 600 came back all
-- zeros for dmc_tests: with power-on RAM forced to zeros, "blank" means
-- the ROM has not drawn YET, which a golden would freeze as if it were
-- the answer. Sample where there is something to compare.
local reported = false
local frames = 0

local function anyWritten()
  for a = 0x2000, 0x27FF, 7 do
    if emu.read(a, emu.memType.nesPpuMemory) ~= 0 then return true end
  end
  return false
end

emu.addEventCallback(function()
  frames = frames + 1
  if not reported and anyWritten() then
    reported = true
    emu.log("FIRST_WRITE_FRAME " .. frames)
  end
  if frames == 2400 then
    for row = 0, 29 do
      local parts = {}
      for col = 0, 31 do
        parts[#parts + 1] = string.format("%02X",
          emu.read(0x2000 + row * 32 + col, emu.memType.nesPpuMemory))
      end
      emu.log("NT " .. string.format("%02d", row) .. " " .. table.concat(parts))
    end
    emu.log("DONE")
  end
end, emu.eventType.endFrame)
