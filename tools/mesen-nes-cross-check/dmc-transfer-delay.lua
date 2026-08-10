-- Watch _transferStartDelay across the window where the ROM's re-arm-to-fetch
-- latency is 12, to see when it is set, when it expires, and how that relates
-- to the fetch. Polling state on every read is slow, so it is armed only after
-- the 4th $4015 write, which is where that window begins.
local writes, active, lines = 0, false, 0
local prev = -1

local function onWrite(addr, value)
  if addr == 0x4015 then
    writes = writes + 1
    if writes >= 4 then active = true end
    if active and lines < 120 then
      local st = emu.getState()
      lines = lines + 1
      emu.log(string.format("W  c=%d val=%02X br=%s tsd=%s timer=%s bits=%s",
        st["cpu.cycleCount"], value, tostring(st["apu.dmc.bytesRemaining"]),
        tostring(st["apu.dmc.transferStartDelay"]), tostring(st["apu.dmc.timer.timer"]),
        tostring(st["apu.dmc.bitsRemaining"])))
    end
  end
end

local function onFetch(addr, value)
  if active and lines < 120 then
    local st = emu.getState()
    lines = lines + 1
    emu.log(string.format("F  c=%d br=%s tsd=%s timer=%s bits=%s",
      st["cpu.cycleCount"], tostring(st["apu.dmc.bytesRemaining"]),
      tostring(st["apu.dmc.transferStartDelay"]), tostring(st["apu.dmc.timer.timer"]),
      tostring(st["apu.dmc.bitsRemaining"])))
  end
end

local function onAny(addr, value)
  if active and lines < 120 then
    local st = emu.getState()
    local tsd = st["apu.dmc.transferStartDelay"]
    if tsd ~= prev then
      prev = tsd
      lines = lines + 1
      emu.log(string.format("D  c=%d tsd=%s timer=%s bits=%s br=%s",
        st["cpu.cycleCount"], tostring(tsd), tostring(st["apu.dmc.timer.timer"]),
        tostring(st["apu.dmc.bitsRemaining"]), tostring(st["apu.dmc.bytesRemaining"])))
    end
  end
end

emu.addMemoryCallback(onWrite, emu.callbackType.write, 0x4015, 0x4015)
emu.addMemoryCallback(onFetch, emu.callbackType.read, 0xE3C0, 0xE3C0)
emu.addMemoryCallback(onAny, emu.callbackType.read, 0x0000, 0xFFFF)
