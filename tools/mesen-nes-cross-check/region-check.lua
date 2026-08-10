-- Control for EMU198X_MESEN_REGION: report what region Mesen is actually
-- running, not what was requested. A PAL capture taken while Mesen is in
-- NTSC is an NTSC capture with a PAL filename.
local done = false
emu.addEventCallback(function()
  if done then return end
  done = true
  local st = emu.getState()
  local keys = {}
  for k, v in pairs(st) do
    if type(v) ~= "table" then
      keys[#keys + 1] = k .. "=" .. tostring(v)
    else
      keys[#keys + 1] = k .. "={table}"
    end
  end
  table.sort(keys)
  emu.log("STATE " .. table.concat(keys, " "))
  if st.ppu then
    local pk = {}
    for k, v in pairs(st.ppu) do
      if type(v) ~= "table" then pk[#pk + 1] = k .. "=" .. tostring(v) end
    end
    table.sort(pk)
    emu.log("PPU " .. table.concat(pk, " "))
  end
end, emu.eventType.endFrame)
