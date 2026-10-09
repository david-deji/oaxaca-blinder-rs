// Generator step for the native <-> wasm parity leg (0119-MERIDIAN S3).
// Runs the native decompose_inner() example, writes native-baseline.json, and writes the stamp that
// parity.spec.mjs checks (hash of the engine source + sha256 of the baseline bytes).
//   node gen-native-baseline.mjs
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BASELINE_FILE, STAMP_FILE, writeStamp } from './baseline-stamp.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');

const run = spawnSync('cargo', ['run', '--locked', '-p', 'pay-equity-engine', '--example', 'native_baseline'], {
  cwd: root,
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
  stdio: ['ignore', 'pipe', 'inherit'],
});
if (run.error || run.status !== 0) {
  console.error(`native_baseline example failed: ${run.error ? run.error.message : 'exit ' + run.status}`);
  process.exit(1);
}
try {
  JSON.parse(run.stdout);
} catch (e) {
  console.error(`native_baseline output is not JSON: ${e.message}`);
  process.exit(1);
}

const baselinePath = join(here, BASELINE_FILE);
const stampPath = join(here, STAMP_FILE);
writeFileSync(baselinePath, run.stdout);

const git = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' });
const commit = git.status === 0 ? git.stdout.trim() : null;
const stamp = writeStamp({ root, baselinePath, stampPath, commit });
console.log(`wrote ${BASELINE_FILE} (${stamp.baselineSha256}) and ${STAMP_FILE} (source ${stamp.sourceHash}, ${stamp.sourceFileCount} files)`);
