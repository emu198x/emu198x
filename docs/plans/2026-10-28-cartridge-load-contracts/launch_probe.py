import json,subprocess,hashlib
from pathlib import Path
root=Path('/private/tmp/198x-next-launch-fixtures');root.mkdir(exist_ok=True)
binroot=Path('/private/tmp/198x-next-issues-target/debug')
rows=[]
for name,limit,base,ram in [('sord-m5',0x5000,0x2000,0x7000),('sega-sg-1000',0xc000,0,0xc000),('colecovision',0x8000,0x8000,0x6000)]:
 folder=root/name;folder.mkdir(exist_ok=True)
 image=bytearray(limit);start=0x24 if name=='colecovision' else 0
 if start:image[:2]=bytes([0xaa,0x55])
 last=base+limit-1
 image[start:start+12]=bytes([0x3a,last&255,last>>8,0x32,ram&255,ram>>8,0x3e,0x5a,0x32,(ram+1)&255,(ram+1)>>8,0x76])
 image[-1]=0xa7
 valid=folder/'valid.rom';valid.write_bytes(image)
 big=folder/'oversize.rom';big.write_bytes(image+b'!')
 empty=folder/'empty.rom';empty.write_bytes(b'')
 args=[str(binroot/('emu198x-'+name))]
 if name!='sega-sg-1000':
  bios=bytearray(8192);entry=base+start;bios[:3]=bytes([0xc3,entry&255,entry>>8]);path=folder/'bios.rom';path.write_bytes(bios);args+=['--rom',str(path)]
 script=folder/'script.json';script.write_text(json.dumps([{'action':'run_frames','frames':1},{'action':'memory_read','addr':ram,'len':2}]))
 args+=['--frames','0','--script',str(script)]
 for label,path in [('valid',valid),('oversize',big),('empty',empty)]:
  before=hashlib.sha256(path.read_bytes()).hexdigest()
  p=subprocess.run(args+['--cart',str(path)],capture_output=True,text=True,timeout=30)
  assert hashlib.sha256(path.read_bytes()).hexdigest()==before
  if label=='valid':
   assert p.returncode==0,p.stderr
   report=json.loads(p.stdout);reads=[o['bytes'] for o in report['observations'] if o['kind']=='memory_read'];assert reads==[[0xa7,0x5a]],report
   rows.append({'system':name,'input':label,'returncode':p.returncode,'guest_bytes':reads[0]})
  else:
   assert p.returncode!=0,(name,label,p.stdout)
   assert ('cartridge' in p.stderr.lower()),p.stderr
   rows.append({'system':name,'input':label,'returncode':p.returncode,'error':p.stderr.strip()})
print(json.dumps(rows,indent=2))
