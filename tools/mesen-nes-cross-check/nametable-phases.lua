-- Log the frame at which the nametable changes, and which rows changed.
--
-- Companion to `palette-phases.lua`. Where that finds when the screen's
-- colours move, this finds when its text is written, so a timing offset
-- between two emulators can be localised to the sub-test that caused it
-- rather than attributed to the run as a whole.
--
-- Emits one line per changed frame:
--   NTCHG <frame> rows=<comma-separated row numbers>
--
-- ⚠ Mesen's script log is a ring buffer; this logs transitions only.

local LIMIT = 700
local frames = 0
local prev = {}

local function rowhash(row)
  local h = 0
  for col = 0, 31 do
    h = (h * 31 + emu.read(0x2000 + row * 32 + col, emu.memType.nesPpuMemory)) % 16777216
  end
  return h
end

emu.addEventCallback(function()
  frames = frames + 1
  if frames > LIMIT then
    return
  end
  local changed = {}
  for row = 0, 29 do
    local h = rowhash(row)
    if prev[row] ~= h then
      if prev[row] ~= nil then
        changed[#changed + 1] = row
      end
      prev[row] = h
    end
  end
  if #changed > 0 then
    emu.log("NTCHG " .. string.format("%5d", frames) .. " rows=" .. table.concat(changed, ","))
  end
  if frames == LIMIT then
    emu.log("END " .. frames)
  end
end, emu.eventType.endFrame)
