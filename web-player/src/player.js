import {playerExtras} from './player-extras.js';
import {writePreferences} from './preferences.js';
import {sessionControls} from './session-controls.js';
import {amigaMouse} from './mouse.js';
export function mountPlayer(root, system, asset, selectVariant) {
const $ = id => root.getElementById(id);
const listeners = new AbortController();
const listen = (target, type, handler) => target.addEventListener(type, handler, {signal:listeners.signal});
let disposed = false, animation;
const variant=system.variants?.find(entry=>entry.id===(system.selectedVariant || system.defaultVariant));
const fleet=system.kind==='fleet' || (variant && variant.id!==system.defaultVariant);
// Only the Spectrum 48K can carry bundled firmware, and only in a build that
// embedded the verified ROM in its wasm; the catalogue says which.
const bundledFirmware=!fleet && system.kind==='spectrum' && Boolean(variant?.firmware?.some(entry=>entry.bundled));
const cartridge = system.console ?? ['nes','game-boy'].includes(system.kind);
let keymap={};
const slots=fleet ? variant.slots : [];
const slot=()=>slots.find(entry=>entry.id===$('slot').value) || slots[0];
const mouseMachine=system.family==='commodore-amiga' || system.kind==='amiga';
const canvas = $('screen'), context = canvas.getContext('2d');
let worker, serial = 0, generation = 0, running = false, busy = false, last = 0;
let audioContext, audioNode, gainNode, volume=0.7, persistence, sessionSeed, extras, exampleMedia;
let mounted={};
const pending = new Map(), held = new Map();
function status(message, error = false) { $('status').textContent = message; $('status').classList.toggle('error',error); $('error-help').hidden=!error; }
function rpc(command,...args) {
  if (!worker) return Promise.reject(new Error('Start a machine first.'));
  const id = ++serial;
  return new Promise((resolve,reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('The emulator did not respond. Restart to try again.')); },60000);
    pending.set(id,{resolve,reject,timer}); worker.postMessage({id,command,args});
  });
}
function send(events) {
  if (!worker || !events.length) return;
  const current = generation;
  rpc('input',events).catch(error => { if(current === generation) { pause(); status(String(error),true); } });
}
function release() {
  send([...held.values()].flat().map(([type,name])=>[type,name,false])); held.clear();
  root.querySelectorAll('#pad button').forEach(button=>button.setAttribute('aria-pressed','false'));
}
const mouse = amigaMouse({canvas,button:$('capture-mouse'),hint:$('mouse-hint'),send,releaseKeys:release});
function clearAudio() { audioNode?.port.postMessage('clear'); }
function stop() {
  mouse.setEnabled(false); release(); generation++; running = false; busy = false;
  worker?.terminate(); worker = undefined;
  for (const {reject,timer} of pending.values()) { clearTimeout(timer); reject(new Error('Machine restarted.')); }
  persistence?.stopped();
  pending.clear(); clearAudio(); $('pause').disabled = $('step').disabled = true;
}
function pause() {
  mouse.setEnabled(false); release(); running = false; clearAudio();
  $('pause').textContent = 'Resume'; $('step').disabled = !worker;
}
function resume() {
  running = true; last = performance.now(); clearAudio();
  mouse.setEnabled(mouseMachine); $('pause').textContent = 'Pause'; $('step').disabled = true; canvas.focus();
}
async function setupAudio() {
  if (!audioContext) {
    audioContext = new AudioContext(); await audioContext.audioWorklet.addModule(asset('audio.js'));
    if(disposed) { await audioContext.close(); throw new Error('Player closed.'); }
    audioNode = new AudioWorkletNode(audioContext,'emulator-audio',{outputChannelCount:[2]}); gainNode=audioContext.createGain();gainNode.gain.value=volume;audioNode.connect(gainNode);gainNode.connect(audioContext.destination);
  }
  await audioContext.resume();
}
async function readFile(input) {
  const selected = input.files[0];
  if (!selected) { if (input.required) throw new Error(`Choose ${input.dataset.label || 'a cartridge'}.`); return new Uint8Array(); }
  if (selected.size > 32 * 1024 * 1024) throw new Error('Choose an unpacked file smaller than 32 MiB.');
  return new Uint8Array(await selected.arrayBuffer());
}
async function mediaFile() {
  if(exampleMedia)return exampleMedia;
  const selected = $('media').files[0];
  if (!selected) { await readFile($('media')); return null; }
  const format = selected.name.split('.').pop().toLowerCase();
  const accept=fleet ? slot()?.accept || '' : system.media;
  if (!accept.split(',').includes(`.${format}`)) throw new Error(`Choose one of these formats: ${accept}.`);
  return {name:selected.name,format,slot:fleet ? (slot()?.id==='snapshot' ? format : slot()?.id) : undefined,bytes:await readFile($('media'))};
}
async function startMachine(propagateErrors = false) {
  if(disposed || $('start').disabled)return;
  if(typeof WebAssembly==='undefined' || typeof Worker==='undefined'){status('This browser cannot run the emulator. WebAssembly and web workers are required.',true);return;}
  await persistence.ready();
  if(disposed || $('start').disabled)return;
  if(system.family==='atari-800xl' && ![...root.querySelectorAll('#firmware input')].some(input=>persistence.hasFirmware(input)) && !$('media').files.length && !exampleMedia) { $('firmware-settings').open=true; status('Choose an OS ROM or a cartridge to start.'); return; }
  if(cartridge && !$('media').files.length && !exampleMedia) { $('media').click(); return; }
  const missing=[...root.querySelectorAll('#firmware input[required]')].find(input=>!persistence.hasFirmware(input));
  if(missing) { $('firmware-settings').open=true; missing.focus(); status(`Choose ${missing.dataset.label} to start this model. Firmware is not included.`,true); return; }
  stop(); const current = generation;
  $('start').disabled = $('begin').disabled = $('choose-media').disabled = $('boot').disabled = true; status('Starting…');
  try {
    let audioWarning='';
    if ($('sound').checked)try {await setupAudio();}catch {$('sound').checked=false;audioWarning=' Sound is unavailable; the machine is running muted.';}
    const roms = {};
    for (const input of root.querySelectorAll('#firmware input[type=file]')) {
      if (!input.disabled) roms[input.id] = await persistence.readFirmware(input,readFile);
    }
    const media = await mediaFile();
    if (current !== generation) return;
    worker = new Worker(asset('worker.js'),{type:'module'});
    worker.onmessage = ({data}) => {
      const request = pending.get(data.id); if (!request) return;
      pending.delete(data.id); clearTimeout(request.timer);
      data.error ? request.reject(new Error(data.error)) : request.resolve(data.result);
    };
    worker.onerror = event => { if(current === generation) { stop(); status(event.message || 'Emulator failed to load. Please restart.',true); } };
    const bootArgs=[system.kind,roms,media,audioContext?.sampleRate || 48000,fleet ? {family:system.family,id:variant.id} : null];
    const boot=await rpc('boot',...bootArgs);
    sessionSeed=bootArgs; mounted={}; if(media)rememberMedia(media);
    await persistence.remember(roms);
    keymap=boot?.keymap || {};
    if (current !== generation) return;
    $('pause').disabled = false; $('pause').hidden = $('start').hidden = false; $('standby').hidden = true; $('choose-media').hidden = fleet && !slots.length;
    $('firmware-settings').open = false; $('start').textContent = 'Restart'; resume(); persistence.started(); status('Running'+audioWarning); return true;
  } catch(error) { if(current === generation) { stop(); $('standby').hidden=false; status(String(error),true); } if(propagateErrors)throw error; }
  finally { if(!disposed)$('start').disabled = $('begin').disabled = $('choose-media').disabled = $('boot').disabled = false; }
}
$('setup').onsubmit=event=>{event.preventDefault();startMachine();};
$('begin').onclick=()=>cartridge ? $('media').click() : startMachine();
$('choose-media').onclick=()=>$('media').click();
function draw(result) {
  if(result.pixels.length) {
    if(canvas.width !== result.width || canvas.height !== result.height) { canvas.width=result.width; canvas.height=result.height; }
    context.putImageData(new ImageData(new Uint8ClampedArray(result.pixels.buffer),result.width,result.height),0,0);
  }
  if(running && $('sound').checked && audioNode && result.audio.length) audioNode.port.postMessage(result.audio,[result.audio.buffer]);
}
async function tick(now) {
  if(disposed)return;
  animation=requestAnimationFrame(tick);
  if(!running || busy || document.hidden) return;
  const elapsed=now-last; last=now; busy=true; const current=generation;
  try { const result=await rpc('tick',elapsed); if(current===generation)draw(result); }
  catch(error) { if(current===generation) { pause(); status(String(error),true); } }
  finally { if(current===generation)busy=false; }
}
animation=requestAnimationFrame(tick);
$('pause').onclick=()=>{ if(running) { pause(); status('Paused.'); } else { resume(); status('Running.'); } };
$('step').onclick=async()=>{
  if(busy || !worker)return; busy=true; const current=generation;
  try { const result=await rpc('step'); if(current===generation)draw(result); }
  catch(error) { if(current===generation)status(String(error),true); }
  finally { if(current===generation)busy=false; }
};
$('sound').onchange=async()=>{
  const current=generation; clearAudio();
  try { if($('sound').checked) { await setupAudio(); if(worker && current===generation)await rpc('audioRate',audioContext.sampleRate); } }
  catch(error) { $('sound').checked=false; status(String(error),true); }
};
$('media').onchange=async()=>{
  if(!$('media').files.length)return;
  exampleMedia=undefined;
  if(cartridge || (fleet && slot()?.kind==='Cartridge')) { await startMachine(); return; }
  if(!worker) { status('Media ready. Start when firmware is selected.'); return; }
  const current=generation;
  try { const media=await mediaFile(); if(media && current===generation) { await rpc('load',media.slot || media.format,media.bytes); if(current===generation){rememberMedia(media);persistence.mediaChanged();status('Media loaded.');} } }
  catch(error) { if(current===generation)status(String(error),true); }
};
function controls(code) {
  if(fleet && cartridge) {
    const family=system.family;
    if(family.startsWith('nintendo-')) {
      const name={ArrowUp:'up',ArrowDown:'down',ArrowLeft:'left',ArrowRight:'right',KeyX:'a',KeyZ:'b',Enter:'start',ShiftRight:'select'}[code];
      return name ? [['button',name]] : [];
    }
    const directions={ArrowUp:'up',ArrowDown:'down',ArrowLeft:'left',ArrowRight:'right'};
    if(directions[code])return [['button',directions[code]]];
    if(code==='KeyX' || code==='KeyZ')return [['button',code==='KeyX'?'fire1':'fire2']];
    if(/^Digit[0-9]$/.test(code))return [['key',code.slice(5)]];
    if(code==='Enter')return [['key',family==='sega-game-gear'?'start':family.startsWith('sega-')?'pause':family==='atari-5200'?'start':family==='coleco-colecovision'?'1':'reset']];
    if(code==='ShiftRight')return [['key',family.startsWith('sega-') || family==='atari-5200'?'pause':family==='coleco-colecovision'?'2':'select']];
    if(code==='NumpadMultiply')return [['key','star']];
    if(code==='NumpadDivide')return [['key','hash']];
    return [];
  }
  if(fleet)return (keymap[code] || []).map(name=>['key',name]);
  if(['nes','game-boy'].includes(system.kind)) {
    const name={ArrowUp:'up',ArrowDown:'down',ArrowLeft:'left',ArrowRight:'right',KeyX:'a',KeyZ:'b',Enter:'start',ShiftRight:'select'}[code];
    return name ? [['button',name]] : [];
  }
  if(system.kind==='spectrum') return [['code',code]];
  const shared={Enter:'enter',Space:'space',ArrowDown:'down',ArrowRight:'right',Backspace:'delete',ShiftLeft:'lshift',ShiftRight:'rshift',ControlLeft:'ctrl',ControlRight:'ctrl',Comma:'comma',Period:'period',Slash:'slash',Minus:'minus',Equal:'equals'};
  const name=/^Key[A-Z]$/.test(code)?code.slice(3):/^Digit[0-9]$/.test(code)?code.slice(5):/^F([1-9]|10)$/.test(code)?code.toLowerCase():shared[code];
  const special=system.kind==='c64'
    ? {ArrowUp:['lshift','down'],ArrowLeft:['lshift','right'],F2:['lshift','f1'],F4:['lshift','f3'],F6:['lshift','f5'],F8:['lshift','f7'],Escape:['runstop'],AltLeft:['commodore'],AltRight:['commodore'],Home:['home'],Semicolon:['semicolon'],Quote:['colon'],BracketLeft:['at'],BracketRight:['asterisk'],Backquote:['leftarrow']}
    : {ArrowUp:['up'],ArrowLeft:['left'],Backspace:['backspace'],ControlLeft:['control'],ControlRight:['control'],AltLeft:['lalt'],AltRight:['ralt'],MetaLeft:['lamiga'],MetaRight:['ramiga'],Escape:['escape'],Delete:['delete'],Semicolon:['semicolon'],Quote:['apostrophe'],BracketLeft:['lbracket'],BracketRight:['rbracket'],Backslash:['backslash'],Backquote:['backquote']};
  return (special[code] || (name?[name]:[])).map(name=>['key',name]);
}
function down(id,events) { if(!running || held.has(id) || !events.length)return; held.set(id,events); send(events.map(([type,name])=>[type,name,true])); }
function up(id) {
  const events=held.get(id); if(!events)return; held.delete(id);
  const others=[...held.values()].flat().map(event=>event.join(':'));
  send(events.filter(event=>!others.includes(event.join(':'))).map(([type,name])=>[type,name,false]));
}
canvas.addEventListener('keydown',event=>{
  // Always let Tab leave the screen. Escape remains available to pointer lock.
  if(!running || event.repeat || event.code==='Tab')return;
  const events=controls(extras?.mapKey(event.code) ?? event.code); if(!events.length)return; event.preventDefault(); down(event.code,events);
});
listen(window,'keyup',event=>{ if(held.has(event.code)) { event.preventDefault(); up(event.code); } });
canvas.addEventListener('blur',release);
canvas.addEventListener('contextmenu',event=>event.preventDefault());
listen(window,'blur',()=>{ if(running) { pause(); status('Paused while the player is unfocused.'); } });
listen(document,'visibilitychange',()=>{ if(document.hidden && running)pause(); });
listen(window,'pagehide',stop);
for(const button of root.querySelectorAll('#pad button')) {
  button.setAttribute('aria-pressed','false');
  button.onpointerdown=event=>{ if(!running)return; event.preventDefault(); button.setPointerCapture(event.pointerId); down(`pad-${event.pointerId}`,controls(({up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight',a:'KeyX',b:'KeyZ',start:'Enter',select:'ShiftRight'})[button.dataset.button])); button.setAttribute('aria-pressed','true'); };
  button.onpointerup=button.onpointercancel=button.onlostpointercapture=event=>{ up(`pad-${event.pointerId}`); button.setAttribute('aria-pressed','false'); };
  button.onkeydown=event=>{ if(['Space','Enter'].includes(event.code)) { event.preventDefault(); down(`pad-key-${button.dataset.button}`,controls(({up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight',a:'KeyX',b:'KeyZ',start:'Enter',select:'ShiftRight'})[button.dataset.button])); } };
  button.onkeyup=event=>{ if(['Space','Enter'].includes(event.code)) { event.preventDefault(); up(`pad-key-${button.dataset.button}`); } };
  button.onblur=()=>up(`pad-key-${button.dataset.button}`);
}
function firmwareInput(id,label,optional=false) {
  const wrapper=document.createElement('label'); wrapper.textContent=label;
  const input=document.createElement('input'); input.type='file'; input.id=id; input.accept='.rom,.bin'; input.required=!optional; input.dataset.label=label;
  wrapper.append(input); $('firmware').append(wrapper); return input;
}
$('model').textContent=fleet ? variant.name : system.model; $('help').textContent=fleet && system.family==='sinclair-zx-spectrum' ? 'Click the screen to type. Shift is CAPS SHIFT; Control or Alt is SYMBOL SHIFT. For tapes, type the guest LOAD command, then choose Play tape below.' : system.help;
canvas.setAttribute('aria-label',`${system.name} screen. ${system.help}`);
const sizes = {'game-boy':[160,144],nes:[256,240],spectrum:[352,296],c64:[416,312],amiga:[768,576]};
[canvas.width,canvas.height]=sizes[system.kind] || [640,480];
$('media').accept=fleet ? slot()?.accept || '' : system.media; $('media').required=cartridge;
$('pad').hidden=!cartridge; $('firmware-settings').hidden=fleet ? !variant.firmware.length : cartridge || bundledFirmware;
$('choose-media').hidden=cartridge || (fleet && !slots.length);
$('begin').textContent=cartridge ? 'Choose a cartridge' : (fleet && !variant.firmware.length) || bundledFirmware ? 'Start' : 'Choose firmware';
$('choose-media').textContent=cartridge ? 'Change cartridge' : system.kind==='amiga' ? 'Choose disk' : 'Choose program or disk';
$('file-hint').textContent=cartridge ? `${system.media} · stays on your device` : bundledFirmware ? 'The 48K ROM is included with this player.' : 'Select your ROM files to start this computer.';
// Amstrad's permission asks for this acknowledgement wherever the ROM is used.
if(bundledFirmware) { $('firmware-credit').textContent='This player includes the ZX Spectrum 48K ROM. Amstrad have kindly given their permission for the redistribution of their copyrighted material but retain that copyright.'; $('firmware-credit').hidden=false; }
if(system.externalSource){$('begin').hidden=true;$('file-hint').textContent='Use Assemble & run in the source editor to start or update your program.';}
$('licence').href=asset('LICENSE.txt');
  if(!fleet && system.kind==='spectrum' && !bundledFirmware)firmwareInput('rom','48K firmware ROM (16 KiB)');
  if(!fleet && system.kind==='c64') {
    firmwareInput('kernal','KERNAL ROM'); firmwareInput('basic','BASIC ROM'); firmwareInput('chargen','Character ROM'); firmwareInput('drive','1541 drive ROM (optional)',true);
    $('firmware-note').append('Original firmware or matching ');
    const link=document.createElement('a'); link.href='https://github.com/MEGA65/open-roms'; link.target='_blank'; link.rel='noopener'; link.textContent='MEGA65 Open ROMs'; $('firmware-note').append(link,'. Open ROMs is incomplete and may not run software that relies on the original ROMs.');
  }
  if(!fleet && system.kind==='amiga') {
    firmwareInput('kickstart','Kickstart ROM, or AROS main ROM'); firmwareInput('extended','AROS extended ROM (required with AROS)',true); $('capture-mouse').hidden=false; $('mouse-hint').hidden=false;
    $('firmware-note').textContent='For AROS, select both matching 512 KiB m68k ROM images. For Kickstart, leave the extended-ROM field empty.';
  }

if(system.variants?.length>1) {
  $('variant-label').hidden=false;
  for(const entry of system.variants) { const option=document.createElement('option'); option.value=entry.id; option.textContent=entry.name; $('variant').append(option); }
  $('variant').value=variant.id;
  $('variant').onchange=()=>{writePreferences(system.id,{variant:$('variant').value});selectVariant?.($('variant').value);};
}
if(fleet) {
  if(system.family==='nintendo-game-boy')$('help').textContent='Arrow keys move; X is A, Z is B, Enter is Start and right Shift is Select. These models start after the boot-ROM sequence. Super Game Boy models emulate the handheld core; SNES borders and host features are not included.';
  if(['sinclair-zx80','sinclair-zx81'].includes(system.family))$('help').append(' Display generation is currently simplified; some software will differ from real hardware.');
  const tape=slots.find(entry=>entry.kind==='Tape');
  if(tape && ['acorn-bbc-micro','acorn-electron','commodore-c64','dragon','sinclair-zx-spectrum','sinclair-zx80','sinclair-zx81'].includes(system.family)) {
    $('tape-controls').hidden=false;
    for(const [id,playing] of [['tape-play',true],['tape-stop',false]])$(id).onclick=async()=>{try {await rpc('transport',tape.id,playing);status(playing?'Tape playing.':'Tape stopped.');}catch(error){status(String(error),true);}};
  }
  for(const firmware of variant.firmware)firmwareInput(firmware.id,firmware.display_name+(firmware.optional?' (optional)':''),firmware.optional);
  for(const entry of slots) { const option=document.createElement('option'); option.value=entry.id; option.textContent=entry.display_name; $('slot').append(option); }
  $('slot-label').hidden=slots.length<2;
  $('slot').onchange=()=>{ exampleMedia=undefined; $('media').value=''; $('media').accept=slot()?.accept || ''; persistence?.mediaChanged(); status('Choose media for this slot.'); };
  $('choose-media').textContent=cartridge?'Change cartridge':'Choose media';
  if(mouseMachine) { $('capture-mouse').hidden=false; $('mouse-hint').hidden=false; }
  if(cartridge && !system.family.startsWith('nintendo-')) {
    for(const button of root.querySelectorAll('#pad .actions button'))button.textContent=button.dataset.button==='a'?'1':'2';
    for(const button of root.querySelectorAll('#pad .system-buttons button'))button.textContent=button.dataset.button==='start'?(system.family.startsWith('sega-')?'Start / Pause':'Start / Reset'):'Select / Pause';
  }
}

function rememberMedia(media) {
 const id=media.slot || (system.kind==='amiga'?'floppy-0':system.kind==='c64'?(media.format==='prg'?'program':'drive-8'):'cartridge');
 mounted[id]={slot:id,name:media.name || `disk.${media.format}`,format:media.format};
}
function mediaExport() {
 const family=system.family || system.id;
 const id=fleet?slot()?.id:system.kind==='amiga'?'floppy-0':'drive-8';
 const media=mounted[id];
 return media && ((family==='commodore-amiga' && media.format==='adf') || (family==='commodore-c64' && ['d64','d81'].includes(media.format)) || (family==='dragon' && media.format==='vdk'))?media:null;
}
async function capture() {
 if(!worker || !sessionSeed)throw new Error('Start a machine before saving.');
 pause();const current=generation;
 const state=await rpc('save');
 if(disposed || current!==generation)throw new Error('The machine changed before it could be saved.');
 return {system:system.id,variant:variant?.id || system.model,date:new Date().toISOString(),boot:sessionSeed,state,mounted};
}
async function restore(save) {
 const model=variant?.id || system.model;
 if(save.system!==system.id || save.variant!==model || !Array.isArray(save.boot) || save.boot[0]!==system.kind || (fleet ? save.boot[4]?.family!==system.family || save.boot[4]?.id!==model : Boolean(save.boot[4])))throw new Error('This save is for another system or model.');
 pause();const current=generation;
 const candidate=new Worker(asset('worker.js'),{type:'module'});
 let candidateSerial=0, timer;
 const ask=(command,...args)=>new Promise((resolve,reject)=>{
  const id=++candidateSerial;
  timer=setTimeout(()=>reject(new Error('The saved machine did not respond.')),60000);
  candidate.onmessage=({data})=>{if(data.id!==id)return;clearTimeout(timer);data.error?reject(new Error(data.error)):resolve(data.result);};
  candidate.onerror=event=>{clearTimeout(timer);reject(new Error(event.message || 'Saved machine failed to load.'));};
  candidate.postMessage({id,command,args});
 });
 let installed=false;
 try {
  const bootArgs=[...save.boot];bootArgs[3]=audioContext?.sampleRate || 48000;
  const boot=await ask('boot',...bootArgs);await ask('restore',save.state);
  const frame=await ask('step');
  if(disposed || current!==generation)throw new Error('The selected machine changed while restoring.');
  stop();worker=candidate;installed=true;serial=Math.max(serial,candidateSerial);
  worker.onmessage=({data})=>{const request=pending.get(data.id);if(!request)return;pending.delete(data.id);clearTimeout(request.timer);data.error?request.reject(new Error(data.error)):request.resolve(data.result);};
  const restoredGeneration=generation;
  worker.onerror=event=>{if(restoredGeneration===generation){stop();status(event.message || 'Emulator failed.',true);}};
  sessionSeed=bootArgs;mounted=save.mounted || {};keymap=boot.keymap || {};
  $('pause').hidden=$('start').hidden=false;$('pause').disabled=false;$('standby').hidden=true;$('start').textContent='Restart';
  $('choose-media').hidden=fleet && !slots.length;$('firmware-settings').open=false;
  draw(frame);resume();persistence.started();status('Saved progress restored.');
 }finally{clearTimeout(timer);if(!installed)candidate.terminate();}
}
persistence=sessionControls({root,system,model:variant?.id || system.model,capture,restore,rpc,pause,mediaExport});

async function loadExample(example) {
 const current=generation;
 try {
  const url=new URL(example.url,asset('index.html'));
  if(url.origin!==new URL(asset('index.html')).origin)throw new Error('Examples must come from this site.');
  status('Loading example…');
  const response=await fetch(url);
  if(!response.ok)throw new Error('The example could not be downloaded. Try again or choose a local file.');
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(!bytes.length || bytes.length>32*1024*1024)throw new Error('The example is empty or too large.');
  if(disposed || current!==generation)return;
  await loadMedia({name:decodeURIComponent(url.pathname.split('/').pop()),bytes});
 }catch(error){if(!disposed)status(error.message,true);}
}
// Shared entry point for lesson assemblers and fetched examples. Bytes remain
// on the device; no FileList mutation or knowledge of the player's DOM is needed.
async function loadMedia({name,bytes}) {
 if(disposed)throw new Error('The player was closed.');
 if($('start').disabled)throw new Error('Wait for the current machine to finish starting.');
 if(typeof name!=='string' || !(bytes instanceof Uint8Array) || !bytes.length || bytes.length>32*1024*1024)throw new Error('Supply a named program smaller than 32 MiB.');
 const format=name.split('.').pop().toLowerCase();
 const target=fleet?slots.find(s=>s.accept.split(',').includes(`.${format}`)):null;
 if(!(fleet?target:system.media.split(',').includes(`.${format}`)))throw new Error('This program does not match the selected machine.');
 if(target)$('slot').value=target.id;
 exampleMedia={name,format,slot:target?(target.id==='snapshot'?format:target.id):undefined,bytes:bytes.slice(),autorun:format==='prg' && system.family==='commodore-c64' && bytes[0]===1 && bytes[1]===8?'run':undefined};
 return Boolean(await startMachine(true));
}
extras=playerExtras({root,system,model:variant?.id || system.model,asset,pause,release,setVolume:value=>{volume=value;if(gainNode)gainNode.gain.value=value;},loadExample});

const destroy = () => {
  disposed=true; extras.destroy(); persistence.destroy(); stop(); mouse.destroy(); listeners.abort(); cancelAnimationFrame(animation);
  audioNode?.disconnect(); audioContext?.close().catch(()=>{});
};
return Object.assign(destroy,{loadMedia,pause:()=>{if(running){pause();status('Paused.');}}});
}
