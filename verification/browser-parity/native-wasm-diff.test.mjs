// Unit tests for the native <-> wasm comparator (0119-MERIDIAN S3). Run: `node --test` (no browser).
// They prove the comparator can fail: a comparator that cannot is why the old parity check was a false red
// for months and could have become a false green just as easily.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  ABSENT_AS_NULL_PATHS,
  NUMERIC_LEAF_FLOOR,
  REQUIRED_NUMERIC_PATHS,
  coverageProblems,
  diffNativeWasm,
} from './native-wasm-diff.mjs';

// Shape of engine/examples/native_baseline.rs output (what decompose_inner() serializes), numbers as emitted.
const NATIVE = Object.freeze({
  total_gap: 0.9086731472231868,
  explained_gap: 0.1212803477399896,
  unexplained_gap: 0.7542160167898229,
  interaction_gap: 0.033176782693374214,
  explained_percentage: 13.34697169280396,
  unexplained_percentage: 83.00190438053889,
  interaction_percentage: 3.651123926657138,
  detailed_explained: [],
  detailed_unexplained: [],
  data_summary: {
    total_count: 300,
    group_a_count: 150,
    group_b_count: 150,
    group_a_mean: 2.8320603882784767,
    group_b_mean: 3.740733535501663,
  },
  unexplained_standard_error: null,
  run_metadata: {
    seed: '6840134480574414850',
    rng_algorithm: 'ChaCha8',
    rand_chacha_version: '0.3.1',
    bootstrap_reps_requested: 64,
    bootstrap_reps_succeeded: 64,
    bootstrap_reps_discarded: 0,
  },
  unresolved_row_keys: null,
  analysed_reference_count: 150,
  analysed_target_count: 150,
  excluded_rows: [],
  adjustments_on_excluded_rows: 0,
});

// What serde_wasm_bindgen hands back: `None` fields are dropped, not null.
function wasmFrom(native) {
  const w = structuredClone(native);
  delete w.unexplained_standard_error;
  delete w.unresolved_row_keys;
  return w;
}

test('the absent-as-null allowlist is exactly the two TRUST-4 paths and cannot be widened at runtime', () => {
  assert.deepEqual([...ABSENT_AS_NULL_PATHS], ['$.unexplained_standard_error', '$.unresolved_row_keys']);
  assert.ok(Object.isFrozen(ABSENT_AS_NULL_PATHS));
  assert.throws(() => {
    'use strict';
    ABSENT_AS_NULL_PATHS.push('$.total_gap');
  });
});

test('real-shaped payload: null vs absent on the allowlisted paths passes and compares 18 numeric leaves', () => {
  const r = diffNativeWasm(NATIVE, wasmFrom(NATIVE));
  assert.deepEqual(r.mismatches, []);
  assert.equal(r.numericLeafCount, 18);
  assert.deepEqual(coverageProblems(r), []);
});

test('the floor and the required paths describe the real payload exactly', () => {
  const r = diffNativeWasm(NATIVE, wasmFrom(NATIVE));
  assert.equal(NUMERIC_LEAF_FLOOR, r.numericLeafCount);
  assert.deepEqual([...REQUIRED_NUMERIC_PATHS].sort(), [...r.numericPaths].sort());
});

test('null on the wasm side and absent on the native side also passes on an allowlisted path', () => {
  const native = wasmFrom(NATIVE);
  const wasm = structuredClone(NATIVE);
  assert.deepEqual(diffNativeWasm(native, wasm).mismatches, []);
});

test('a number against an absent key fails, even on an allowlisted path', () => {
  const native = { ...structuredClone(NATIVE), unexplained_standard_error: 0.0123 };
  const r = diffNativeWasm(native, wasmFrom(NATIVE));
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.unexplained_standard_error: key present in only one/);
});

test('null against an absent key fails on a path that is not allowlisted', () => {
  const native = { ...structuredClone(NATIVE), some_other_field: null };
  const r = diffNativeWasm(native, wasmFrom(NATIVE));
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.some_other_field: key present in only one/);
});

test('a numeric leaf 2e-6 off fails, 5e-7 off passes', () => {
  const off = (delta) => {
    const w = wasmFrom(NATIVE);
    w.data_summary.group_a_mean += delta;
    return diffNativeWasm(NATIVE, w);
  };
  const bad = off(2e-6);
  assert.equal(bad.mismatches.length, 1);
  assert.match(bad.mismatches[0], /\$\.data_summary\.group_a_mean/);
  assert.deepEqual(off(5e-7).mismatches, []);
});

test('a count that differs by one fails', () => {
  const w = wasmFrom(NATIVE);
  w.analysed_target_count = 149;
  assert.equal(diffNativeWasm(NATIVE, w).mismatches.length, 1);
});

test('a number against a string, and a NaN on one side only, fail', () => {
  const w = wasmFrom(NATIVE);
  w.total_gap = String(w.total_gap);
  assert.equal(diffNativeWasm(NATIVE, w).mismatches.length, 1);
  const n = wasmFrom(NATIVE);
  n.total_gap = Number.NaN;
  assert.equal(diffNativeWasm(NATIVE, n).mismatches.length, 1);
});

test('arrays of different length fail', () => {
  const native = { ...structuredClone(NATIVE), detailed_explained: [{ coefficient: 1 }] };
  const r = diffNativeWasm(native, wasmFrom(NATIVE));
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /length native=1 wasm=0/);
});

test('two empty objects agree but fail the floor and every required path', () => {
  const r = diffNativeWasm({}, {});
  assert.deepEqual(r.mismatches, []);
  assert.equal(r.numericLeafCount, 0);
  const problems = coverageProblems(r);
  assert.match(problems[0], /only 0 numeric leaves compared, floor is 18/);
  assert.equal(problems.length, 1 + REQUIRED_NUMERIC_PATHS.length);
});

test('a payload that loses one required numeric path on both sides fails coverage', () => {
  const native = structuredClone(NATIVE);
  delete native.analysed_reference_count;
  const r = diffNativeWasm(native, wasmFrom(native));
  assert.deepEqual(r.mismatches, []);
  const problems = coverageProblems(r);
  assert.ok(problems.some((p) => p.includes('$.analysed_reference_count')));
  assert.ok(problems.some((p) => p.includes('floor is 18')));
});
