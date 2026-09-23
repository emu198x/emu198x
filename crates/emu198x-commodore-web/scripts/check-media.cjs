// PLAYWRIGHT_PACKAGE=... node scripts/check-media.cjs KICKSTART PAULA_TEST_ADF
// Uses the repository-owned channel-0-full.adf from the Paula audio corpus.
const {chromium}=require(process.env.PLAYWRIGHT_PACKAGE || 'playwright');
const {readFileSync}=require('node:fs');
const assert=require('node:assert/strict');
if(process.argv.length!==4)throw new Error('Pass KICKSTART and the Paula channel-0-full.adf');
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  const page=await browser.newPage();await page.goto(process.env.PROTOTYPE_URL || 'http://127.0.0.1:8765/example/');
  const result=await page.evaluate(async({kick,adf})=>{
   const worker=new Worker('./worker.js',{type:'module'});let id=0;
   const call=(command,...args)=>new Promise((resolve,reject)=>{
    worker.onmessage=({data})=>data.error?reject(new Error(data.error)):resolve(data.result);
    worker.postMessage({id:++id,command,args});
   });
   try {
    const get=async name=>new Uint8Array(await (await fetch(`./roms/${name}.rom`)).arrayBuffer());
    await call('boot','c64',{kernal:await get('kernal_generic'),basic:await get('basic_generic'),chargen:await get('chargen_openroms'),drive:new Uint8Array()},48000);
    for(let i=0;i<300;i++)await call('step');
    // Self-contained 6510 test program at $C000: set voice 1 to a sustained
    // sawtooth, then increment the border forever. No firmware bytes involved.
    const program=[0,0xc0];
    for(const [value,register] of [[15,0x18],[0,5],[0xf0,6],[0,0],[0x20,1],[0x21,4]])program.push(0xa9,value,0x8d,register,0xd4);
    program.push(0xee,0x20,0xd0,0x4c,0x1e,0xc0);
    await call('load','prg',new Uint8Array(program));
    for(const key of ['s','y','s','4','9','1','5','2','enter']) {
     await call('input',[['key',key,true]]);for(let i=0;i<4;i++)await call('step');
     await call('input',[['key',key,false]]);for(let i=0;i<4;i++)await call('step');
    }
    let c64Peak=0;
    for(let i=0;i<120;i++){const r=await call('step');for(const v of r.audio)c64Peak=Math.max(c64Peak,Math.abs(v));}
    const c64=await call('benchmark');
    await call('boot','amiga',{kickstart:new Uint8Array(kick)},48000);
    await call('load','adf',new Uint8Array(adf));
    let amigaPeak=0,frames=0;
    for(let i=0;i<1200;i++) {
     const r=await call('step');for(const v of r.audio)amigaPeak=Math.max(amigaPeak,Math.abs(v));
     frames++;if(i>700 && amigaPeak>.01)break;
    }
    const amiga=await call('benchmark');
    return {c64Peak,c64,amigaPeak,amigaFrames:frames,amiga};
   } finally {worker.terminate();}
  },{kick:Array.from(readFileSync(process.argv[2])),adf:Array.from(readFileSync(process.argv[3]))});
  console.log(JSON.stringify(result,null,2));
  assert(result.c64Peak>.01,'PRG/SYS did not produce sustained SID audio');
  assert(result.amigaPeak>.01,'ADF did not produce Paula audio');
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
