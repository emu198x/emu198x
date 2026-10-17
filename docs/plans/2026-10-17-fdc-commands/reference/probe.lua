-- SPDX-License-Identifier: GPL-2.0-or-later
-- Direct controller ports, injected idle loop: never ROM qualification.
local cpu=assert(manager.machine.devices[":maincpu"])
local ports=cpu.spaces["io"]
local output=assert(os.getenv("PROBE_OUTPUT"))
local opcode=assert(tonumber(os.getenv("PROBE_OPCODE")))
local fill=assert(tonumber(os.getenv("PROBE_FILL")))
local eot=assert(tonumber(os.getenv("PROBE_EOT")))
local r=assert(tonumber(os.getenv("PROBE_R")))
local stp=assert(tonumber(os.getenv("PROBE_STP")))
local function poll(wanted)
 local deadline=emu.time()+2
 while emu.time()<deadline do
  local msr=ports:read_u8(0x2ffd)
  if (msr&0xc0)==wanted then return msr end
  emu.wait(0.000001)
 end
 error("request deadline")
end
local function send(b)
 poll(0x80);ports:write_u8(0x3ffd,b);emu.wait(0.000001)
end
local function run(op,host)
 for _,b in ipairs({op,0,0,0,r,2,eot,0x2a,stp}) do send(b) end
 local data,result={},{}
 local sent=0
 local deadline=emu.time()+2
 while emu.time()<deadline do
  local msr=ports:read_u8(0x2ffd)
  if (msr&0xe0)==0xa0 then
   assert(host,"unexpected host request")
   assert(sent<4096,"transfer bound")
   ports:write_u8(0x3ffd,host);sent=sent+1
  elseif (msr&0xc0)==0xc0 then
   local byte=ports:read_u8(0x3ffd)
   if (msr&0x20)~=0 then
    assert(#data<4096,"payload bound");data[#data+1]=byte
   else
    result[#result+1]=byte
    if #result==7 then break end
   end
  end
  emu.wait(0.000001)
 end
 assert(#result==7,"result deadline")
 return string.format('{"sent":%d,"data":[%s],"result":[%s],"idle_msr":%d}',sent,table.concat(data,","),table.concat(result,","),ports:read_u8(0x2ffd))
end
emu.wait(5)
local mem=cpu.spaces["program"]
for i,b in ipairs({0xf3,0xc3,0,0x80}) do mem:write_u8(0x7fff+i,b) end
cpu.state["PC"].value=0x8000
emu.wait(0.001);ports:write_u8(0x1ffd,8);emu.wait(1)
send(3);send(0xdf);send(3)
local start=emu.time()
local observed=run(opcode,fill)
local readback="null"
if (opcode&0x1f)==9 then readback=run(0x4c,nil) end
local f=assert(io.open(output,"w"))
f:write(string.format('{"opcode":%d,"seconds":%.9f,"observed":%s,"readback":%s}\n',opcode,emu.time()-start,observed,readback))
assert(f:close());manager.machine:exit()
