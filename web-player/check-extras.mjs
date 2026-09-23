// Explicit browser QA: run against the built shared player.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {chromium,webkit}=await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE));
const base=process.env.PLAYER_TEST_URL || 'http://127.0.0.1:8768/';
for(const name of (process.env.PLAYER_BROWSERS || 'chrome,webkit').split(',')) {
 const browser=await (name==='chrome'?chromium:webkit).launch(name==='chrome'?{channel:'chrome',headless:true}:{headless:true});
 const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 const control=id=>page.locator('emu198x-player').locator('#'+id);
 const go=async system=>{await page.goto(base+'?system='+system);await control('begin').waitFor();};
 const running=()=>control('status').filter({hasText:'Running'}).waitFor({timeout:60000});
 try {
  for(const family of ['nintendo-game-boy','nintendo-nes','atari-2600','atari-5200','atari-7800','sega-master-system','sega-game-gear','sega-sg-1000']) {
   console.log(`${name}: demo ${family}`);await go(family);await control('run-example').click();await running();
   assert.equal(await control('controls-summary').isVisible(),true);
   await control('pause').click();
  }
  await go('coleco-colecovision');
  await control('media').setInputFiles({name:'test.col',mimeType:'application/octet-stream',buffer:Buffer.alloc(32768)});
  await control('status').filter({hasText:'Choose'}).waitFor();
  assert.equal(await control('error-help').isVisible(),true);
  await go('nintendo-game-boy');
  await page.getByText('Preferences',{exact:true}).click();
  await control('volume').fill('25');await control('volume').dispatchEvent('input');await control('display').selectOption('smooth');
  const binding=page.locator('[data-action="a"]');await binding.click();await binding.press('KeyC');
  assert((await control('controls-summary').textContent()).includes('C'));
  await page.reload();await control('begin').waitFor();
  assert.equal(await control('volume').inputValue(),'25');assert.equal(await control('display').inputValue(),'smooth');
  assert((await control('controls-summary').textContent()).includes('C'));
  await control('run-example').click();await running();
  // Observe the real worker input call to verify remapping affects input.
  await page.evaluate(()=>{window.inputEvents=[];const original=Worker.prototype.postMessage;Worker.prototype.postMessage=function(message,...rest){if(message.command==='input')window.inputEvents.push(...message.args[0]);return original.call(this,message,...rest);};});
  await control('screen').press('KeyC');
  assert((await page.evaluate(()=>window.inputEvents)).some(event=>event[0]==='button' && event[1]==='a' && event[2]));
  await page.getByText('Report a problem',{exact:true}).click();await control('copy-report').click();
  const report=await control('report-text').inputValue();assert(report.includes('System: nintendo-game-boy'));assert(report.includes('Build: '));assert(!report.includes('.gb'));assert(!report.includes('http://'));
  await control('variant').selectOption('mgb');await page.reload();await control('begin').waitFor();assert.equal(await control('variant').inputValue(),'mgb');
  // Explicit lesson models take precedence over remembered system-page models.
  await page.evaluate(()=>{const old=document.querySelector('emu198x-player');const player=document.createElement('emu198x-player');player.setAttribute('system','nintendo-game-boy');player.setAttribute('variant','dmg');player.setAttribute('src','/demos/nintendo-game-boy-logo.gb');old.replaceWith(player);});
  await control('run-example').filter({hasText:'Run this lesson'}).waitFor();assert.equal(await control('variant').inputValue(),'dmg');await control('run-example').click();await running();
  // A missing example must show an actionable error and leave the worker alive.
  await page.route('**/demos/nintendo-game-boy-logo.gb',route=>route.fulfill({status:404,body:'missing'}));
  await control('run-example').click();await control('status').filter({hasText:'could not be downloaded'}).waitFor();assert.equal(await control('pause').isEnabled(),true);
  assert.deepEqual(errors,[]);console.log(`${name}: eight demos, preferences, remapped input, diagnostics, model precedence and failed-download recovery passed.`);
 }finally{await browser.close();}
}
