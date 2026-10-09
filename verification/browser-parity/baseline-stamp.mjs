// 0119-MERIDIAN S3: the native baseline is only meaningful if it was generated from the engine source
// under test. The old guard compared file mtimes, which a checkout, a cache restore or an artifact
// download rewrites freely (and which says nothing about content). The generator step now writes a
// stamp next to the baseline: a hash of the engine source files that decide the native output, and
// the sha256 of the baseline bytes. parity.spec.mjs recomputes both and refuses a mismatch.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { join, sep } from 'node:path';

// Files and directories (relative to the repository root) whose bytes decide the native baseline.
// A listed path that does not exist is an error, so a rename cannot silently shrink the hash.
export const SOURCE_PATHS = Object.freeze([
  'Cargo.toml',
  'Cargo.lock',
  'rust-toolchain.toml',
  '.cargo/config.toml',
  'oaxaca_blinder/Cargo.toml',
  'oaxaca_blinder/src',
  'engine/Cargo.toml',
  'engine/src',
  'engine/examples/native_baseline.rs',
  'oaxaca_blinder/tests/fixtures/parity_fixture.csv',
]);

export const BASELINE_FILE = 'native-baseline.json';
export const STAMP_FILE = 'native-baseline.stamp.json';

function listFiles(root, rel, out) {
  const abs = join(root, rel);
  if (!existsSync(abs)) throw new Error(`baseline-stamp: source path missing: ${rel}`);
  const st = statSync(abs);
  if (st.isDirectory()) {
    for (const name of readdirSync(abs).sort()) listFiles(root, join(rel, name), out);
  } else {
    out.push(rel.split(sep).join('/'));
  }
}

export function sourceHash(root, paths = SOURCE_PATHS) {
  const files = [];
  for (const p of paths) listFiles(root, p, files);
  files.sort();
  const h = createHash('sha256');
  for (const f of files) {
    const fileHash = createHash('sha256').update(readFileSync(join(root, f))).digest('hex');
    h.update(`${f}\0${fileHash}\n`);
  }
  return { hash: h.digest('hex'), fileCount: files.length };
}

export function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

export function writeStamp({ root, baselinePath, stampPath, commit = null, paths = SOURCE_PATHS }) {
  const { hash, fileCount } = sourceHash(root, paths);
  const stamp = {
    sourceHash: hash,
    sourceFileCount: fileCount,
    baselineSha256: sha256File(baselinePath),
    commit,
  };
  writeFileSync(stampPath, JSON.stringify(stamp, null, 2) + '\n');
  return stamp;
}

// Returns a list of human-readable problems; empty means the baseline matches the source on disk.
export function stampProblems({ root, baselinePath, stampPath, paths = SOURCE_PATHS }) {
  const problems = [];
  if (!existsSync(stampPath)) {
    return [`${STAMP_FILE} missing: the baseline was not produced by gen-native-baseline.mjs`];
  }
  let stamp;
  try {
    stamp = JSON.parse(readFileSync(stampPath, 'utf8'));
  } catch (e) {
    return [`${STAMP_FILE} is not valid JSON: ${e.message}`];
  }
  if (!existsSync(baselinePath)) return [`${BASELINE_FILE} missing`];
  const actualBaseline = sha256File(baselinePath);
  if (stamp.baselineSha256 !== actualBaseline) {
    problems.push(`baseline bytes changed after generation: stamp ${stamp.baselineSha256}, file ${actualBaseline}`);
  }
  const { hash } = sourceHash(root, paths);
  if (stamp.sourceHash !== hash) {
    problems.push(
      `baseline was generated from different engine source: stamp ${stamp.sourceHash}, source now ${hash}. ` +
        'Regenerate it with `node gen-native-baseline.mjs` (npm test does this).'
    );
  }
  return problems;
}
