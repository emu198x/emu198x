// Validate the publication fixtures against every real PET runtime profile.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtempSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {fixtures} from './fixtures.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const run = (binary, args = []) => JSON.parse(execFileSync('cargo', [
  'run', '--quiet', '--locked', '--release', '-p', 'emu198x-fleet-web',
  '--no-default-features', '--features', 'commodore-pet', '--bin', binary,
  ...(args.length ? ['--', ...args] : []),
], {cwd: root, encoding: 'utf8', maxBuffer: 4e6}));
const profiles = run('fleet-catalogue');
assert(profiles.length > 0, 'The runtime catalogue must contain PET profiles');
assert(profiles.every(profile => profile.family === 'commodore-pet'));
// No local firmware: this gate must exercise the shipped synthetic fixtures.
const {cases, missing} = fixtures(profiles, '');
assert.deepEqual(missing, []);
assert.deepEqual(cases.map(test => test.id).sort(), profiles.map(profile => profile.id).sort());
const scratch = mkdtempSync(path.join(tmpdir(), 'emu198x-pet-fixtures-'));
try {
  const manifest = path.join(scratch, 'fixtures.json');
  writeFileSync(manifest, JSON.stringify(cases));
  const results = run('fleet-probe', [manifest]);
  assert.equal(results.length, cases.length);
  for (const [index, result] of results.entries()) {
    assert(!result.error, `${cases[index].id}: ${result.error}`);
    assert(result.width > 0 && result.height > 0);
    assert.equal(result.checkpoints.length, 3);
    console.log(`${cases[index].id}: synthetic firmware accepted by its runtime`);
  }
} finally {
  rmSync(scratch, {recursive: true, force: true});
}
