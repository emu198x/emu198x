// node scripts/check-mouse-counts.mjs
// Checks that the browser's fractional mouse movement adds up to whole
// counts instead of rounding away. Needs no browser: it loads only the
// count arithmetic from example/mouse.js.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source = readFileSync(new URL('../example/mouse.js', import.meta.url), 'utf8');
const {relativeCounts} = await import(`data:text/javascript,${encodeURIComponent(source)}`);

const sum = moves => {
  const counts = relativeCounts(), total = [0, 0];
  for (const [dx, dy] of moves) {
    const [x, y] = counts.take(dx, dy);
    total[0] += x; total[1] += y;
  }
  return total;
};
assert.deepEqual(sum(Array(10).fill([0.4, -0.3])), [4, -3], 'slow movement adds up');
assert.deepEqual(sum(Array(8).fill([0.5, 0.5])), [4, 4], 'half counts add up');
assert.deepEqual(sum([[13, -7]]), [13, -7], 'whole counts pass through');
assert.deepEqual(sum([[2.6, 0], [-2.6, 0]]), [0, 0], 'there and back is no movement');
const counts = relativeCounts();
counts.take(0.4, 0.4);
counts.reset();
assert.deepEqual(counts.take(0.4, 0.4), [0, 0], 'a reset drops the fraction');
console.log('Passed: fractional mouse movement adds up to whole counts');

// Page positions to framebuffer pixels, stretched and letterboxed.
const {toFramebuffer} = await import(`data:text/javascript,${encodeURIComponent(source)}`);
const box = {left: 10, top: 20, width: 1536, height: 1152};
assert.deepEqual(toFramebuffer(box, [768, 576], false, 10, 20), [0, 0]);
assert.deepEqual(toFramebuffer(box, [768, 576], false, 10 + 300, 20 + 100), [150, 50]);
// Letterboxed into a wider box: scaled by height, centred across.
const wide = {left: 0, top: 0, width: 2000, height: 1152};
assert.deepEqual(toFramebuffer(wide, [768, 576], true, 232, 0), [0, 0]);
assert.deepEqual(toFramebuffer(wide, [768, 576], true, 1000, 576), [384, 288]);
console.log('Passed: page positions map onto the framebuffer');
