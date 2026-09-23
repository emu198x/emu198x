// DOM-level host regression, separate from the executable WASM worker check.
// DOM_PACKAGE points to happy-dom's lib/index.js; no browser is launched.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
if(!process.env.DOM_PACKAGE || !process.argv[2])throw new Error('Set DOM_PACKAGE to happy-dom/lib/index.js and pass the built player directory.');
const {Window}=await import(pathToFileURL(process.env.DOM_PACKAGE));
const window=new Window({url:'https://example.test/systems/game-boy/'});
Object.assign(globalThis,{window,document:window.document,AbortController:window.AbortController});
let nextFrame=0;const animations=new Set();
globalThis.requestAnimationFrame=()=>{const id=++nextFrame;animations.add(id);return id;};
globalThis.cancelAnimationFrame=id=>animations.delete(id);
const workers=[];
globalThis.Worker=class {
 constructor(url){this.url=url;this.calls=[];workers.push(this);}
 postMessage(message){this.calls.push(message);queueMicrotask(()=>this.onmessage?.({data:{id:message.id,result:message.command==='boot'?{frameMs:16.74}:undefined}}));}
 terminate(){this.terminated=true;}
};
const dist=path.resolve(process.argv[2]);
const asset=name=>pathToFileURL(path.join(dist,name));
const {mountPlayer}=await import(asset('player.js'));
const catalog=JSON.parse(readFileSync(asset('catalog.json'),'utf8'));
function mount(kind,selectedVariant){
 const host=document.createElement('div');document.body.append(host);const root=host.attachShadow({mode:'open'});
 root.innerHTML=readFileSync(asset('template.html'),'utf8');
 const canvas=root.getElementById('screen');canvas.getContext=()=>({putImageData(){}});
 const cleanup=mountPlayer(root,{...catalog.find(x=>x.kind===kind || x.family===kind),selectedVariant},asset);
 return {host,root,canvas,cleanup,$:id=>root.getElementById(id)};
}
function file(input,name){Object.defineProperty(input,'files',{configurable:true,value:[new window.File([new Uint8Array(64)],name)]});}
const gb=mount('game-boy');
assert.equal(workers.length,0,'Mounting must not download or start emulation');
assert.equal(gb.$('begin').textContent,'Choose a cartridge');assert(gb.$('firmware-settings').hidden);
file(gb.$('media'),'game.gb');await gb.$('media').onchange();
assert.equal(workers.length,1);assert(gb.$('standby').hidden);assert(!gb.$('pause').hidden);
assert.equal(workers[0].calls[0].command,'boot');assert.equal(workers[0].calls[0].args[0],'game-boy');
assert.equal(workers[0].url.href,asset('worker.js').href,'Workers resolve from assets, not the system page');
const nes=mount('nes');assert(!nes.$('standby').hidden,'Instances must have independent controls');
file(gb.$('media'),'next.gb');await gb.$('media').onchange();assert(workers[0].terminated);assert.equal(workers.length,2);
const amiga=mount('amiga');
amiga.$('begin').click();await new Promise(resolve=>setTimeout(resolve,0));assert(amiga.$('firmware-settings').open);assert.equal(workers.length,2,'Missing firmware must not boot');
assert.equal(amiga.root.activeElement.id,'kickstart');
file(amiga.$('kickstart'),'kick.rom');await amiga.$('setup').onsubmit({preventDefault(){}});
// The submit callback schedules boot; wait for its asynchronous file reads.
for(let i=0;i<20 && !amiga.$('standby').hidden;i++)await new Promise(resolve=>setTimeout(resolve,0));
assert(amiga.$('standby').hidden);assert(!amiga.$('firmware-settings').open);
Object.defineProperty(amiga.root,'pointerLockElement',{configurable:true,writable:true,value:null});
Object.defineProperty(document,'pointerLockElement',{configurable:true,writable:true,value:null});
amiga.canvas.requestPointerLock=()=>{amiga.root.pointerLockElement=amiga.canvas;document.pointerLockElement=amiga.host;document.dispatchEvent(new window.Event('pointerlockchange'));};
document.exitPointerLock=()=>{amiga.root.pointerLockElement=null;document.pointerLockElement=null;document.dispatchEvent(new window.Event('pointerlockchange'));};
amiga.$('capture-mouse').click();assert.equal(amiga.$('capture-mouse').getAttribute('aria-pressed'),'true','Shadow-root pointer lock must be recognised');
document.dispatchEvent(new window.KeyboardEvent('keydown',{code:'Escape'}));assert.equal(amiga.root.pointerLockElement,null);
for(const player of [gb,nes,amiga])player.cleanup();
assert(workers.every(worker=>worker.terminated));assert.equal(animations.size,0,'Disconnected players must cancel their animation loops');
// Every declared model must produce its own setup without starting WASM.
for(const system of catalog)for(const variant of system.variants || []) {
 const player=mount(system.family,variant.id);
 assert.equal(player.$('variant').value || variant.id,variant.id);
 assert.equal(player.$('model').textContent,variant.id===system.defaultVariant && system.kind!=='fleet'?system.model:variant.name);
 player.cleanup();player.host.remove();
}
const sms=mount('sega-master-system');
file(sms.$('media'),'game.sms');await sms.$('media').onchange();
const smsWorker=workers.at(-1);
assert.equal(smsWorker.calls[0].args[4].family,'sega-master-system');
sms.canvas.dispatchEvent(new window.KeyboardEvent('keydown',{code:'Enter'}));
assert.deepEqual(smsWorker.calls.at(-1).args[0],[['key','pause',true]]);
sms.cleanup();
const coleco=mount('coleco-colecovision');
file(coleco.$('media'),'game.col');await coleco.$('media').onchange();
assert(coleco.$('firmware-settings').open,'BIOS consoles must request firmware after cartridge selection');coleco.cleanup();
const atom=mount('acorn-atom');
assert(!atom.$('slot-label').hidden);atom.$('slot').value='program-1';atom.$('slot').onchange();assert.equal(atom.$('media').accept,'.atm');atom.cleanup();
// Exercise the actual custom-element loader too: it should fetch only its
// small presentation assets and tear down when an Astro page removes it.
Object.assign(globalThis,{HTMLElement:window.HTMLElement,customElements:window.customElements});
const fetched=[];
globalThis.fetch=async url=>{fetched.push(String(url));return new Response(readFileSync(url));};
await import(asset('embed.js'));
const element=document.createElement('emu198x-player');element.setAttribute('system','nintendo-game-boy');document.body.append(element);
for(let i=0;i<20 && !element.shadowRoot?.getElementById('screen');i++)await new Promise(resolve=>setTimeout(resolve,0));
assert(element.shadowRoot?.getElementById('screen'),'Custom element should mount its inline screen');
assert.equal(fetched.length,4);assert(fetched.every(url=>!url.includes('.wasm')));
const selector=element.shadowRoot.getElementById('variant');selector.value='mgb';selector.onchange();
assert.equal(element.shadowRoot.getElementById('variant').value,'mgb','Changing model remounts the matching setup');
assert.equal(animations.size,1,'Changing model disposes the previous animation loop');
element.remove();assert.equal(animations.size,0);
console.log('Inline host: lazy start, cartridge autostart/replacement, instance isolation, firmware disclosure, shadow-root mouse capture and disposal passed.');
window.happyDOM.abort();
