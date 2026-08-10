-- Log the frame at which palette RAM changes, for ROMs whose screen moves
-- through phases while a long test runs.
--
-- ⚠ Why this exists: `screen-state.lua` samples one frame. That is only
-- meaningful if the screen has settled by then. `test_ppu_read_buffer`
-- displays art for hundreds of frames while its longest sub-test runs,
-- so a single-frame comparison can catch the two emulators in different
-- phases of the same correct sequence — which is a sampling artifact,
-- not a defect. This finds the phase boundaries so a settled frame can
-- be chosen deliberately.
--
-- Emits one line per change:
--   PHASE <frame> <32 hex bytes>
-- and a final END <frame>.
--
-- ⚠ Mesen's script log is a ring buffer, so this deliberately logs only
-- transitions, never per-frame state.

local LIMIT = 2600
local frames = 0
local last = ""

local function palette()
  local parts = {}
  for i = 0, 31 do
    parts[#parts + 1] = string.format("%02X", emu.read(0x3F00 + i, emu.memType.nesPpuMemory))
  end
  return table.concat(parts)
end

emu.addEventCallback(function()
  frames = frames + 1
  if frames > LIMIT then
    return
  end
  local now = palette()
  if now ~= last then
    emu.log("PHASE " .. string.format("%5d", frames) .. " " .. now)
    last = now
  end
  if frames == LIMIT then
    emu.log("END " .. frames)
  end
end, emu.eventType.endFrame)
