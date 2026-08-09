-- Cycle-by-cycle bus-op trace of the first few DMA episodes.
--
-- `sprdma_and_dmc_dma` fails on Emu198x by exactly +1 cycle at half the get/put
-- alignments. Which cycle is the extra one cannot be read off the ROM's summary
-- table, so log the bus operation of every cycle from each `$4014` write and
-- compare the sequences directly: the first position where the two emulators
-- disagree is the defect.
--
-- Addresses identify the cycle's role without needing any other state:
--   $02xx  OAM DMA read      (the transfer's own reads)
--   $C0xx  DMC sample fetch  (the steal)
--   other  halt / dummy / alignment read at the CPU's pending address

-- A full OAM transfer is ~515 cycles and the script log is a 500-row ring
-- buffer, so pack addresses 16 to a row: whole episodes fit, and the earlier
-- ones survive instead of being scrolled away by the last.
-- Log every episode: the 500-row ring buffer then retains the LAST ~30, which
-- is where the ROM takes its measurements. The first episodes are setup and
-- were already shown to match.
local EPISODES = 1000000
local CYCLES = 560
local PER_ROW = 16

local episode = 0
local count = -1
local row = {}

local function flush()
  if #row > 0 then
    emu.log(table.concat(row, " "))
    row = {}
  end
end

local oamWrites = 0

local function onWrite(addr, value)
  if addr == 0x4014 and episode < EPISODES then
    flush()
    episode = episode + 1
    count = 0
    oamWrites = 0
    emu.log("=== episode " .. episode)
  elseif addr == 0x2004 and count >= 0 then
    -- The transfer's own put cycles. The 256th ends it, which is how this
    -- side knows to stop logging; without it the trace runs on into ordinary
    -- CPU reads and no longer lines up with the Emu198x side, which records
    -- DMA cycles only.
    oamWrites = oamWrites + 1
    if oamWrites >= 256 then
      flush()
      count = -1
    end
  end
end

local function onRead(addr, value)
  if count >= 0 then
    row[#row + 1] = string.format("%04X", addr)
    if #row >= PER_ROW then
      flush()
    end
    count = count + 1
    if count >= CYCLES then
      flush()
      count = -1
    end
  end
end

emu.addMemoryCallback(onWrite, emu.callbackType.write, 0x2004, 0x2004)
emu.addMemoryCallback(onWrite, emu.callbackType.write, 0x4014, 0x4014)
emu.addMemoryCallback(onRead, emu.callbackType.read, 0x0000, 0xFFFF)
emu.log("trace armed")
