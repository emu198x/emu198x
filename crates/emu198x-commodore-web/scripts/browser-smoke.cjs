// PLAYWRIGHT_PACKAGE=... node scripts/browser-smoke.cjs KICKSTART [OUTPUT_DIR]
// Requires fetch-open-roms.py and the local prototype server. Firmware is selected
// through file inputs; it is never copied into the server's document root.
const {chromium}=require(process.env.PLAYWRIGHT_PACKAGE || 'playwright');
const {mkdirSync}=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const kickstart=process.argv[2], output=process.argv[3];
if(!kickstart)throw new Error('Pass a local A500 Kickstart ROM');
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  const page=await browser.newPage({viewport:{width:1100,height:1000}});
  const errors=[]; page.on('pageerror',error=>errors.push(error.message));
  await page.goto(process.env.PROTOTYPE_URL || 'http://127.0.0.1:8765/example/');
  await page.locator('#sound').check();
  await page.locator('#start').click();
  const waitBoot=()=>page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Running'),{},{timeout:60000});
  await waitBoot();
  await page.waitForTimeout(6000);
  if(output){mkdirSync(output,{recursive:true});await page.locator('#screen').screenshot({path:path.join(output,'c64-open.png')});}
  // Focused physical keys reach the ROM keyboard scanner.
  await page.locator('#screen').focus();
  for(const key of ['p','r','i','n','t',' ','Shift+2','o','k','Shift+2','Enter'])await page.keyboard.press(key,{delay:90});
  await page.waitForTimeout(300);
  if(output)await page.locator('#screen').screenshot({path:path.join(output,'c64-print.png')});
  await page.locator('#pause').click();await page.waitForTimeout(100);
  await page.locator('#benchmark').click();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Measurement complete'),{},{timeout:60000});
  console.log('C64',await page.locator('#metrics').textContent());
  // Frame stepping, malformed media and recovery.
  await page.locator('#step').click();
  await page.locator('#media').setInputFiles({name:'bad.prg',mimeType:'application/octet-stream',buffer:Buffer.from([1])});
  await page.waitForFunction(()=>document.querySelector('#status').classList.contains('error'));
  // Restart clears the error and all held inputs.
  await page.locator('#start').click();await waitBoot();
  await page.locator('#pause').click();await page.waitForTimeout(100);
  await page.locator('#benchmark').click();
  await page.locator('#start').click();await waitBoot();
  await page.waitForTimeout(500);
  assert((await page.locator('#status').textContent()).startsWith('Running'), 'Old benchmark overwrote the restarted session');
  await page.selectOption('#kind','amiga');
  await page.locator('#kickstart').setInputFiles(kickstart);
  await page.locator('#start').click();await waitBoot();
  await page.waitForTimeout(10000);
  if(output)await page.locator('#screen').screenshot({path:path.join(output,'amiga-kickstart.png')});
  await page.locator('#pause').click();await page.waitForTimeout(100);
  await page.locator('#benchmark').click();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Measurement complete'),{},{timeout:60000});
  console.log('Amiga',await page.locator('#metrics').textContent());
  assert.deepEqual(errors,[]);
  console.log('Boot, input, audio-worklet setup, pause/step/restart and malformed-media UI checks passed');
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
