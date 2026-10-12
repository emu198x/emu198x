import {readFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
import init,{Spectrum,basicTape} from '../pkg/emu198x_spectrum_web.js';
// Run after scripts/build-npm.sh. Optionally pass a Build198x executable and
// a Code198x code-samples checkout to compare the three maintained lessons.
const fixture=new URL('./fixtures/rom-editor/',import.meta.url);
const wasm=await readFile(new URL('../pkg/emu198x_spectrum_web_bg.wasm',import.meta.url));
await init({module_or_path:wasm});
const source=await readFile(new URL('cases.bas',fixture),'utf8');
const hex=(await readFile(new URL('cases.hex',fixture),'utf8')).replace(/\s/g,'');
const expected=Buffer.from(hex,'hex');
let machine=Spectrum.createHeadlessBundled();
try{
 machine.runBasic(source);
 assert.deepEqual(Buffer.from(machine.readMemory(0x5ccb,expected.length)),expected);
}finally{machine.free();}
machine=Spectrum.createHeadlessBundled();
try{
 machine.runBasic('10 POKE 32768,42');
 assert.equal(machine.readMemory(32768,1)[0],42);
}finally{machine.free();}
machine=Spectrum.createHeadlessBundled();
try{
 machine.runCode(Uint8Array.from([0x3e,73,0x32,0x01,0x80,0xc9]),0x9000,0x9000);
 assert.equal(machine.readMemory(32769,1)[0],73);
}finally{machine.free();}
for(const line of (await readFile(new URL('refused.bas',fixture),'utf8')).trim().split('\n'))assert.throws(()=>basicTape(line,'refused'));
const [buildArg,samplesArg]=process.argv.slice(2);
assert.equal(Boolean(buildArg),Boolean(samplesArg),'Pass both executable and sample checkout, or neither');
const build=buildArg?resolve(buildArg):null;
const samples=[
 'sinclair-zx-spectrum/basic/meet-basic/unit-01/steps/step-02.bas',
 'sinclair-zx-spectrum/basic/sonar/unit-06/sonar.bas',
 'sinclair-zx-spectrum/basic/night-patrol/prototype/night-patrol.bas'
];
const results=[];
const temporary=build?await mkdtemp(join(tmpdir(),'emu198x-basic-parity-')):null;
try {
for(let i=0;build&&i<samples.length;i++){
 const file=join(resolve(samplesArg),samples[i]);
 const text=await readFile(file,'utf8');
 const out=join(temporary,`sample-${i}.tap`);
 execFileSync(build,['basic',file,'--machine','sinclair-zx-spectrum','--name','lesson','-o',out],{encoding:'utf8'});
 const native=await readFile(out),browser=Buffer.from(basicTape(text,'lesson'));
 assert.deepEqual(browser,native,samples[i]);
 results.push({sample:samples[i],bytes:browser.length,sourceSha256:createHash('sha256').update(text).digest('hex')});
}
} finally {if(temporary)await rm(temporary,{recursive:true});}
const result={wasmSha256:createHash('sha256').update(wasm).digest('hex'),buildBinarySha256:build?createHash('sha256').update(await readFile(build)).digest('hex'):null,romEditorBytes:expected.length,basicExecution:true,assemblyLauncher:true,samples:results};
console.log(JSON.stringify(result,null,2));
