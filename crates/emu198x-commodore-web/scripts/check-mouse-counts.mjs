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
