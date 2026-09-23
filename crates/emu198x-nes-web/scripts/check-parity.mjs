// Run after wasm-pack build; pass heartbeat.nes then read-pad.nes.
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import init, {Nes} from '../pkg/emu198x_nes_web.js';
await init({module_or_path:readFileSync(new URL('../pkg/emu198x_nes_web_bg.wasm',import.meta.url))});
const expected=[['05389dc5','08d09dc5','1cf81dc5'],['2bd2bdc5','deb61dc5','2bd2bdc5']];
assert.equal(process.argv.slice(2).length,2,'Pass heartbeat.nes and read-pad.nes');
for (const [index,file] of process.argv.slice(2).entries()) {
  const bytes=readFileSync(file);
  for(let repeat=0;repeat<2;repeat++) {
    const nes=new Nes(bytes);
    for (const [stage,pressed] of [false,true,false].entries()) {
      nes.button_a(pressed);
      for(let frame=0;frame<10;frame++) nes.step();
      let hash=2166136261;
      for(const byte of nes.pixels()) hash=Math.imul(hash^byte,16777619)>>>0;
      assert.equal(hash.toString(16).padStart(8,'0'),expected[index][stage]);
    }
    nes.free();
  }
  console.log(`${file}: WASM matches native at 10/20/30 frames; fresh-machine repeat matches`);
}
assert.throws(()=>new Nes(new Uint8Array([1,2,3])));
console.log('Malformed cartridge rejected');
