// node scripts/check-parity.mjs EXPECTED.json ROM... [--browser]
// Browser mode needs a local server and PLAYWRIGHT_PACKAGE pointing to a
// Playwright installation (or an installed playwright package).
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import init, {Commodore} from '../pkg/emu198x_commodore_web.js';
import {runCheck} from '../example/parity.js';
const args=process.argv.slice(2), browserMode=args.includes('--browser');
const files=args.filter(x=>x!=='--browser');
if(files.length<2)throw new Error('Pass EXPECTED.json then KERNAL BASIC CHARGEN [1541], or KICKSTART');
const expected=JSON.parse(readFileSync(files[0],'utf8'));
const roms=files.slice(1).map(path=>Array.from(readFileSync(path)));
if(expected.kind==='c64' && roms.length===3)roms.push([]);
if(browserMode) {
  const require=createRequire(import.meta.url);
  const {chromium}=require(process.env.PLAYWRIGHT_PACKAGE || 'playwright');
  const browser=await chromium.launch({headless:true});
  try {
    const page=await browser.newPage();
    await page.goto(process.env.PROTOTYPE_URL || 'http://127.0.0.1:8765/example/');
    console.log(await page.evaluate(async ({expected,roms})=>{
      const {default:init,Commodore}=await import('../pkg/emu198x_commodore_web.js');
      const {runCheck}=await import('./parity.js');
      await init();return runCheck(Commodore,expected.kind,roms,expected);
    },{expected,roms}));
  } finally {await browser.close();}
} else {
  await init({module_or_path:readFileSync(new URL('../pkg/emu198x_commodore_web_bg.wasm',import.meta.url))});
  console.log(runCheck(Commodore,expected.kind,roms,expected));
}
