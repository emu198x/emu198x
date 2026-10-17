-- SPDX-License-Identifier: GPL-2.0-or-later
-- Direct register probe with injected idle loop; never ROM qualification.
local cpu=assert(manager.machine.devices[":maincpu"])
local ports=cpu.spaces.io
local op=assert(tonumber(os.getenv("PROBE_OPCODE")))
local pause=assert(tonumber(os.getenv("PROBE_PAUSE")))
local function send(b)
 local deadline=emu.time()+2
 while emu.time()<deadline do
  if (ports:read_u8(0x2ffd)&0xc0)==0x80 then ports:write_u8(0x3ffd,b);emu.wait(0.000001);return end
  emu.wait(0.000001)
 end
 error("command deadline")
end
emu.wait(5)
for i,b in ipairs({0xf3,0xc3,0,0x80}) do cpu.spaces.program:write_u8(0x7fff+i,b) end
cpu.state.PC.value=0x8000
emu.wait(0.001);ports:write_u8(0x1ffd,8);emu.wait(1)
send(3);send(0xdf);send(3)
for _,b in ipairs({op,0,0,0,1,2,1,0x2a,0xff}) do send(b) end
local data,result,times,cleared={},{},{},{}
local deadline=emu.time()+2
local paused=false
while emu.time()<deadline do
 local msr=ports:read_u8(0x2ffd)
 if (msr&0xc0)==0xc0 then
  if (msr&0x20)~=0 then
   assert(#data<2048,"payload bound")
   if not paused and pause>0 then paused=true;emu.wait(pause/1000000)
   else
    times[#times+1]=string.format('%.9f',emu.time())
    data[#data+1]=ports:read_u8(0x3ffd)
    cleared[#cleared+1]=ports:read_u8(0x2ffd)
   end
  else
   result[#result+1]=ports:read_u8(0x3ffd)
   if #result==7 then break end
  end
 end
 emu.wait(0.000001)
end
assert(#result==7,"result deadline")
local f=assert(io.open(assert(os.getenv("PROBE_OUTPUT")),"w"))
f:write(string.format('{"data":[%s],"times":[%s],"after_read_msr":[%s],"result":[%s],"idle_msr":%d}\n',table.concat(data,","),table.concat(times,","),table.concat(cleared,","),table.concat(result,","),ports:read_u8(0x2ffd)))
assert(f:close());manager.machine:exit()
