// Local native/WASM probes. Paths are consumed here, never copied to the player.
import {existsSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const synthetic={
 'acorn-atom-rom':'acorn-atom.rom','acorn-bbc-mos':'acorn-bbc-micro.rom','acorn-electron-os':'acorn-electron.rom','acorn-electron-basic':'acorn-electron-basic.rom','amstrad-cpc464-firmware':'amstrad-cpc.rom',
 'atari-5200-bios':'atari-5200-bios-handover.rom','colecovision-bios':'coleco-colecovision.rom','commodore-amiga-kickstart-rom':'commodore-amiga-kickstart.rom',
 'commodore-pet-kernal':'commodore-pet-kernal.rom','commodore-pet-basic':'commodore-pet-basic.rom','commodore-pet-editor':'commodore-pet-editor.rom','commodore-pet-char':'commodore-pet-chargen-4k.rom',
 'commodore-vic-20-kernal':'commodore-vic-20-kernal.rom','commodore-vic-20-basic':'commodore-vic-20-basic.rom','commodore-vic-20-char':'commodore-vic-20-chargen.rom',
 'dragon32-basic-rom':'dragon-32.rom','dragon64-compatible-rom':'dragon-32.rom','jupiter-ace-rom':'jupiter-ace.rom',
 'mattel-aquarius-rom':'mattel-aquarius.rom','memotech-mtx-rom':'memotech-mtx.rom','msx1-bios':'msx.rom','oric-rom':'oric-atmos.rom','sinclair-zx80-rom':'sinclair-zx80.rom','sinclair-zx81-rom':'sinclair-zx81.rom','sord-m5-rom':'sord-m5.rom','spectravideo-svi-328-rom':'spectravideo-svi-328.rom','tatung-einstein-mos':'tatung-einstein.rom',
};
const carts={
 'atari-2600':'synthetic-cartridges/atari-2600-logo.bin','atari-5200':'synthetic-cartridges/atari-5200-logo.bin','atari-7800':'synthetic-cartridges/atari-7800-logo.bin',
 'nintendo-nes':'synthetic-cartridges/nintendo-nes-logo.nes','nintendo-game-boy':'synthetic-cartridges/nintendo-game-boy-logo.gb',
 'sega-game-gear':'sega/synthetic-cart/game-gear.gg','sega-master-system':'sega/synthetic-cart/master-system.sms','sega-sg-1000':'sega/synthetic-cart/sg-1000.sg',
};
export function fixtures(profiles,romRoot=process.env.PLAYER_TEST_ROM_ROOT) {
 const cases=[], missing=[];
 for(const p of profiles) {
  const firmware={};let unavailable=false;
  for(const source of p.sources) {
   const candidates=romRoot?p.romDirs.flatMap(dir=>source.candidates.map(name=>path.join(romRoot,dir,name))):[];
   if(synthetic[source.id])candidates.push(path.join(root,'test-data/synthetic-firmware',synthetic[source.id]));
   const file=candidates.find(existsSync);
   if(file)firmware[source.id]=file;
   else if(!source.optional)unavailable=true;
  }
  if(p.family==='atari-800xl' && !firmware['atari-800xl-os'])unavailable=true;
  if(unavailable){missing.push(`${p.family}/${p.id}`);continue;}
  const media=carts[p.family]?{slot:p.slots.find(s=>s.kind==='Cartridge').id,path:path.join(root,'test-data',carts[p.family])}:undefined;
  cases.push({family:p.family,id:p.id,firmware,media});
 }
 return {cases,missing};
}
