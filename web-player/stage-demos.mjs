import {cpSync,mkdirSync,readdirSync,readFileSync,writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
export const demos=JSON.parse(readFileSync(new URL('./demos.json',import.meta.url)));
export function demoFor(family) {
 const demo=demos.find(item=>item.family===family);
 return demo?{url:`demos/${path.basename(demo.file)}`,title:demo.title,description:demo.description,source:'https://github.com/emu198x/emu198x/tree/main/web-player#bundled-demos'}:undefined;
}
export function stageDemos(destination) {
 const output=path.join(destination,'demos');mkdirSync(output,{recursive:true});
 for(const demo of demos)cpSync(path.join(root,'test-data',demo.file),path.join(output,path.basename(demo.file)));
 cpSync(path.join(root,'LICENSE'),path.join(output,'LICENSE.txt'));
 for(const directory of ['synthetic-cartridges','sega/synthetic-cart']) {
  const source=path.join(root,'test-data',directory), target=path.join(output,'source',directory);mkdirSync(target,{recursive:true});
  for(const file of readdirSync(source))if(/\.(py|s|asm|md)$/.test(file))cpSync(path.join(source,file),path.join(target,file));
 }
 writeFileSync(path.join(output,'provenance.json'),JSON.stringify(demos.map(d=>({...d,source:`https://github.com/emu198x/emu198x/tree/main/test-data/${path.dirname(d.file)}`,licence:'GPL-2.0-or-later'})),null,2)+'\n');
}
