import {readPreferences,writePreferences} from './preferences.js';
export function playerExtras({root,system,model,asset,pause,release,setVolume,loadExample}) {
 const $=id=>root.getElementById(id), prefs=readPreferences(system.id);
 let disposed=false, revision='unavailable', mapping={};
 const save=changes=>{if(!writePreferences(system.id,changes))$('preferences-status').textContent='Preferences could not be remembered in this browser. Your current choices still work.';};
 $('volume').value=Number.isFinite(prefs.volume)?Math.max(0,Math.min(100,prefs.volume)):70;
 const volume=()=>{setVolume(Number($('volume').value)/100);$('volume-value').textContent=`${$('volume').value}%`;};
 volume();$('volume').oninput=()=>{volume();save({volume:Number($('volume').value)});};
 $('display').value=prefs.display==='smooth'?'smooth':'pixel';
 const display=()=>$('screen').style.imageRendering=$('display').value==='smooth'?'auto':'pixelated';
 display();$('display').onchange=()=>{display();save({display:$('display').value});};
 $('sound').checked=prefs.sound===true;
 $('sound').addEventListener('change',()=>save({sound:$('sound').checked}));
 const roles=system.family?.startsWith('nintendo-')?['Start','Select']:system.family==='sega-game-gear'?['Start','Pause']:system.family?.startsWith('sega-')?['Pause','Pause']:system.family==='coleco-colecovision'?['Keypad 1','Keypad 2']:system.family==='atari-5200'?['Start','Pause']:['Reset','Select'];
 const keyLabel=code=>({ArrowUp:'↑',ArrowDown:'↓',ArrowLeft:'←',ArrowRight:'→',ShiftRight:'Right Shift',ShiftLeft:'Left Shift'})[code] || code.replace(/^(Key|Digit)/,'');
 const canonical={up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight',a:'KeyX',b:'KeyZ',start:'Enter',select:'ShiftRight'};
 if(system.console) {
  for(const [action,label] of [['start',roles[0]],['select',roles[1]]])root.querySelector(`#pad [data-button=${action}]`).textContent=label;
  $('input-settings').hidden=false;
  const used=new Set();
  for(const [action,code] of Object.entries(canonical)) {
   const wanted=prefs.keys?.[action];
   // Reject malformed, reserved and duplicate stored bindings.
   mapping[action]=typeof wanted==='string' && /^(Key[A-Z]|Digit[0-9]|Arrow(Up|Down|Left|Right)|Space|Enter|Shift(Left|Right))$/.test(wanted) && !used.has(wanted)?wanted:code;
   used.add(mapping[action]);
   const label=document.createElement('label');label.textContent=({a:'Action 1 / A',b:'Action 2 / B',start:roles[0],select:roles[1]})[action] || action;
   const button=document.createElement('button');button.type='button';button.textContent=keyLabel(mapping[action]);button.dataset.action=action;
   button.onclick=()=>{pause();button.textContent='Press a key…';button.dataset.waiting='true';};
   button.onblur=()=>{delete button.dataset.waiting;button.textContent=keyLabel(mapping[action]);};
   button.onkeydown=event=>{
    if(!button.dataset.waiting || event.code==='Tab')return;
    event.preventDefault();
    if(event.code==='Escape'){button.blur();return;}
    if(!/^(Key[A-Z]|Digit[0-9]|Arrow(Up|Down|Left|Right)|Space|Enter|Shift(Left|Right))$/.test(event.code))return;
    const other=Object.keys(mapping).find(key=>key!==action && mapping[key]===event.code);
    if(other){$('preferences-status').textContent='That key is already assigned. Choose another key.';return;}
    release();mapping[action]=event.code;delete button.dataset.waiting;button.textContent=keyLabel(event.code);save({keys:mapping});$('preferences-status').textContent='Controls remembered.';legend();
   };
   label.append(button);$('key-bindings').append(label);
  }
 }
 function legend() {
  $('controls-summary').textContent=system.console ? `Move: ${keyLabel(mapping.up)}, ${keyLabel(mapping.down)}, ${keyLabel(mapping.left)}, ${keyLabel(mapping.right)}. Actions: ${keyLabel(mapping.a)}, ${keyLabel(mapping.b)}. ${roles[0]}: ${keyLabel(mapping.start)}. ${roles[1]}: ${keyLabel(mapping.select)}. Click the screen to play; Tab leaves it.` : $('help').textContent+' Tab leaves the screen.';
 }
 legend();
 fetch(asset('build.json')).then(response=>response.ok?response.json():null).then(build=>{if(!disposed && build)revision=build.revision+(build.modified?' (modified)':'');}).catch(()=>{});
 $('report-link').href='https://github.com/emu198x/emu198x/issues/new';
 $('copy-report').onclick=async()=>{
  const report=`Emu198x browser report\nSystem: ${system.id}\nModel: ${model}\nBuild: ${revision}\nBrowser: ${navigator.userAgent}\nWebAssembly: ${typeof WebAssembly!=='undefined'}\nAudioWorklet: ${typeof AudioWorkletNode!=='undefined'}\n\nWhat happened:\n\nSteps to reproduce:\n\nExpected behaviour:\n`;
  // No page URL, media names, firmware names, saves or error strings are copied.
  $('report-text').value=report;$('report-text').hidden=false;
  try {await navigator.clipboard.writeText(report);$('report-status').textContent='Details copied. Review them and add what happened before opening an issue.';}
  catch {$('report-text').focus();$('report-text').select();$('report-status').textContent='Select and copy these details, then add what happened.';}
 };
 const example=system.example || system.demo;
 if(example) {
  $('run-example').hidden=false;$('run-example').textContent=system.example?'Run this lesson':'Try the demo';
  $('example-description').textContent=example.description || example.title;
  if(example.source){$('example-source').href=example.source;$('example-source').hidden=false;}
  $('run-example').onclick=async()=>{
   $('run-example').disabled=true;
   try {await loadExample(example);}
   finally {if(!disposed)$('run-example').disabled=false;}
  };
 }
 return {mapKey:code=>Object.entries(mapping).find(([,bound])=>bound===code)?.[0] ? canonical[Object.entries(mapping).find(([,bound])=>bound===code)[0]] : Object.values(canonical).includes(code) && system.console ? '' : code, destroy(){disposed=true;}};
}
