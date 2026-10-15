// Compare an identified generated WASM module with the existing native
// parity example. No browser instrumentation or firmware copying is involved.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {runCheck} from '../../../crates/emu198x-commodore-web/example/parity.js';
const [modulePath, expectedPath, firmwarePath] = process.argv.slice(2);
if(!modulePath || !expectedPath || !firmwarePath)throw Error('Pass module JS, native checkpoint JSON and Kickstart');
const url=pathToFileURL(resolve(modulePath));
const {default:init,Commodore}=await import(url);
await init({module_or_path:readFileSync(new URL('./emu198x_commodore_web_bg.wasm',url))});
const expected=JSON.parse(readFileSync(expectedPath,'utf8'));
console.log(runCheck(Commodore,expected.kind,[Array.from(readFileSync(firmwarePath))],expected));
