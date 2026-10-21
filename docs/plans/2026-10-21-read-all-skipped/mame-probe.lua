-- SPDX-License-Identifier: GPL-2.0-or-later
-- Reuses Scan study direct-port/idle-loop host. Not ROM qualification.
local cpu=assert(manager.machine.devices[":maincpu"])
local ports=cpu.spaces["io"]
local output=assert(os.getenv("PROBE_OUTPUT"))
local dtl=assert(tonumber(os.getenv("PROBE_DTL")))
local opcode=assert(tonumber(os.getenv("PROBE_OPCODE")))
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
emu.wait(5)
local mem=cpu.spaces["program"]
for i,b in ipairs({0xf3,0xc3,0,0x80}) do mem:write_u8(0x7fff+i,b) end
cpu.state["PC"].value=0x8000
emu.wait(0.001);ports:write_u8(0x1ffd,8);emu.wait(1)
send(3);send(0xdf);send(3)
local start=emu.time()
for _,b in ipairs({opcode,0,0,0,1,2,1,0x2a,dtl}) do send(b) end
local data,result={},{}
local deadline=emu.time()+2
while emu.time()<deadline do
 local msr=ports:read_u8(0x2ffd)
 if (msr&0xc0)==0xc0 then
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
local idle=ports:read_u8(0x2ffd);assert(idle==0x80,"idle phase")
send(4);send(0);assert((poll(0xc0)&0x20)==0,"recovery result")
local recovery=ports:read_u8(0x3ffd)
local recovered=ports:read_u8(0x2ffd);assert(recovered==0x80,"next command recovery")
local f=assert(io.open(output,"w"))
f:write(string.format('{"data":[%s],"result":[%s],"idle_msr":%d,"recovery":%d,"recovery_msr":%d,"seconds":%.9f}\n',table.concat(data,","),table.concat(result,","),idle,recovery,recovered,emu.time()-start))
assert(f:close());manager.machine:exit()
