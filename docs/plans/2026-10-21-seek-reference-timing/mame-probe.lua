-- SPDX-License-Identifier: GPL-2.0-or-later
-- Direct-port measurement; authored idle-loop injection, NOT ROM loading.
local cpu=assert(manager.machine.devices[":maincpu"])
local ports=cpu.spaces["io"]
local output=assert(os.getenv("PROBE_OUTPUT"))
local period=assert(tonumber(os.getenv("PROBE_POLL_US")))/1000000
local function poll(wanted)
 local deadline=emu.time()+0.5
 while emu.time()<deadline do
  local msr=ports:read_u8(0x2ffd)
  if (msr&0xc0)==wanted then return msr end
  emu.wait(0.000001)
 end
 error("register deadline")
end
local function send(b) poll(0x80);ports:write_u8(0x3ffd,b) end
local function sis()
 send(8);poll(0xc0);local st0=ports:read_u8(0x3ffd)
 if st0==0x80 then return st0,-1 end
 poll(0xc0);return st0,ports:read_u8(0x3ffd)
end
emu.wait(5)
local mem=cpu.spaces["program"]
for i,b in ipairs({0xf3,0xc3,0,0x80}) do mem:write_u8(0x7fff+i,b) end
cpu.state["PC"].value=0x8000
emu.wait(0.001);ports:write_u8(0x1ffd,8);emu.wait(1)
for i=1,5 do local st0=sis();if st0==0x80 then break end end
local f=assert(io.open(output,"w"))
local function journey(srt,index,from,target,retain)
 send(15);send(0)
 local start=emu.time();send(target)
 local deadline=start+0.25
 local samples={};local st0,pcn
 repeat
  assert(emu.time()<deadline,"seek deadline")
  st0,pcn=sis()
  samples[#samples+1]=string.format('{"seconds":%.9f,"st0":%d,"pcn":%d}',emu.time()-start,st0,pcn)
  if st0~=0x80 then break end
  emu.wait(period)
 until false
 assert(st0==32 and pcn==target,"seek status/PCN")
 assert(ports:read_u8(0x2ffd)==0x80,"idle MSR")
 if retain then
  f:write(string.format('{"srt":%d,"case":%d,"from":%d,"target":%d,"poll_us":%d,"samples":[%s],"completion_seconds":%.9f,"st0":%d,"pcn":%d,"idle_msr":128}\n',srt,index,from,target,period*1000000,table.concat(samples,","),emu.time()-start,st0,pcn))
 end
end
for _,srt in ipairs({0,1,14,15}) do
 send(3);send(srt<<4);send(3)
 journey(srt,-1,-1,0,false)
 local from=0
 for index,target in ipairs({1,3,1,1}) do
  journey(srt,index-1,from,target,true);from=target
 end
end
assert(f:close());manager.machine:exit()
