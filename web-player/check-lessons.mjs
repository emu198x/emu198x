// Explicit local QA against Code198x's built preview. Firmware stays on disk.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
const {chromium}=await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE));
const browser=await chromium.launch({channel:'chrome',headless:true});
const base=process.env.CODE198X_TEST_URL || 'http://127.0.0.1:8766';
const romRoot=process.env.PLAYER_TEST_ROM_ROOT;
const cases=[
 ['nintendo-entertainment-system',{}],
 ['commodore-64',{kernal:'commodore-c64/kernal.rom',basic:'commodore-c64/basic.rom',chargen:'commodore-c64/chargen.rom'}],
 ['commodore-amiga',{kickstart:'commodore-amiga/kick13.rom'}],
 ['sinclair-zx-spectrum',{rom:'sinclair-zx-spectrum-48k/48.rom'}],
];
try {
 for(const [system,firmware] of cases) {
  if(Object.keys(firmware).length && !romRoot)continue;
  const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(String(error)));
  await page.goto(`${base}/systems/${system}/assembly/meet-the-machine/unit-01/`);
  const inline=system==='nintendo-entertainment-system';
  const lesson=page.locator(inline?'code198x-nes-editor':'.lesson-player');
  if(!inline)await lesson.locator(':scope > summary').click();
  const player=lesson.locator('emu198x-player');
  if(inline)await lesson.getByRole('button',{name:'Assemble & run',exact:true}).click();
  else await player.locator('#run-example').click();
  if(Object.keys(firmware).length)await player.locator('#status').filter({hasText:'Choose'}).waitFor();
  for(const [id,file] of Object.entries(firmware))await player.locator('#'+id).setInputFiles(path.join(romRoot,file));
  if(Object.keys(firmware).length)await player.locator('#boot').click();
  await player.locator('#status').filter({hasText:'Running'}).waitFor({timeout:60000});
  const expected={'nintendo-entertainment-system':[181,49,32],'commodore-64':[136,57,50],'commodore-amiga':[255,0,0],'sinclair-zx-spectrum':[194,0,0]}[system];
  // Prove the lesson ran, not merely that firmware drew a boot screen. Tapes
  // take real loading time, so wait for the program's documented red output.
  await page.waitForFunction(rgb=>{
   const canvas=document.querySelector('.lesson-player emu198x-player, code198x-nes-editor emu198x-player')?.shadowRoot?.getElementById('screen');
   if(!canvas)return false;const {data}=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height);let count=0;
   for(let i=0;i<data.length;i+=4)if(data[i]===rgb[0] && data[i+1]===rgb[1] && data[i+2]===rgb[2])count++;
   const ready=count>(rgb[0]===194?54000:20000);
   if(!ready){window.lessonColourSince=0;return false;}
   window.lessonColourSince ||= performance.now();
   return performance.now()-window.lessonColourSince>1000;
  },expected,{timeout:90000});
  const colours=await player.locator('#screen').evaluate(canvas=>{
   const {data}=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height);const counts={};
   for(let i=0;i<data.length;i+=4){const colour=Array.from(data.slice(i,i+3)).join(',');counts[colour]=(counts[colour] || 0)+1;}
   return Object.entries(counts).sort((a,b)=>b[1]-a[1]).slice(0,6);
  });
  assert(colours.some(([colour])=>colour!=='0,0,0'),'The lesson must render a nonblack picture');
  assert.deepEqual(errors,[]);console.log(system,JSON.stringify(colours));
  await player.screenshot({path:`/private/tmp/198x/lesson-${system}.png`});await page.close();
 }
}finally{await browser.close();}
