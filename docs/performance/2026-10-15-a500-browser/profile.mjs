// Run with Node's --cpu-prof for the identified web-target WASM module.
// The supplied firmware remains local and is never included in the report.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';

const [modulePath, firmwarePath, count='120'] = process.argv.slice(2);
assert(modulePath && firmwarePath, 'Pass generated JS module, firmware and optional frame count');
const frames=Number(count);
assert(Number.isSafeInteger(frames) && frames>0 && frames<=3600);
const moduleUrl=pathToFileURL(resolve(modulePath));
const wasmPath=new URL('./emu198x_commodore_web_bg.wasm',moduleUrl);
const wasm=readFileSync(wasmPath),firmware=readFileSync(firmwarePath);
const {default:init,Commodore}=await import(moduleUrl);
await init({module_or_path:wasm});
const machine=Commodore.amiga(firmware);
machine.configure_audio(48000);
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const result={node:process.version,wasm_sha256:sha(wasm),firmware_sha256:sha(firmware),frames,frameMs:machine.frame_ms(),samples:[]};
try {
  for(let i=0;i<20;i++){machine.step();machine.audio();}
  for(let repeat=0;repeat<3;repeat++) {
    let stepMs=0,pixelsMs=0,audioMs=0,audioSamples=0,audioEnergy=0;
    for(let i=0;i<frames;i++) {
      let now=performance.now();machine.step();stepMs+=performance.now()-now;
      now=performance.now();const pixels=machine.pixels();pixelsMs+=performance.now()-now;
      now=performance.now();const audio=machine.audio();audioMs+=performance.now()-now;
      assert.equal(pixels.length,machine.width()*machine.height()*4);
      assert(audio.every(Number.isFinite));audioSamples+=audio.length;
      for(const sample of audio)audioEnergy+=sample*sample;
    }
    result.samples.push({stepMs,pixelsMs,audioMs,audioSamples,audioEnergy});
  }
  result.frame_sha256=sha(machine.pixels());
  const state=machine.save_state();
  machine.step();machine.audio();machine.step();machine.audio();const future=machine.pixels();
  machine.restore_state(state);machine.step();machine.audio();machine.step();machine.audio();
  assert.deepEqual(machine.pixels(),future,'save/restore framebuffer');
  result.save_restore='pass';
  console.log(JSON.stringify(result,null,2));
} finally {machine.free();}
