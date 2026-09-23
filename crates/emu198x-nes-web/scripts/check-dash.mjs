import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
import init,{Nes} from '../pkg/emu198x_nes_web.js';
const [rom,native]=process.argv.slice(2);
assert.ok(rom&&native,'Pass dash.nes and the compiled native dash_parity example');
const expected=execFileSync(native,[rom],{encoding:'utf8'}).trim().split('\n');
await init({module_or_path:readFileSync(new URL('../pkg/emu198x_nes_web_bg.wasm',import.meta.url))});
for(let repeat=0;repeat<2;repeat++){
  const nes=new Nes(readFileSync(rom));
  const hashes=[];
  for(const [name,pressed,frames] of [['start',false,70],['start',true,4],['start',false,2],['left',true,24],['left',false,6],['a',true,4],['a',false,54],['right',true,36],['a',true,4],['a',false,26],['right',false,60]]){
    nes.button(name,pressed);
    for(let i=0;i<frames;i++)nes.step();
    let hash=2166136261;
    for(const b of nes.pixels())hash=Math.imul(hash^b,16777619)>>>0;
    hashes.push(hash.toString(16).padStart(8,'0'));
  }
  assert.deepEqual(hashes,expected);
  assert.throws(()=>nes.button('invalid',true));
  nes.free();
}
console.log('Dash: 11 native/WASM checkpoints match twice from fresh machines; invalid input rejected.');
console.log(expected.join(' '));
