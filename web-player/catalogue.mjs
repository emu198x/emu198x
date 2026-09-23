// The runtime owns variants, firmware and slots. Browser-only presentation and
// file extensions live here; no ROM files or local source paths are published.
import {demoFor} from './stage-demos.mjs';
import {readFileSync} from 'node:fs';
const read=name=>JSON.parse(readFileSync(new URL(name,import.meta.url)));
const legacy=read('./src/catalog.json');
const families=read('./families.json');
const defaults={'commodore-amiga':'a500-a501','commodore-c64':'pal','nintendo-game-boy':'dmg','nintendo-nes':'nintendo-nes-ntsc','sinclair-zx-spectrum':'spectrum_48k'};
const formats={
 'acorn-atom':{Tape:'.uef',Program:'.atm'},'acorn-bbc-micro':{Tape:'.uef'},'acorn-electron':{Tape:'.uef'},
 'amstrad-cpc':{Tape:'.cdt,.tzx'},'atari-2600':{Cartridge:'.a26,.bin'},'atari-5200':{Cartridge:'.a52,.bin'},'atari-7800':{Cartridge:'.a78,.bin'},
 'atari-800xl':{Cartridge:'.car,.rom,.bin',Program:'.xex',Disk:'.atr'},'coleco-colecovision':{Cartridge:'.col,.rom,.bin'},
 'commodore-amiga':{Disk:'.adf'},'commodore-c64':{Tape:'.tap',Disk:'.d64,.g64,.d71,.d81',Cartridge:'.crt'},
 'commodore-pet':{Program:'.prg'},'commodore-vic-20':{Program:'.prg',Cartridge:'.prg,.rom,.bin'},
 dragon:{Tape:'.cas',Cartridge:'.pak,.rom',Disk:'.vdk',Snapshot:'.sna',Program:'.bin'},'jupiter-ace':{Snapshot:'.ace'},
 'nintendo-game-boy':{Cartridge:'.gb'},'nintendo-nes':{Cartridge:'.nes'},'oric-atmos':{Tape:'.tap'},
 'sega-game-gear':{Cartridge:'.gg'},'sega-master-system':{Cartridge:'.sms'},'sega-sg-1000':{Cartridge:'.sg,.bin'},
 'sinclair-zx-spectrum':{Tape:'.tap,.tzx',Disk:'.dsk'},'sinclair-zx80':{Tape:'.o'},'sinclair-zx81':{Tape:'.p,.81'},
 'spectravideo-svi-328':{Tape:'.cas'},'tatung-einstein':{Disk:'.dsk'},
};
export function makeCatalogue(profiles) {
 return families.map(({family,alias})=>{
  const old=legacy.find(entry=>entry.id===family);
  const models=profiles.filter(p=>p.family===family);
  const variants=models.map(p=>({id:p.id,name:p.name,firmware:p.firmware,slots:p.slots.map(slot=>({...slot,accept:formats[family]?.[slot.kind] || '.rom,.bin'}))}));
  if(family==='commodore-c64')for(const variant of variants) {
    variant.slots.unshift({id:'prg',kind:'Program',display_name:'PRG program',accept:'.prg',required:false});
    variant.firmware=variant.firmware.filter(f=>f.id!=='commodore-1571-dos-rom');
    for(const slot of variant.slots)if(slot.kind==='Disk')slot.accept=slot.id==='drive-9'?'.d81':'.d64,.g64';
  }
  if(family==='sinclair-zx-spectrum')for(const variant of variants)variant.slots.push({id:'snapshot',kind:'Snapshot',display_name:'Snapshot',accept:'.sna,.z80',required:false});
  const defaultVariant=defaults[family] || variants[0].id;
  const consoleFamily=['atari-2600','atari-5200','atari-7800','coleco-colecovision','nintendo-game-boy','nintendo-nes','sega-game-gear','sega-master-system','sega-sg-1000'].includes(family);
  const variantAliases=family==='sinclair-zx-spectrum'?{'pentagon-128':'pentagon_128','scorpion-zs256':'scorpion_zs256','timex-tc2048':'timex_tc2048','timex-ts2068':'timex_ts2068'}:{};
  const name=old?.name || models[0].name.replace(/ \(.*/, '');
  return {...old,demo:demoFor(family),id:family,variantAliases,aliases:[...new Set([alias,...Object.keys(variantAliases),...models.map(p=>p.machineId),...(old?.aliases || [])])],siteId:models[0].machineId,name,kind:old?.kind || 'fleet',family,defaultVariant,variants,console:consoleFamily,
   model:old?.model || variants[0].name,media:old?.media || variants[0].slots.map(s=>s.accept).join(','),
   help:old?.help || (consoleFamily ? 'Arrow keys move; X and Z are the action buttons. Enter starts or resets; right Shift selects or pauses. Number keys use the controller keypad where available.' : 'Click the screen to type. Select the media drive or slot before choosing a file. Use the machine’s own loading commands for tapes and disks.'),
  };
 });
}
