// DOM_PACKAGE=/path/to/happy-dom/lib/index.js node scripts/check-mouse-follow.mjs
// The page's side of following a guest pointer, in a DOM without a browser:
// hover sends targets and hides the cursor, clicks go to the guest, leaving
// stops steering, and without a guest pointer a click captures as before.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
if (!process.env.DOM_PACKAGE) throw new Error('Set DOM_PACKAGE to happy-dom/lib/index.js.');
const {Window} = await import(pathToFileURL(process.env.DOM_PACKAGE));
const window = new Window({url: 'https://example.test/'});
Object.assign(globalThis, {window, document: window.document, getComputedStyle: e => window.getComputedStyle(e)});
const frames = [];
globalThis.requestAnimationFrame = callback => frames.push(callback);
const flush = () => { while (frames.length) frames.shift()(); };
const source = readFileSync(new URL('../example/mouse.js', import.meta.url), 'utf8');
const {amigaMouse} = await import(`data:text/javascript,${encodeURIComponent(source)}`);

const canvas = document.createElement('canvas');
canvas.width = 768; canvas.height = 576;
canvas.getBoundingClientRect = () => ({left: 0, top: 0, width: 1536, height: 1152});
let captures = 0;
canvas.requestPointerLock = () => { captures++; };
const button = document.createElement('button'), hint = document.createElement('p');
document.body.append(canvas, button, hint);
const sent = [];
const mouse = amigaMouse({canvas, button, hint, send: events => sent.push(...events), releaseKeys() {}});
const event = (type, init) => canvas.dispatchEvent(new window.MouseEvent(type, {bubbles: true, ...init}));

mouse.setEnabled(true);
mouse.setGuestPointer(new Float64Array([100, 100]));
assert.equal(canvas.dataset.pointer, 'follows');
event('mousemove', {clientX: 300, clientY: 200});
event('mousemove', {clientX: 302, clientY: 202});
flush();
assert.deepEqual(sent, [['target', 151, 101]], 'one target a frame, the latest');
assert.equal(canvas.style.cursor, 'none', 'the guest pointer stands in for the cursor');

sent.length = 0;
event('mousedown', {button: 0});
document.dispatchEvent(new window.MouseEvent('mouseup', {button: 0, bubbles: true}));
assert.deepEqual(sent, [['mouse', 'left', true], ['mouse', 'left', false]], 'a followed click is the guest\'s');
assert.equal(captures, 0, 'following does not capture');

sent.length = 0;
event('mouseleave', {});
assert.deepEqual(sent, [['leave']]);
assert.equal(canvas.style.cursor, '');

// The pointer goes, as when a game takes the screen: back to capture.
mouse.setGuestPointer(undefined);
sent.length = 0;
event('mousemove', {clientX: 300, clientY: 200});
flush();
event('mousedown', {button: 0});
assert.deepEqual(sent, [], 'nothing reaches the guest uncaptured');
assert.equal(captures, 1, 'the click captures');
assert.equal(canvas.dataset.pointer, '');
mouse.destroy();
console.log('Passed: hover steers a known guest pointer, clicks are the guest\'s, and capture remains the fallback');
