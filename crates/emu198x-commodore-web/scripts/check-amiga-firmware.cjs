// PLAYWRIGHT_PACKAGE=... node scripts/check-amiga-firmware.cjs KICKSTART WB13_ADF AROS_MAIN AROS_EXT OUTPUT_DIR
// All firmware and media enter through file selection; no local files are served.
const {chromium}=require(process.env.PLAYWRIGHT_PACKAGE || 'playwright');
const {mkdirSync}=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const [kick,wb,main,ext,output]=process.argv.slice(2);
if(!output)throw new Error('Pass KICKSTART WB13_ADF AROS_MAIN AROS_EXT OUTPUT_DIR');
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  mkdirSync(output,{recursive:true});
  const page=await browser.newPage();
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(process.env.PROTOTYPE_URL || 'http://127.0.0.1:8765/example/');
  await page.selectOption('#kind','amiga');
  await page.locator('#kickstart').setInputFiles(kick);
  await page.locator('#start').click();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Running'),{},{timeout:60000});
  await page.locator('#media').setInputFiles(wb);
  await page.waitForTimeout(24000);
  await page.locator('#screen').screenshot({path:path.join(output,'workbench13.png')});
  console.log('Workbench:',await page.locator('#status').textContent());
  await page.selectOption('#amiga-firmware','aros');
  await page.locator('#aros-main').setInputFiles(main);
  await page.locator('#aros-ext').setInputFiles(ext);
  await page.locator('#start').click();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Running'),{},{timeout:60000});
  await page.waitForTimeout(35000);
  await page.locator('#screen').screenshot({path:path.join(output,'aros.png')});
  const shape=await page.locator('#screen').evaluate(canvas=>{
   const pixels=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;
   const counts=new Map();
   for(let i=0;i<pixels.length;i+=4){const color=(pixels[i]<<16)|(pixels[i+1]<<8)|pixels[i+2];counts.set(color,(counts.get(color)||0)+1);}
   const dominant=[...counts].sort((a,b)=>b[1]-a[1])[0][0];let rows=0;
   for(let y=0;y<canvas.height;y++){
    for(let x=0;x<canvas.width;x++){const i=(y*canvas.width+x)*4;const color=(pixels[i]<<16)|(pixels[i+1]<<8)|pixels[i+2];if(color!==dominant){rows++;break;}}
   }
   return {colors:counts.size,rows};
  });
  assert(shape.colors>4 && shape.rows>50,`AROS did not render its boot screen: ${JSON.stringify(shape)}`);
  assert.deepEqual(errors,[]);
  console.log('AROS:',shape,await page.locator('#metrics').textContent());
  console.log(`Screenshots in ${output}; inspect Workbench for the clock warning and AROS for the boot screen.`);
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
