-- Report the first frame at which a ROM changes ANY part of the
-- structural screen state, and which part.
--
-- ⚠ Checking the nametable alone is too narrow, and the mistake is easy
-- to make twice. The full_palette family never writes a nametable byte —
-- it renders by writing palette RAM under a fixed tile — so a
-- nametable-only probe reports "never draws" for a ROM that plainly
-- does. dmc_tests genuinely never draw; full_palette draws entirely in
-- the palette. Only a probe covering all three channels can tell those
-- apart.
--
-- Requires main.cpp's RamPowerOnState = AllZeros, so "unchanged" means
-- untouched rather than lost in power-on noise.
local reported = false
local frames = 0
local basePal = nil

local function palString()
  local t = {}
  for i = 0, 31 do
    t[#t + 1] = string.format("%02X", emu.read(0x3F00 + i, emu.memType.nesPpuMemory))
  end
  return table.concat(t)
end

local function ntWritten()
  for a = 0x2000, 0x27FF, 3 do
    if emu.read(a, emu.memType.nesPpuMemory) ~= 0 then return true end
  end
  return false
end

local function oamWritten()
  for i = 0, 255, 3 do
    if emu.read(i, emu.memType.nesSpriteRam) ~= 0 then return true end
  end
  return false
end

emu.addEventCallback(function()
  frames = frames + 1
  if basePal == nil then basePal = palString() end
  if not reported then
    local what = nil
    if ntWritten() then what = "NAMETABLE"
    elseif palString() ~= basePal then what = "PALETTE"
    elseif oamWritten() then what = "OAM" end
    if what then
      reported = true
      emu.log("FIRST_WRITE " .. what .. " frame " .. frames)
    end
  end
  if frames == 900 then
    if not reported then emu.log("FIRST_WRITE NONE frame 900") end
    emu.log("DONE")
  end
end, emu.eventType.endFrame)
