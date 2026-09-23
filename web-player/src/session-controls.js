import {deviceStore} from './storage.js';
import {encodeSave,decodeSave} from './save-file.js';
export function sessionControls({root,system,model,capture,restore,rpc,pause,mediaExport}) {
 const $=id=>root.getElementById(id), key=`${system.id}/${model}`;
 let disposed=false, remembered={}, storageReady, working=false;
 const say=message=>{if(!disposed)$('save-status').textContent=message;};
 const firmwareInputs=()=>[...root.querySelectorAll('#firmware input[type=file]')];
 function firmwareMessage() {
  const names=Object.values(remembered).map(entry=>entry.name);
  $('firmware-storage-status').textContent=names.length?`Remembered: ${names.join(', ')}. Choose another file to replace it.`:'';
  $('forget-firmware').hidden=!names.length;
  if(!system.console && names.length && firmwareInputs().filter(input=>input.required).every(input=>input.files.length || remembered[input.id]?.bytes?.length)) { $('begin').textContent='Start'; $('file-hint').textContent='Using firmware remembered on this device.'; }
  else if(!system.console && !names.length){$('begin').textContent='Choose firmware';$('file-hint').textContent='Select your ROM files to start this computer.';}
 }
 storageReady=deviceStore.get('firmware',key).then(value=>{
  if(disposed)return;remembered=value || {};$('remember-firmware').checked=Object.keys(remembered).length>0;firmwareMessage();
 }).catch(error=>{if(!disposed)$('firmware-storage-status').textContent=error.message;});
 async function refreshSave() {
  try {const saved=await deviceStore.get('saves',key);if(disposed)return;
   $('resume-device').disabled=!saved;$('forget-save').hidden=!saved;
   if(saved)say(`Device save: ${saved.boot?.[2]?.name || system.name} — ${new Date(saved.date).toLocaleString()}. Saving again replaces this copy.`);
  }catch(error){say(error.message);}
 }
 refreshSave();
 $('forget-firmware').onclick=async()=>{try{await storageReady;await deviceStore.delete('firmware',key);if(disposed)return;remembered={};$('remember-firmware').checked=false;firmwareMessage();$('firmware-storage-status').textContent='Remembered firmware removed.';}catch(error){$('firmware-storage-status').textContent=error.message;}};
 $('remember-firmware').onchange=async()=>{if(!$('remember-firmware').checked)await $('forget-firmware').onclick();};
 const download=(blob,name)=>{const url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download=name;root.append(link);link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);};
 async function action(run) {if(working)return;working=true;try{await run();}catch(error){say(error.message || String(error));}finally{working=false;}}
 $('save-device').onclick=()=>action(async()=>{const save=await capture();await deviceStore.put('saves',key,save);await refreshSave();});
 $('export-save').onclick=()=>action(async()=>{const save=await capture();download(await encodeSave(save),`${system.id}-${model}.emu198x`);say('Save file exported. Keep it to resume this session.');});
 $('resume-device').onclick=()=>action(async()=>{const save=await deviceStore.get('saves',key);if(!save)throw new Error('No device save was found.');await restore(save);say('Saved progress restored.');});
 $('forget-save').onclick=()=>action(async()=>{await deviceStore.delete('saves',key);await refreshSave();say('Device save deleted.');});
 $('import-save').onclick=()=>$('save-file').click();
 $('save-file').onchange=()=>action(async()=>{const file=$('save-file').files[0];if(!file)return;try{const save=await decodeSave(file,system.id,model);await restore(save);say('Save file restored.');}finally{$('save-file').value='';}});
 $('export-media').onclick=()=>action(async()=>{const media=mediaExport();if(!media)throw new Error('Load an exportable disk first.');pause();const bytes=await rpc('exportMedia',media.slot);download(new Blob([bytes]),`copy-${media.name}`);say('Current disk image exported as a new file.');});
 const host=root.host;
 if(host?.requestFullscreen && document.fullscreenEnabled) {
  $('fullscreen').hidden=false;
  $('fullscreen').onclick=()=>action(async()=>{if(document.fullscreenElement===host)await document.exitFullscreen();else await host.requestFullscreen();});
  const update=()=>{if(!disposed)$('fullscreen').textContent=document.fullscreenElement===host?'Exit fullscreen':'Fullscreen';};
  document.addEventListener('fullscreenchange',update);
  host._playerFullscreenCleanup=()=>document.removeEventListener('fullscreenchange',update);
 }
 return {
  ready:()=>storageReady,
  hasFirmware:input=>Boolean(input.files.length || remembered[input.id]?.bytes?.length),
  readFirmware:async(input,read)=>input.files.length?read(input):remembered[input.id]?.bytes || read(input),
  async remember(roms) {
   if(!$('remember-firmware').checked)return;
   const value=Object.fromEntries(firmwareInputs().filter(input=>roms[input.id]?.length).map(input=>[input.id,{bytes:roms[input.id],name:input.files[0]?.name || remembered[input.id]?.name || input.dataset.label}]));
   try{await deviceStore.put('firmware',key,value);if(!disposed){remembered=value;firmwareMessage();}}catch(error){if(!disposed)$('firmware-storage-status').textContent=error.message;}
  },
  started(){for(const id of ['save-device','export-save'])$(id).disabled=false;this.mediaChanged();},
  stopped(){for(const id of ['save-device','export-save'])$(id).disabled=true;$('export-media').hidden=true;},
  mediaChanged(){$('export-media').hidden=!mediaExport();},
  destroy(){disposed=true;host?._playerFullscreenCleanup?.();},
 };
}
