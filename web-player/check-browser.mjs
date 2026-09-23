// Run explicitly for browser QA. Playwright is supplied by the consuming site.
// No fixture is published or uploaded. Optional real software stays local.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import path from 'node:path';
import {encodeSave,decodeSave} from './src/save-file.js';
const {chromium,firefox,webkit}=await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE));
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const base=process.env.PLAYER_TEST_URL || 'http://127.0.0.1:8768/';
const fixture=name=>path.join(root,'test-data',name);
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
for(const name of (process.env.PLAYER_BROWSERS || 'chrome,firefox,webkit').split(',')) {
 const browser=await (name==='chrome'?chromium:name==='firefox'?firefox:webkit).launch(name==='chrome'?{channel:'chrome',headless:true}:{headless:process.env.PLAYER_HEADED!=='1',timeout:30000});
 const context=await browser.newContext({acceptDownloads:true});
 const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 const player=()=>page.locator('emu198x-player');
 const control=id=>player().locator(`#${id}`);
 const waitText=async(id,text)=>{await control(id).filter({hasText:text}).waitFor({timeout:60000,state:'attached'});};
 const go=async system=>{await page.goto(`${base}?system=${system}`);await control('begin').waitFor();};
 try {
  console.log(`${name}: checking saved sessions and fullscreen`);
  await go('nintendo-game-boy');
  await control('media').setInputFiles(fixture('synthetic-cartridges/nintendo-game-boy-logo.gb'));
  await waitText('status','Running');await control('sound').check();await delay(400);
  await control('save-settings').locator('summary').click();
  await control('save-device').click();await waitText('save-status','Device save:');
  const downloaded=page.waitForEvent('download');await control('export-save').click();
  const download=await downloaded;const exported=readFileSync(await download.path());assert(exported.length>1000);
  await page.reload();await control('begin').waitFor();
  await control('save-settings').locator('summary').click();
  await control('resume-device').click();await waitText('save-status','Saved progress restored.');
  await control('pause').click();await control('forget-save').click();await waitText('save-status','Device save deleted.');
  await control('save-file').setInputFiles({name:'saved.emu198x',mimeType:'application/octet-stream',buffer:exported});await waitText('save-status','Save file restored.');
  await control('pause').click();
  // A valid container with bad runtime state must fail in a replacement
  // worker without destroying the paused machine the visitor was using.
  const badState=await decodeSave(new Blob([exported]),'nintendo-game-boy','dmg');badState.state=new Uint8Array([1,2,3]);
  const badStateFile=Buffer.from(await (await encodeSave(badState)).arrayBuffer());
  await control('save-file').setInputFiles({name:'bad-state.emu198x',mimeType:'application/octet-stream',buffer:badStateFile});
  await waitText('save-status',/snapshot|state|decode|invalid/i);
  assert.equal(await control('pause').isEnabled(),true,'Failed runtime restore must retain the existing worker');
  const damaged=Buffer.from(exported);damaged[damaged.length-1]^=1;
  await control('save-file').setInputFiles({name:'damaged.emu198x',mimeType:'application/octet-stream',buffer:damaged});await waitText('save-status','damaged or incomplete');
  assert.equal(await control('pause').isEnabled(),true,'Damaged import must retain the old machine');
  await control('pause').click();await waitText('status','Running');
  let fullscreen='unavailable';
  if(await control('fullscreen').isVisible()) {
   await control('fullscreen').click();await page.waitForFunction(()=>Boolean(document.fullscreenElement));
   await control('fullscreen').click();await page.waitForFunction(()=>!document.fullscreenElement);fullscreen='enter/exit passed';
  }
  console.log(`${name}: checking firmware memory`);
  await go('acorn-atom');await control('begin').click();
  await control('acorn-atom-rom').setInputFiles(fixture('synthetic-firmware/acorn-atom.rom'));
  await control('remember-firmware').check();await control('boot').click();await waitText('status','Running');
  await page.reload();await waitText('firmware-storage-status','Remembered:');await control('begin').click();await waitText('status','Running');
  await control('firmware-settings').locator('summary').click();await control('forget-firmware').click();await waitText('firmware-storage-status','removed');
  await page.reload();await control('begin').click();await waitText('status','Choose');
  console.log(`${name}: checking local software`);
  if(process.env.PLAYER_SMS_ROM) {
   await go('sega-master-system');await control('media').setInputFiles(process.env.PLAYER_SMS_ROM);await waitText('status','Running');await delay(1500);
   await control('screen').press('Enter');await control('screen').press('KeyX');
   await control('save-settings').locator('summary').click();await control('save-device').click();await waitText('save-status','Device save:');
   await control('resume-device').click();await waitText('save-status','Saved progress restored.');
  }
  if(process.env.PLAYER_AMIGA_ROM && process.env.PLAYER_AMIGA_DISK) {
   await go('commodore-amiga');await control('media').setInputFiles(process.env.PLAYER_AMIGA_DISK);await control('begin').click();
   await control('kickstart').setInputFiles(process.env.PLAYER_AMIGA_ROM);await control('boot').click();await waitText('status','Running');await delay(3000);
   await control('save-settings').locator('summary').click();
   const diskDownload=page.waitForEvent('download');await control('export-media').click();const disk=await diskDownload;assert.equal(readFileSync(await disk.path()).length,readFileSync(process.env.PLAYER_AMIGA_DISK).length);
   await control('save-device').click();await waitText('save-status','Device save:');await control('resume-device').click();await waitText('save-status','Saved progress restored.');
  }
  assert.deepEqual(errors,[]);console.log(`${name}: save/export/import, damaged-file recovery, persistent resume, remembered firmware/forget, fullscreen ${fullscreen}${process.env.PLAYER_SMS_ROM?', real SMS software':''}${process.env.PLAYER_AMIGA_DISK?', Amiga disk export/resume':''}`);
 } finally {await context.close();await browser.close();}
}
