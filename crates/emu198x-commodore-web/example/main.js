import {amigaMouse} from './mouse.js';
const $ = id => document.getElementById(id);
const canvas = $('screen'), context = canvas.getContext('2d');
let worker, serial = 0, pending = new Map(), running = false, busy = false;
let last = 0, frameMs = 20, audioContext, audioNode, activeKind, generation = 0;
let frames = 0, compute = 0, measuredAt = 0;
const held = new Map();
const mouse = amigaMouse({canvas, button:$('capture-mouse'), hint:$('mouse-hint'),
  send:events => {
    const current = generation;
    if (worker) rpc('input',events).catch(error=>{ if (generation === current) status(error,true); });
  },
  releaseKeys:release,
});
function status(message, error = false) { $('status').textContent = message; $('status').classList.toggle('error', error); }
function rpc(command, ...args) {
  if (!worker) return Promise.reject(new Error('Start a machine first'));
  const id = ++serial;
  return new Promise((resolve, reject) => { pending.set(id, {resolve, reject}); worker.postMessage({id, command, args}); });
}
function release() {
  if (worker && held.size) rpc('input', [...held.values()].flat().map(([type, name]) => [type, name, false])).catch(error => status(error, true));
  held.clear();
}
function clearAudio() { audioNode?.port.postMessage('clear'); }
function stop() {
  mouse.setEnabled(false);
  generation++; running = false; busy = false; held.clear(); worker?.terminate(); worker = undefined;
  for (const {reject} of pending.values()) reject(new Error('Machine restarted'));
  pending.clear(); clearAudio();
  for (const id of ['pause','step','benchmark','media']) $(id).disabled = true;
}
function pause() {
  mouse.setEnabled(false);
  release(); running = false; last = 0; clearAudio();
  $('pause').textContent = 'Resume'; $('step').disabled = !worker;
}
function resume() {
  running = true; last = performance.now(); frames = 0; compute = 0; measuredAt = last;
  mouse.setEnabled(activeKind === 'amiga');
  $('pause').textContent = 'Pause'; $('step').disabled = true; clearAudio(); canvas.focus();
}
async function audioSetup() {
  if (!audioContext) {
    audioContext = new AudioContext();
    await audioContext.audioWorklet.addModule('./audio.js');
    audioNode = new AudioWorkletNode(audioContext, 'emulator-audio', {outputChannelCount:[2]});
    audioNode.connect(audioContext.destination);
  }
  await audioContext.resume();
}
async function file(id, optional = false) {
  const selected = $(id).files[0];
  if (!selected && !optional) throw new Error(`Select the ${id} ROM`);
  return selected ? new Uint8Array(await selected.arrayBuffer()) : new Uint8Array();
}
async function openRom(name) {
  const response = await fetch(`./roms/${name}.rom`);
  if (!response.ok) throw new Error('Open ROMs is not installed. Run python3 scripts/fetch-open-roms.py from the prototype directory.');
  return new Uint8Array(await response.arrayBuffer());
}
$('start').onclick = async () => {
  stop(); const current = generation; $('start').disabled = true; status('Preparing firmware…');
  try {
    if ($('sound').checked) await audioSetup();
    let roms;
    activeKind = $('kind').value;
    if (activeKind === 'c64') {
      const [kernal,basic,chargen,drive] = await Promise.all($('firmware').value === 'open'
        ? [openRom('kernal_generic'), openRom('basic_generic'), openRom('chargen_openroms'), file('drive',true)]
        : [file('kernal'),file('basic'),file('chargen'),file('drive',true)]);
      roms = {kernal,basic,chargen,drive};
    } else if ($('amiga-firmware').value === 'aros') {
      roms = {kickstart:await file('aros-main'),extended:await file('aros-ext')};
    } else roms = {kickstart:await file('kickstart')};
    if (generation !== current) return;
    worker = new Worker('./worker.js', {type:'module'});
    worker.onmessage = ({data}) => {
      const request = pending.get(data.id); if (!request) return;
      pending.delete(data.id); data.error ? request.reject(new Error(data.error)) : request.resolve(data.result);
    };
    worker.onerror = event => {
      const message = event.message || 'Worker failed';
      stop(); status(message, true);
    };
    status('Starting emulator…');
    ({frameMs} = await rpc('boot',activeKind,roms,audioContext?.sampleRate || 48000));
    if (generation !== current) return;
    for (const id of ['pause','benchmark','media']) $(id).disabled = false;
    resume(); status(activeKind === 'amiga'
      ? 'Running. Click the screen to capture the Amiga mouse; Escape releases it.'
      : 'Running. Click the screen to use the keyboard.');
  } catch (error) { if (generation === current) { stop(); status(String(error),true); } }
  finally { $('start').disabled = false; }
};
function draw(result) {
  if (result.pixels.length) {
    if (canvas.width !== result.width || canvas.height !== result.height) { canvas.width = result.width; canvas.height = result.height; }
    context.putImageData(new ImageData(new Uint8ClampedArray(result.pixels.buffer),result.width,result.height),0,0);
  }
  if (running && $('sound').checked && audioNode && result.audio.length) audioNode.port.postMessage(result.audio,[result.audio.buffer]);
}
async function tick(now) {
  requestAnimationFrame(tick);
  if (!running || busy || !worker || document.hidden) return;
  const elapsed = now - last; last = now; busy = true; const current = generation;
  try {
    const result = await rpc('tick',elapsed);
    if (generation !== current) return;
    draw(result); frames += result.frames; compute += result.elapsed;
    if (now - measuredAt > 1000) {
      const realtime = frames * frameMs / (now - measuredAt) * 100;
      $('metrics').textContent = `${realtime.toFixed(0)}% real time · ${frames ? (compute/frames).toFixed(2) : '0'} ms emulation/frame · ${frameMs.toFixed(2)} ms budget`;
      frames = 0; compute = 0; measuredAt = now;
    }
  } catch (error) { if (generation === current) { pause(); status(String(error),true); } }
  finally { if (generation === current) busy = false; }
}
requestAnimationFrame(tick);
$('pause').onclick = () => running ? pause() : resume();
$('step').onclick = async () => {
  if (busy) return; busy = true; const current = generation;
  try { const result = await rpc('step'); if (generation === current) draw(result); }
  catch(error) { if (generation === current) status(String(error),true); }
  finally { if (generation === current) busy = false; }
};
$('benchmark').onclick = async () => {
  if (busy) { status('A frame is in progress; pause, then measure.'); return; }
  pause(); busy = true; $('benchmark').disabled = true; const current = generation;
  status('Measuring 120 frames; this advances the machine.');
  try {
    const result = await rpc('benchmark');
    if (generation !== current) return;
    $('metrics').textContent = `120 frames: mean ${result.mean.toFixed(2)} ms · p95 ${result.p95.toFixed(2)} ms · ${(result.frameMs/result.mean).toFixed(2)}× real time (emulation only)`;
    const frame = await rpc('step');
    if (generation === current) { draw(frame); status('Measurement complete. Resume to continue.'); }
  } catch(error) { if (generation === current) status(String(error),true); }
  finally { if (generation === current) { busy = false; $('benchmark').disabled = !worker; } }
};
$('sound').onchange = async () => { try { clearAudio(); if ($('sound').checked) { await audioSetup(); if(worker) await rpc('audioRate',audioContext.sampleRate); } } catch(error) { status(String(error),true); $('sound').checked=false; } };
$('media').onchange = async () => {
  const selected = $('media').files[0]; if (!selected) return;
  const current = generation;
  try {
    const bytes = new Uint8Array(await selected.arrayBuffer());
    if (generation !== current) return;
    await rpc('load',selected.name.split('.').pop().toLowerCase(),bytes);
    if (generation === current) status(`Loaded ${selected.name}.`);
  }
  catch(error) { if (generation === current) status(String(error),true); }
  finally { if (generation === current) $('media').value=''; }
};
function choose() { stop(); $('c64').hidden = $('kind').value !== 'c64'; $('amiga').hidden = $('kind').value !== 'amiga'; status('Choose firmware, then start.'); }
$('kind').onchange = choose;
$('firmware').onchange = () => { $('own').hidden = $('firmware').value !== 'own'; $('open-note').hidden = !$('own').hidden; };
$('amiga-firmware').onchange = () => {
  const aros = $('amiga-firmware').value === 'aros';
  $('kickstart-fields').hidden = aros; $('aros-fields').hidden = !aros;
};
const shared = {Enter:'enter',Space:'space',ArrowDown:'down',ArrowRight:'right',Backspace:'delete',
  ShiftLeft:'lshift',ShiftRight:'rshift',ControlLeft:'ctrl',ControlRight:'ctrl',Comma:'comma',Period:'period',Slash:'slash',Minus:'minus',Equal:'equals'};
function keys(code) {
  if ($('joystick').checked && ({ArrowUp:1,ArrowDown:1,ArrowLeft:1,ArrowRight:1,Space:1})[code]) return [['joystick',code==='Space'?'fire':code.slice(5).toLowerCase()]];
  const name = /^Key[A-Z]$/.test(code) ? code.slice(3) : /^Digit[0-9]$/.test(code) ? code.slice(5) : /^F([1-9]|10)$/.test(code) ? code.toLowerCase() : shared[code];
  const special = activeKind === 'c64'
    ? {ArrowUp:['lshift','down'],ArrowLeft:['lshift','right'],F2:['lshift','f1'],F4:['lshift','f3'],F6:['lshift','f5'],F8:['lshift','f7'],Escape:['runstop'],AltLeft:['commodore'],AltRight:['commodore'],Home:['home'],Semicolon:['semicolon'],Quote:['colon'],BracketLeft:['at'],BracketRight:['asterisk'],Backquote:['leftarrow']}
    : {ArrowUp:['up'],ArrowLeft:['left'],Backspace:['backspace'],ControlLeft:['control'],ControlRight:['control'],AltLeft:['lalt'],AltRight:['ralt'],MetaLeft:['lamiga'],MetaRight:['ramiga'],Escape:['escape'],Tab:['tab'],Delete:['delete'],Semicolon:['semicolon'],Quote:['apostrophe'],BracketLeft:['lbracket'],BracketRight:['rbracket'],Backslash:['backslash'],Backquote:['backquote']};
  return (special[code] || (name ? [name] : [])).map(n=>['key',n]);
}
canvas.addEventListener('keydown',event=>{
  if (!running || event.repeat || held.has(event.code)) return;
  const controls=keys(event.code); if(!controls.length)return; event.preventDefault(); held.set(event.code,controls);
  rpc('input',controls.map(([type,name])=>[type,name,true])).catch(error=>status(error,true));
});
window.addEventListener('keyup',event=>{
  const controls=held.get(event.code); if(!controls)return; event.preventDefault(); held.delete(event.code);
  // Keep a shared modifier down until all host keys using it have been released.
  const stillHeld=[...held.values()].flat().map(x=>x.join(':'));
  rpc('input',[...controls].reverse().filter(x=>!stillHeld.includes(x.join(':'))).map(([type,name])=>[type,name,false])).catch(error=>status(error,true));
});
canvas.addEventListener('blur',release);
window.addEventListener('blur',()=>{ if(running) pause(); });
document.addEventListener('visibilitychange',()=>{ if(document.hidden && running)pause(); });
$('joystick').onchange=release;
canvas.addEventListener('contextmenu',event=>event.preventDefault());
