// Real Chrome layout and native keyboard input, with no extra dependencies.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const chrome = process.env.CHROME_BIN || (process.platform === 'darwin'
  ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
const profiles = await readFile(path.join(root, 'crates/runtime-commodore-amiga/src/profiles.rs'), 'utf8');
const displayNames = profiles.split('pub const fn display_name(self)')[1]?.split('/// RAM layout')[0];
const models = [...(displayNames || '').matchAll(/Self::\w+ => "([^"]+)"/g)].map(match => match[1]);
assert(models.length >= 18, 'Read the real Amiga model names; an empty fixture must fail');
const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    if (pathname === '/web-player/layout-models.json') {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify(models));
      return;
    }
    const filename = path.resolve(root, '.' + decodeURIComponent(pathname));
    if (!filename.startsWith(root + path.sep)) throw new Error('Outside fixture root');
    const content = await readFile(filename);
    response.setHeader('Content-Type', ({'.html': 'text/html', '.css': 'text/css'})[path.extname(filename)] || 'application/octet-stream');
    response.end(content);
  } catch { response.writeHead(404); response.end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const temporary = await mkdtemp(path.join(tmpdir(), 'emu198x-layout-'));
let socket, child;
try {
  child = spawn(chrome, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${temporary}`,
    '--no-first-run', '--no-default-browser-check', 'about:blank'], {stdio: 'ignore'});
  let launchError;
  child.on('error', error => { launchError = error; });
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (launchError) throw launchError;
    try { port = Number((await readFile(path.join(temporary, 'DevToolsActivePort'), 'utf8')).split('\n')[0]); break; }
    catch { await delay(50); }
  }
  assert(port, 'Chrome did not open its debugging endpoint within five seconds');
  const targets = await fetch(`http://127.0.0.1:${port}/json/list`, {signal: AbortSignal.timeout(5000)}).then(response => response.json());
  socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Chrome debugging socket timed out')), 5000);
    socket.onopen = () => { clearTimeout(timer); resolve(); };
    socket.onerror = error => { clearTimeout(timer); reject(error); };
  });
  let serial = 0;
  const pending = new Map();
  socket.onmessage = ({data}) => {
    const message = JSON.parse(data), entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id); clearTimeout(entry.timer);
    if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
    else entry.resolve(message.result);
  };
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++serial;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, 10000);
    pending.set(id, {resolve, reject, timer}); socket.send(JSON.stringify({id, method, params}));
  });
  const evaluate = async expression => {
    const result = await call('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await call('Page.enable');
  const receipts = [];
  for (const width of [320, 390, 1280]) for (const theme of ['light', 'dark']) {
    await call('Emulation.setDeviceMetricsOverride', {width, height: 900, deviceScaleFactor: 1, mobile: false});
    await call('Emulation.setEmulatedMedia', {features: [{name: 'prefers-color-scheme', value: theme}]});
    await call('Page.navigate', {url: `http://127.0.0.1:${server.address().port}/web-player/check-layout.html${process.argv.includes('--negative') ? '?negative=1' : ''}`});
    let ready = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      ready = await evaluate('document.readyState === "complete" && Boolean(window.ready)');
      if (ready) break;
      await delay(20);
    }
    assert(ready, 'Layout fixture did not load');
    await evaluate('window.ready');
    await evaluate(`document.documentElement.style.colorScheme = ${JSON.stringify(theme)}`);
    const bounds = await evaluate(`(() => {
      const root = document.querySelector('emu198x-player').shadowRoot;
      return {viewport: innerWidth, document: document.documentElement.scrollWidth,
        controls: ['variant', 'slot'].map(id => {
          const control = root.getElementById(id), box = control.getBoundingClientRect();
          const label = control.closest('label').getBoundingClientRect();
          return {id, left: box.left, right: box.right, width: box.width, height: box.height,
            labelLeft: label.left, labelRight: label.right, options: control.options.length};
        })};
    })()`);
    assert.equal(bounds.viewport, width, 'Test must run at the requested CSS viewport width');
    assert(bounds.document <= width, `${width}/${theme}: document overflows to ${bounds.document}px`);
    for (const control of bounds.controls) {
      assert(control.left >= 24 && control.right <= width - 24 + 1, JSON.stringify(control));
      assert(control.labelLeft >= 24 && control.labelRight <= width - 24 + 1, JSON.stringify(control));
      assert(control.width >= 44 && control.height >= 44 && control.options > 1, JSON.stringify(control));
    }
    // Tab from the canvas through the production template to the model select.
    await evaluate("document.querySelector('emu198x-player').shadowRoot.getElementById('screen').focus()");
    for (let attempt = 0; attempt < 20; attempt++) {
      await call('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9});
      await call('Input.dispatchKeyEvent', {type: 'keyUp', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9});
      if (await evaluate("document.querySelector('emu198x-player').shadowRoot.activeElement?.id === 'variant'")) break;
    }
    assert(await evaluate("document.querySelector('emu198x-player').shadowRoot.activeElement?.id === 'variant'"), 'Model must be reachable by Tab');
    assert(await evaluate("document.querySelector('emu198x-player').shadowRoot.activeElement.matches(':focus-visible')"), 'Keyboard focus must be visible');
    const before = await evaluate("document.querySelector('emu198x-player').shadowRoot.activeElement.value");
    // Native typeahead stays in Blink on every platform. On macOS arrows open
    // an OS popup which CDP cannot drive (chrome-devtools-mcp issue #3033).
    for (const key of 'Commodore Amiga 1200') {
      await call('Input.dispatchKeyEvent', {type: 'keyDown', key, text: key});
      await call('Input.dispatchKeyEvent', {type: 'keyUp', key});
    }
    const selection = await evaluate(`({value: document.querySelector('emu198x-player').shadowRoot.getElementById('variant').value, changes: window.changes})`);
    assert(selection.value !== before && selection.changes > 0, `Native keyboard selection must change the model: ${JSON.stringify({before, selection})}`);
    if (process.env.PLAYER_LAYOUT_SCREENSHOTS) {
      const {data} = await call('Page.captureScreenshot');
      await writeFile(path.join(process.env.PLAYER_LAYOUT_SCREENSHOTS, `${width}-${theme}.png`), Buffer.from(data, 'base64'));
    }
    receipts.push({width, theme, document: bounds.document, controls: bounds.controls, keyboard: 'passed'});
  }
  console.log(JSON.stringify({status: 'PASS', receipts}, null, 2));
} finally {
  socket?.close();
  if (child && child.exitCode === null && child.pid) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    child.kill();
    const timer = setTimeout(() => child.kill('SIGKILL'), 2000);
    await exited; clearTimeout(timer);
  }
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(temporary, {recursive: true, force: true});
}
