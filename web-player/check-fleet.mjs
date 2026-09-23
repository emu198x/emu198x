// Execute the shipped worker and compare frames/audio with native runtimes.
// Synthetic fixtures work in CI. PLAYER_TEST_ROM_ROOT additionally exercises
// locally supplied firmware; neither paths nor ROM bytes enter the distribution.
import {Worker} from 'node:worker_threads';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fixtures} from './fixtures.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const dist=path.resolve(process.argv[2]);
const profiles=JSON.parse(execFileSync('cargo',['run','--quiet','--locked','-p','emu198x-fleet-web','--features','all-families','--bin','fleet-catalogue'],{cwd:root,encoding:'utf8',maxBuffer:4e6}));
const {cases,missing}=fixtures(profiles);
const scratch=mkdtempSync(path.join(tmpdir(),'198x-fleet-'));
let native;
try {
 const manifest=path.join(scratch,'fixtures.json');writeFileSync(manifest,JSON.stringify(cases));
 native=JSON.parse(execFileSync('cargo',['run','--quiet','--locked','--release','-p','emu198x-fleet-web','--features','all-families','--bin','fleet-probe','--',manifest],{cwd:root,encoding:'utf8',maxBuffer:4e6}));
} finally {rmSync(scratch,{recursive:true,force:true});}
const bootstrap=`
const {parentPort,workerData}=require('node:worker_threads');
const {readFile}=require('node:fs/promises');
global.self=global; global.postMessage=(data,transfer)=>parentPort.postMessage(data,transfer);
global.fetch=async input=>new Response(await readFile(new URL(input)),{headers:{'Content-Type':'application/wasm'}});
import(workerData).then(()=>parentPort.on('message',data=>self.onmessage({data})));
`;
for(const [index,test] of cases.entries()) {
 const expected=native[index];assert(!expected.error,`${test.family}/${test.id}: ${expected.error}`);
 const worker=new Worker(bootstrap,{eval:true,workerData:pathToFileURL(path.join(dist,'worker.js')).href});
 let serial=0;const pending=new Map();
 worker.on('message',({id,result,error})=>{const task=pending.get(id);if(!task)return;pending.delete(id);clearTimeout(task.timer);error?task.reject(new Error(error)):task.resolve(result);});
 worker.on('error',error=>{for(const task of pending.values())task.reject(error);});
 const rpc=(command,...args)=>new Promise((resolve,reject)=>{const id=++serial;const timer=setTimeout(()=>reject(new Error(`${test.id}/${command} timed out`)),60000);pending.set(id,{resolve,reject,timer});worker.postMessage({id,command,args});});
 try {
  const roms=Object.fromEntries(Object.entries(test.firmware).map(([id,file])=>[id,new Uint8Array(readFileSync(file))]));
  const media=test.media?{slot:test.media.slot,bytes:new Uint8Array(readFileSync(test.media.path))}:null;
  const boot=await rpc('boot','fleet',roms,media,48000,{family:test.family,id:test.id});
  assert.equal(boot.frameMs,expected.frameMs);assert(boot.frameMs>10 && boot.frameMs<30,`${test.id}: invalid frame period ${boot.frameMs}`);
  assert.deepEqual(boot.keymap,expected.keymap);
  for(const [i,pressed] of [false,true,false].entries()) {
   await rpc('input',[['key','A',pressed],['button','fire1',pressed]]);
   let frame,samples=0,energy=0;
   for(let n=0;n<6;n++){frame=await rpc('step');samples+=frame.audio.length;for(const sample of frame.audio){assert(Number.isFinite(sample));energy+=Math.abs(sample);}}
   assert.equal(frame.pixels.length,frame.width*frame.height*4);assert.equal(frame.width,expected.width);assert.equal(frame.height,expected.height);
   const hash=frame.pixels.reduce((h,b)=>Math.imul(h^b,16777619)>>>0,2166136261);
   assert.equal(hash,expected.checkpoints[i].hash,`${test.id}: frame mismatch`);
   assert.equal(samples,expected.checkpoints[i].samples,`${test.id}: audio count`);
   assert(Math.abs(energy-expected.checkpoints[i].energy)<Math.max(.001,expected.checkpoints[i].energy*.001),`${test.id}: audio energy`);
  }
  const saved=await rpc('save');assert(saved.length>0,`${test.id}: empty save`);
  // Spectrum snapshots deliberately omit transient ULA border latches;
  // compare after their documented one-cell reseed has rendered a full frame.
  let future=await rpc('step');if(test.family==='sinclair-zx-spectrum')future=await rpc('step');
  await rpc('restore',saved);let replay=await rpc('step');if(test.family==='sinclair-zx-spectrum')replay=await rpc('step');
  assert.deepEqual(replay.pixels,future.pixels,`${test.id}: save/restore frame round trip`);
  // Host-only downsampling phase (not guest chip state) is intentionally
  // reset by some snapshots, e.g. POKEY. Allow one stereo output sample.
  assert(Math.abs(replay.audio.length-future.audio.length)<=2,`${test.id}: save/restore audio length`);
  assert([...replay.audio].every(Number.isFinite));
  await assert.rejects(rpc('load','not-a-slot',new Uint8Array(3)),/unknown media slot/);
  console.log(`${test.family}/${test.id}: native/WASM frames, audio and input parity`);
 } finally {for(const task of pending.values())clearTimeout(task.timer);await worker.terminate();}
}
console.log(`Fleet parity: ${cases.length} variants checked; ${missing.length} require additional local firmware.${missing.length?' Set PLAYER_TEST_ROM_ROOT to include them.':''}`);
