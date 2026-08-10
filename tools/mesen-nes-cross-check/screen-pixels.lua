-- Framebuffer capture as a COLOUR-INDEX image.
--
-- ⚠ Why not raw pixels. Two emulators need not agree on the RGB value
-- of NES colour $21 — palettes are a rendering choice, and Mesen2 ships
-- several. Comparing raw pixels would report a difference on every
-- pixel of a perfectly correct frame. But if both emulators drew the
-- same picture, their framebuffers are identical *up to a bijection on
-- colours*. Replacing each pixel with the index of its colour's first
-- appearance in raster order cancels that bijection exactly, so the
-- index images must match byte for byte.
--
-- This is what `screen-state.lua` cannot do. Structural state
-- (nametable + palette RAM + OAM) is region-BLIND for every PAL test
-- ROM measured: their region-dependence lives in raster timing, which
-- reaches the framebuffer and nothing else.
--
-- Emits:
--   PX <row> <256 chars>   -- one char per pixel, index in ALPHABET
--   COLOURS <n>
--   DONE
--
-- ⚠ 240 rows plus two lines fits inside Mesen's 500-row script log ring
-- buffer. Do not add per-pixel or per-frame logging here.
--
-- ⚠ Only meaningful once the ROM's screen has SETTLED — see the warning
-- in `screen-state.lua`. Use `palette-phases.lua` to find the
-- boundaries before choosing a frame.

SAMPLE_FRAME = SAMPLE_FRAME or 600

local ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/"
local WIDTH = 256
local HEIGHT = 240

local frames = 0
local dumped = false

local function dump()
  local buf = emu.getScreenBuffer()
  local index = {}
  local count = 0
  local rows = {}
  for y = 0, HEIGHT - 1 do
    local chars = {}
    for x = 0, WIDTH - 1 do
      -- ⚠ 1-based: `buf[0]` is nil, and reading it would make every
      -- frame start with a phantom colour that our side never sees.
      local c = buf[y * WIDTH + x + 1]
      local i = index[c]
      if i == nil then
        i = count
        index[c] = i
        count = count + 1
      end
      chars[#chars + 1] = ALPHABET:sub(i + 1, i + 1)
    end
    rows[#rows + 1] = table.concat(chars)
  end
  -- ⚠ Refuse rather than truncate. More than 64 distinct colours means
  -- the alphabet cannot encode the frame, and a silently clipped image
  -- would compare as a difference that is not one.
  if count > #ALPHABET then
    emu.log("ERROR too many colours: " .. count .. " > " .. #ALPHABET)
    return
  end
  for y = 1, HEIGHT do
    emu.log("PX " .. string.format("%03d", y - 1) .. " " .. rows[y])
  end
  emu.log("COLOURS " .. count)
  emu.log("DONE")
end

emu.addEventCallback(function()
  frames = frames + 1
  if not dumped and frames >= SAMPLE_FRAME then
    dumped = true
    dump()
  end
end, emu.eventType.endFrame)
