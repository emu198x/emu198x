// PLAYWRIGHT_PACKAGE=... node scripts/check-mouse.cjs KICKSTART
const {chromium}=require(process.env.PLAYWRIGHT_PACKAGE || 'playwright');
const assert=require('node:assert/strict');
if(!process.argv[2])throw new Error('Pass an A500 Kickstart ROM');
(async()=>{
 // macOS Chromium requires a focused native view for real pointer lock.
 const browser=await chromium.launch({headless:false});
 try {
  const page=await browser.newPage({viewport:{width:1200,height:1100}});
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(()=>{
   window.mouseInputs=[];
   const original=Worker.prototype.postMessage;
   Worker.prototype.postMessage=function(message,...rest){
    if(message.command==='input')window.mouseInputs.push(...message.args[0]);
    return original.call(this,message,...rest);
   };
  });
  await page.goto(process.env.PROTOTYPE_URL || 'http://127.0.0.1:8765/example/');
  await page.selectOption('#kind','amiga');
  await page.locator('#kickstart').setInputFiles(process.argv[2]);
  await page.locator('#start').click();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Running'),{},{timeout:60000});
  const screen=page.locator('#screen');await screen.scrollIntoViewIfNeeded();
  const inputs=()=>page.evaluate(()=>window.mouseInputs);
  const clear=()=>page.evaluate(()=>{window.mouseInputs=[];});
  const locked=()=>page.waitForFunction(()=>document.pointerLockElement===document.querySelector('#screen'));
  const unlocked=()=>page.waitForFunction(()=>!document.pointerLockElement);
  await screen.hover();await page.mouse.move(600,700);await clear();
  await screen.hover();assert.deepEqual(await inputs(),[],'Hover must not move the guest');
  await page.bringToFront();
  await screen.click();await locked();
  assert.deepEqual(await inputs(),[],'Capture click must not click the guest');
  for(const width of ['1000px','500px']) {
   await screen.evaluate((canvas,width)=>canvas.style.width=width,width);
   await clear();
   // Same physical mouse deltas remain relative device counts across CSS sizes.
   await page.evaluate(()=>document.dispatchEvent(new MouseEvent('mousemove',{movementX:13,movementY:-7,bubbles:true})));
   assert.deepEqual(await inputs(),[['move',13,-7]],`Relative input changed at ${width}`);
  }
  await clear();await page.mouse.down({button:'left'});
  await page.keyboard.down('Shift');
  assert((await inputs()).some(x=>x[0]==='key' && x[1]==='lshift' && x[2]===true),'Captured keyboard must reach guest');
  await page.keyboard.press('Escape');await unlocked();
  await page.waitForFunction(()=>window.mouseInputs.some(x=>x[0]==='key' && x[1]==='lshift' && x[2]===false));
  const released=await inputs();
  assert(released.some(x=>x[0]==='mouse' && x[1]==='left' && x[2]===true));
  assert(released.some(x=>x[0]==='mouse' && x[1]==='left' && x[2]===false),'Unlock must release held mouse button');
  assert(released.some(x=>x[0]==='key' && x[1]==='lshift' && x[2]===false),'Unlock must release held key');
  assert(!released.some(x=>x[0]==='key' && x[1]==='escape'),'Unlock Escape leaked to the guest');
  await page.mouse.up();await page.keyboard.up('Shift');
  await clear();await screen.hover();assert.deepEqual(await inputs(),[]);
  // Real engagement after Escape, using the accessible capture button.
  await page.locator('#capture-mouse').click();await locked();
  // Keyboard activation avoids trying to aim the hidden browser cursor at UI.
  await page.locator('#pause').evaluate(button=>button.click());await unlocked();
  assert(await page.locator('#capture-mouse').isDisabled(),'Paused capture must be disabled');
  await page.locator('#pause').click();
  await page.locator('#capture-mouse').click();await locked();
  await page.locator('#start').evaluate(button=>button.click());await unlocked();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Running'),{},{timeout:60000});
  await page.selectOption('#kind','c64');
  assert(await page.locator('#capture-mouse').isDisabled(),'C64 must not capture an Amiga mouse');
  assert.deepEqual(errors,[]);
  console.log('Passed: real capture/release, no hover or activation-click leakage, scaled canvas, Escape, held input release, pause/restart and C64 isolation');
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
