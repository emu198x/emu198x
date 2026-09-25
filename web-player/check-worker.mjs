// Executes the shipped worker and WASM under Node. Only the Web Worker transport
// and file:// fetch are adapted; boot, media, input and frames use shipped code.
import {Worker} from 'node:worker_threads';
import {readFileSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const dist=path.resolve(process.argv[2] || '/private/tmp/198x/browser-player');
const read=file=>new Uint8Array(readFileSync(file));
const fixture=name=>read(path.join(root,'test-data/synthetic-cartridges',name));
const bootstrap=`
const {parentPort,workerData}=require('node:worker_threads');
const {readFile}=require('node:fs/promises');
global.self=global;
global.postMessage=(data,transfer)=>parentPort.postMessage(data,transfer);
const fetchOriginal=global.fetch;
global.fetch=async(input,...args)=>{
 const url=new URL(input);
 return url.protocol==='file:' ? new Response(await readFile(url),{headers:{'Content-Type':'application/wasm'}}) : fetchOriginal(input,...args);
};
import(workerData).then(()=>parentPort.on('message',data=>self.onmessage({data})));
`;
async function check(kind,roms,media) {
 const worker=new Worker(bootstrap,{eval:true,workerData:pathToFileURL(path.join(dist,'worker.js')).href});
 let serial=0; const pending=new Map();
 worker.on('message',({id,result,error})=>{const task=pending.get(id);if(!task)return;pending.delete(id);clearTimeout(task.timer);error?task.reject(new Error(error)):task.resolve(result);});
 const rpc=(command,...args)=>new Promise((resolve,reject)=>{const id=++serial;const timer=setTimeout(()=>reject(new Error(`${kind} ${command} timed out`)),30000);pending.set(id,{resolve,reject,timer});worker.postMessage({id,command,args});});
 try {
  const boot=await rpc('boot',kind,roms,media,48000);assert(boot.frameMs>10 && boot.frameMs<30);
  let frame;
  for(let i=0;i<60;i++)frame=await rpc('step');
  assert.equal(frame.pixels.length,frame.width*frame.height*4);assert(frame.width>0);assert(frame.audio.length>0);assert.equal(frame.audio.length%2,0);
  assert([...frame.audio].every(Number.isFinite));
  const event=kind==='spectrum'?['code','KeyA']:['nes','game-boy'].includes(kind)?['button','a']:['key','A'];
  await rpc('input',[[...event,true]]);await rpc('step');await rpc('input',[[...event,false]]);
  const saved=await rpc('save');assert(saved.length>0);
  // Spectrum snapshots omit transient ULA border latches, as in check-fleet:
  // compare after their documented one-cell reseed has rendered a full frame.
  let future=await rpc('step');if(kind==='spectrum')future=await rpc('step');
  await rpc('restore',saved);let replay=await rpc('step');if(kind==='spectrum')replay=await rpc('step');
  assert.deepEqual(replay.pixels,future.pixels,`${kind} save/restore`);
  await assert.rejects(rpc('unknown'),/Unknown player command/);
  if(kind==='amiga') { await rpc('input',[['move',13,-7],['mouse','left',true],['mouse','left',false]]); await assert.rejects(rpc('load','adf',new Uint8Array(5))); }
  console.log(`${kind}: boot, frames, stereo audio, input and error handling passed (${frame.width}×${frame.height})`);
 } finally {for(const task of pending.values())clearTimeout(task.timer);await worker.terminate();}
}
await check('game-boy',{}, {format:'gb',bytes:fixture('nintendo-game-boy-logo.gb')});
await check('nes',{}, {format:'nes',bytes:fixture('nintendo-nes-logo.nes')});
if(process.env.SPECTRUM_ROM)await check('spectrum',{rom:read(process.env.SPECTRUM_ROM)},null);
// A build with the 48K ROM embedded must start the Spectrum with no firmware sent.
const spectrum=JSON.parse(readFileSync(path.join(dist,'catalog.json'))).find(entry=>entry.id==='sinclair-zx-spectrum');
if(spectrum.variants.find(variant=>variant.id==='spectrum_48k').firmware.some(firmware=>firmware.bundled))await check('spectrum',{},null);
if(process.env.AMIGA_ROM)await check('amiga',{kickstart:read(process.env.AMIGA_ROM)},null);
if(process.env.C64_ROM_DIR)await check('c64',Object.fromEntries(['kernal','basic','chargen'].map((name,i)=>[name,read(path.join(process.env.C64_ROM_DIR,['kernal_generic.rom','basic_generic.rom','chargen_openroms.rom'][i]))]).concat([['drive',new Uint8Array()]])),null);
// Independently produced native checkpoints, same input schedule on WASM.
const native=execFileSync('cargo',['run','--quiet','--release','-p','emu198x-game-boy-web','--example','parity','--',path.join(root,'test-data/synthetic-cartridges/nintendo-game-boy-logo.gb')],{cwd:root,encoding:'utf8'}).trim().split('\n').map(line=>line.split(' ').map(Number));
const {default:init,GameBoy}=await import(pathToFileURL(path.join(dist,'modules/game-boy/emu198x_game_boy_web.js')));
await init({module_or_path:read(path.join(dist,'modules/game-boy/emu198x_game_boy_web_bg.wasm'))});
for(let repeat=0;repeat<2;repeat++) {
 const machine=new GameBoy('dmg',fixture('nintendo-game-boy-logo.gb'));machine.configure_audio(48000);
 let index=0;
 for(const pressed of [false,true,false]) {
  machine.button('a',pressed);let count=0,energy=0;
  for(let i=0;i<20;i++){machine.step();const audio=machine.audio();count+=audio.length;for(const sample of audio)energy+=Math.abs(sample);}
  const hash=machine.pixels().reduce((h,b)=>Math.imul(h^b,16777619)>>>0,2166136261);
  const expected=native[index++];assert.equal(hash,expected[0]);assert.equal(count,expected[1]);assert(Math.abs(energy-expected[2])<Math.max(.001,expected[2]*.001));
 }
 machine.free();
}
assert.throws(()=>new GameBoy('dmg',new Uint8Array(4)));assert.throws(()=>new GameBoy('cgb',fixture('nintendo-game-boy-logo.gb')));
console.log('Game Boy native/WASM frame hashes, sample counts and audio energy match twice; malformed inputs rejected.');
