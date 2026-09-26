// Checks the bundled-firmware guard without needing the ROM, and, given a built
// distribution, that no firmware image travels in it as a separate file.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {chmodSync,mkdirSync,mkdtempSync,readdirSync,readFileSync,rmSync,statSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spectrum48kRom,SPECTRUM_48K_ROM} from './bundled-firmware.mjs';
import {makeCatalogue} from './catalogue.mjs';
const scratch=mkdtempSync(path.join(tmpdir(),'198x-bundled-firmware-'));
try {
 // Unset or empty (a CI secret that is not configured) means no bundling.
 assert.equal(spectrum48kRom({}),null);
 assert.equal(spectrum48kRom({EMU198X_SPECTRUM_48K_ROM:''}),null);
 // Set, but wrong, fails loudly rather than quietly building without it.
 assert.throws(()=>spectrum48kRom({EMU198X_SPECTRUM_48K_ROM:path.join(scratch,'missing.rom')}),/names no file/);
 const short=path.join(scratch,'short.rom');writeFileSync(short,new Uint8Array(8192));
 assert.throws(()=>spectrum48kRom({EMU198X_SPECTRUM_48K_ROM:short}),/8192 bytes/);
 // Right size, wrong bytes: the shape of a patched 48K ROM.
 const patched=path.join(scratch,'patched.rom');writeFileSync(patched,new Uint8Array(SPECTRUM_48K_ROM.size).fill(0xff));
 assert.throws(()=>spectrum48kRom({EMU198X_SPECTRUM_48K_ROM:patched}),/not the unmodified Sinclair 48K ROM/);
 // The npm package's build applies the same check, before wasm-pack runs. A
 // stub wasm-pack earlier on PATH fails the check if the build gets that far.
 const bin=path.join(scratch,'bin');mkdirSync(bin);
 writeFileSync(path.join(bin,'wasm-pack'),'#!/bin/sh\necho "wasm-pack ran" >&2\nexit 99\n');chmodSync(path.join(bin,'wasm-pack'),0o755);
 const buildNpm=fileURLToPath(new URL('../crates/emu198x-spectrum-web/scripts/build-npm.sh',import.meta.url));
 for(const [rom,reason] of [[patched,/not the unmodified Sinclair 48K ROM/],[short,/8192 bytes/]]) {
  const run=spawnSync('bash',[buildNpm],{env:{...process.env,PATH:`${bin}${path.delimiter}${process.env.PATH}`,EMU198X_SPECTRUM_48K_ROM:rom},encoding:'utf8'});
  assert.notEqual(run.status,0,'build-npm.sh accepted a ROM that is not the genuine 48K image');
  assert.doesNotMatch(run.stderr,/wasm-pack ran/,'build-npm.sh reached wasm-pack with a wrong ROM');
  assert.match(run.stderr,reason);
 }
} finally {rmSync(scratch,{recursive:true,force:true});}
// Only the 48K variant is marked, and only when the build embedded the ROM.
const families=JSON.parse(readFileSync(new URL('./families.json',import.meta.url)));
const profiles=families.flatMap(({family})=>family==='sinclair-zx-spectrum'
 ?['spectrum_16k','spectrum_48k','spectrum_128k'].map(id=>({family,id,name:id,machineId:family,firmware:[{id:`${id}-rom`,display_name:'ROM',optional:false}],slots:[]}))
 :[{family,id:'default',name:family,machineId:family,firmware:[{id:'rom',display_name:'ROM',optional:false}],slots:[{id:'slot',kind:'Cartridge',display_name:'Cartridge',required:false}]}]);
const bundled=entries=>entries.flatMap(entry=>entry.variants.filter(v=>v.firmware.some(f=>f.bundled)).map(v=>`${entry.family}/${v.id}`));
assert.deepEqual(bundled(makeCatalogue(profiles)),[]);
assert.deepEqual(bundled(makeCatalogue(profiles,{bundledSpectrum48k:true})),['sinclair-zx-spectrum/spectrum_48k']);
console.log('Bundled firmware: unset builds bring-your-own; wrong, short and patched images are refused, by the web player and the npm build; only the Spectrum 48K is marked.');
// The ROM may reach a visitor only inside the Spectrum module's wasm.
const dist=process.argv[2];
if(dist) {
 const files=[];const walk=dir=>{for(const name of readdirSync(dir)){const file=path.join(dir,name);statSync(file).isDirectory()?walk(file):files.push(file);}};walk(path.resolve(dist));
 assert.deepEqual(files.filter(file=>/\.rom$/i.test(file)),[],'the distribution contains a .rom file');
 for(const file of files)if(statSync(file).size===SPECTRUM_48K_ROM.size)assert.notEqual(createHash('sha1').update(readFileSync(file)).digest('hex'),SPECTRUM_48K_ROM.sha1,`${file} is the 48K ROM as a separate file`);
 console.log(`Bundled firmware: no firmware image among ${files.length} distribution files.`);
}
