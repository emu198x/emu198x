-- Structural screen signature: nametable + palette RAM + OAM.
--
-- For ROMs that report visually with a custom tile font, pixels are the wrong
-- comparison — Mesen2 and Emu198x need not agree on palette *rendering* to
-- agree on what the PPU was told to draw. Tile indices, palette entries and
-- sprite state are emulator-independent, and that is what this dumps.
--
-- Emitted after a fixed frame count so the two runs sample the same point:
--   NT  <row> <32 hex bytes>   -- nametable 0, $2000-$23BF + attributes
--   PAL <32 hex bytes>         -- palette RAM $3F00-$3F1F
--   OAM <256 hex bytes>        -- sprite RAM
--   DONE

-- ⚠⚠ NOT USABLE AS A GOLDEN SOURCE YET. Mesen2's NES default is
-- `RamState::Random` (Core/Shared/SettingTypes.h), so nametable RAM,
-- palette RAM and OAM all come up randomised. Two consecutive runs of
-- dmc_tests/latency.nes differ on EVERY line of this dump — the bytes
-- the ROM writes are buried in power-on noise, and a golden captured
-- this way would be a snapshot of one RNG draw.
--
-- The fix is `SetNesConfig` (exported from InteropDLL/ConfigApiWrapper.cpp)
-- with `RamPowerOnState = AllZeros`, which means replicating the
-- `NesConfig` struct layout byte-exactly in main.cpp. Getting that wrong
-- is undefined behaviour rather than a visible error, so it needs doing
-- carefully against the current Mesen2 snapshot.
--
-- The script itself is correct and worth keeping; only the determinism
-- of what it observes is missing.

local dumped = false
-- ⚠ Hardcoded: Mesen sandboxes Lua's `os` library by default, so this
-- cannot be read from the environment. Keep it in step with the frame
-- count the Emu198x side samples at.
local TARGET_FRAME = 600

local function dump()
  if dumped then return end
  dumped = true
  for row = 0, 29 do
    local parts = {}
    for col = 0, 31 do
      parts[#parts + 1] = string.format("%02X",
        emu.read(0x2000 + row * 32 + col, emu.memType.nesPpuMemory))
    end
    emu.log("NT " .. string.format("%02d", row) .. " " .. table.concat(parts))
  end

  local pal = {}
  for i = 0, 31 do
    pal[#pal + 1] = string.format("%02X", emu.read(0x3F00 + i, emu.memType.nesPpuMemory))
  end
  emu.log("PAL " .. table.concat(pal))

  local oam = {}
  for i = 0, 255 do
    oam[#oam + 1] = string.format("%02X", emu.read(i, emu.memType.nesSpriteRam))
  end
  emu.log("OAM " .. table.concat(oam))
  emu.log("DONE")
end

local frames = 0
emu.addEventCallback(function()
  frames = frames + 1
  if frames >= TARGET_FRAME then
    dump()
  end
end, emu.eventType.endFrame)
