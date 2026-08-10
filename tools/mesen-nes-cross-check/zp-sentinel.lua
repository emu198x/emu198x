-- Dump zero page and the $6000 shell region once the multi-cart has finished.
--
-- Motivating case: blargg_nes_cpu_test5 is graded by the sweep on
-- `$00FF == 0xFF`, read as "some sub-test failed". That reading was inherited
-- from a 2026-05-30 investigation and never checked against a reference. If
-- Mesen2 -- which runs this ROM correctly -- also ends with $00FF == 0xFF, the
-- byte is not a failure sentinel and the verdict built on it is wrong.
--
-- Emits `ZP <row> <32 hex bytes>` for $0000-$00FF, then `SHELL <16 bytes>` for
-- $6000-$600F.

local dumped = false

local function dump()
  if dumped then return end
  dumped = true
  for row = 0, 7 do
    local parts = {}
    for col = 0, 31 do
      local b = emu.read(row * 32 + col, emu.memType.nesMemory)
      parts[#parts + 1] = string.format("%02X", b)
    end
    emu.log("ZP " .. string.format("%02X", row * 32) .. " " .. table.concat(parts))
  end
  local shell = {}
  for i = 0, 15 do
    shell[#shell + 1] = string.format("%02X", emu.read(0x6000 + i, emu.memType.nesMemory))
  end
  emu.log("SHELL " .. table.concat(shell))
  emu.log("DONE")
end

local frames = 0
emu.addEventCallback(function()
  frames = frames + 1
  if frames >= 1800 then
    dump()
  end
end, emu.eventType.endFrame)
