// Unit tests for the native-baseline stamp (0119-MERIDIAN S3): content hashes, not mtimes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sourceHash, stampProblems, writeStamp, SOURCE_PATHS } from './baseline-stamp.mjs';

const PATHS = ['Cargo.lock', 'src'];

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'stamp-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'Cargo.lock'), 'lock-v1');
  writeFileSync(join(root, 'src', 'a.rs'), 'fn a() {}');
  writeFileSync(join(root, 'src', 'b.rs'), 'fn b() {}');
  const baselinePath = join(root, 'native-baseline.json');
  const stampPath = join(root, 'native-baseline.stamp.json');
  writeFileSync(baselinePath, '{"total_gap":1}');
  writeStamp({ root, baselinePath, stampPath, paths: PATHS });
  return { root, baselinePath, stampPath };
}
const check = (f) => stampProblems({ root: f.root, baselinePath: f.baselinePath, stampPath: f.stampPath, paths: PATHS });

test('a freshly stamped baseline has no problems', () => {
  const f = fixture();
  assert.deepEqual(check(f), []);
  rmSync(f.root, { recursive: true });
});

test('touching files (mtime only) changes nothing, a content change in the source does', () => {
  const f = fixture();
  const future = new Date(Date.now() + 3600_000);
  utimesSync(join(f.root, 'src', 'a.rs'), future, future);
  assert.deepEqual(check(f), []);
  writeFileSync(join(f.root, 'src', 'a.rs'), 'fn a() { 1; }');
  const problems = check(f);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /different engine source/);
  rmSync(f.root, { recursive: true });
});

test('a baseline edited after generation is refused', () => {
  const f = fixture();
  writeFileSync(f.baselinePath, '{"total_gap":2}');
  const problems = check(f);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /baseline bytes changed after generation/);
  rmSync(f.root, { recursive: true });
});

test('a missing stamp, and a missing source path, are errors', () => {
  const f = fixture();
  rmSync(f.stampPath);
  assert.match(check(f)[0], /stamp.*missing|missing/i);
  writeStamp({ root: f.root, baselinePath: f.baselinePath, stampPath: f.stampPath, paths: PATHS });
  rmSync(join(f.root, 'Cargo.lock'));
  assert.throws(() => check(f), /source path missing: Cargo.lock/);
  rmSync(f.root, { recursive: true });
});

test('a new file in a hashed directory changes the hash', () => {
  const f = fixture();
  const before = sourceHash(f.root, PATHS).hash;
  writeFileSync(join(f.root, 'src', 'c.rs'), 'fn c() {}');
  assert.notEqual(sourceHash(f.root, PATHS).hash, before);
  rmSync(f.root, { recursive: true });
});

test('every real SOURCE_PATHS entry exists in this repository', () => {
  const root = new URL('../../', import.meta.url).pathname;
  const { hash, fileCount } = sourceHash(root, SOURCE_PATHS);
  assert.match(hash, /^[0-9a-f]{64}$/);
  assert.ok(fileCount > 10);
});
