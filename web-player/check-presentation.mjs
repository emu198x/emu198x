// Real browser pixels, not a mocked canvas. CHROME_BIN can select a local Chrome.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,mkdtemp,rm} from 'node:fs/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const exec=promisify(execFile), root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const chrome=process.env.CHROME_BIN || (process.platform==='darwin'?'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome':'google-chrome');
const server=createServer(async(request,response)=>{
 try {
  const filename=path.resolve(root,'.'+decodeURIComponent(new URL(request.url,'http://localhost').pathname));
  if(!filename.startsWith(root+path.sep))throw new Error('Outside fixture root');
  const content=await readFile(filename);
  response.writeHead(200,{'Content-Type':({'.js':'text/javascript','.html':'text/html','.css':'text/css'})[path.extname(filename)] || 'application/octet-stream'});response.end(content);
 } catch {response.writeHead(404);response.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const temporary=await mkdtemp(path.join(tmpdir(),'emu198x-presentation-'));
try {
 const receipts=[];
 for(const [dpr,negative] of [[1,false],[2,false],[1,true]]) {
  const {stdout,stderr}=await exec(chrome,[
   '--headless=new',`--user-data-dir=${temporary}/profile-${dpr}-${negative}`,
   `--force-device-scale-factor=${dpr}`,'--window-size=800,700',
   '--virtual-time-budget=5000','--dump-dom',
   ...(process.platform==='linux'?['--enable-unsafe-swiftshader','--use-angle=swiftshader']:[]),
   `http://127.0.0.1:${server.address().port}/web-player/check-presentation.html${negative?'?negative=1':''}`,
  ],{timeout:30000,maxBuffer:1024*1024});
  const output=stdout.match(/<pre id="result">([\s\S]*?)<\/pre>/)?.[1];
  assert(output,`Browser produced no result: ${stderr}`);
  if(negative)assert.match(output,/FAIL: Error: .*source frame changed/,'Corrupt source control must fail');
  else {
   const receipt=JSON.parse(output);assert.equal(receipt.status,'PASS');assert.equal(receipt.dpr,dpr);
   receipts.push(receipt);
  }
 }
 console.log(JSON.stringify({receipts,negativeControl:'rejected source mutation'},null,2));
} finally {
 await new Promise(resolve=>server.close(resolve));
 await rm(temporary,{recursive:true,force:true});
}
