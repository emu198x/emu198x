-- Dump nametable 0 as raw tile indices, so a summary screen's layout can be
-- compared against Emu198x's byte for byte.
--
-- Motivating case: blargg_nes_cpu_test5/official.nes prints a per-sub-test
-- pass marker (tile $00) at column 31. Emu198x puts eleven markers on the
-- eleven rows BELOW the eleven test names, which reads either as an off-by-one
-- row or as the shell's intended layout. Only the reference can say which.
--
-- Emitted as one line per row: `NT <row> <32 hex bytes>`. The script log is a
-- 500-row ring buffer, so 30 rows fits comfortably.

local dumped = false

local function dump()
  if dumped then return end
  dumped = true
  for row = 0, 29 do
    local parts = {}
    for col = 0, 31 do
      -- Nametable 0 starts at PPU $2000.
      local b = emu.read(0x2000 + row * 32 + col, emu.memType.nesPpuMemory)
      parts[#parts + 1] = string.format("%02X", b)
    end
    emu.log("NT " .. string.format("%02d", row) .. " " .. table.concat(parts))
  end
  emu.log("DONE")
end

-- The multi-cart runs all eleven sub-tests before printing "All tests
-- complete"; sampling on a late frame avoids catching it mid-run.
local frames = 0
emu.addEventCallback(function()
  frames = frames + 1
  if frames >= 1800 then
    dump()
  end
end, emu.eventType.endFrame)
