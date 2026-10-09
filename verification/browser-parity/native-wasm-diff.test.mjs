// Unit tests for the native <-> wasm comparator (0119-MERIDIAN S3, review F2). Run: `node --test` (no browser).
// They prove the comparator can fail: a comparator that cannot is why the old parity check was a false red
// for months and could have become a false green just as easily.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  ABSENT_AS_NULL_PATHS,
  CASES,
  NUMERIC_LEAF_FLOOR,
  REQUIRED_NUMERIC_PATHS,
  coverageProblems,
  diffNativeWasm,
} from './native-wasm-diff.mjs';

const SUMMARY = {
  total_count: 300,
  group_a_count: 150,
  group_b_count: 150,
  group_a_mean: 2.8320603882784767,
  group_b_mean: 3.740733535501663,
};
const META = {
  seed: '6840134480574414850',
  rng_algorithm: 'ChaCha8',
  rand_chacha_version: '0.3.1',
  bootstrap_reps_requested: 64,
  bootstrap_reps_succeeded: 64,
  bootstrap_reps_discarded: 0,
};
const row = (name, estimate, std_err, p_value, ci_lower, ci_upper) => ({ name, estimate, std_err, p_value, ci_lower, ci_upper });

// Shape of engine/examples/native_baseline.rs output (what decompose_inner() serializes for the two requests),
// numbers as emitted.
const NATIVE = Object.freeze({
  three_fold: {
    total_gap: 0.9086731472231868,
    explained_gap: 0.1212803477399896,
    unexplained_gap: 0.7542160167898229,
    interaction_gap: 0.033176782693374214,
    explained_percentage: 13.34697169280396,
    unexplained_percentage: 83.00190438053889,
    interaction_percentage: 3.651123926657138,
    detailed_explained: [],
    detailed_unexplained: [],
    data_summary: { ...SUMMARY },
    unexplained_standard_error: null,
    run_metadata: { ...META },
    unresolved_row_keys: null,
    analysed_reference_count: 150,
    analysed_target_count: 150,
    excluded_rows: [],
    adjustments_on_excluded_rows: 0,
  },
  two_fold: {
    total_gap: 0.9086731472231868,
    explained_gap: 0.13949279280134985,
    unexplained_gap: 0.7691803544218374,
    interaction_gap: null,
    explained_percentage: 15.351261697082796,
    unexplained_percentage: 84.64873830291725,
    interaction_percentage: null,
    detailed_explained: [
      row('__ob_intercept__', 0.0, 0.0, 1.0, 0.0, 0.0),
      row('education', 0.08843734748890879, 0.026546602008185368, 0.0, 0.041749451611121716, 0.1410275380017216),
      row('experience', 0.049474952309129704, 0.013373071919156543, 0.0, 0.029580934548372257, 0.07432952292014958),
      row('tenure', 0.0015804930033113584, 0.006011369936608489, 0.875, -0.012298245396968693, 0.010399985153902119),
    ],
    detailed_unexplained: [
      row('__ob_intercept__', 0.3668510091518924, 0.13353136625391704, 0.0, 0.09784259171634724, 0.5826893963097504),
      row('education', 0.32755039170276934, 0.1360265997235263, 0.0, 0.07981378686114723, 0.6230527576188665),
      row('experience', 0.07018771184505318, 0.03934699438108443, 0.09375, -0.02094819692367139, 0.1414993380403597),
      row('tenure', 0.004591241722121973, 0.03185175621839614, 1.0, -0.051357946758268255, 0.06930626775627252),
    ],
    data_summary: { ...SUMMARY },
    unexplained_standard_error: 0.016717583223865955,
    run_metadata: { ...META },
    unresolved_row_keys: null,
    analysed_reference_count: 150,
    analysed_target_count: 150,
    excluded_rows: [],
    adjustments_on_excluded_rows: 0,
  },
});

// What serde_wasm_bindgen hands back: `None` fields are dropped, not null.
function wasmFrom(native) {
  const w = structuredClone(native);
  delete w.three_fold.unexplained_standard_error;
  delete w.three_fold.unresolved_row_keys;
  delete w.two_fold.interaction_gap;
  delete w.two_fold.interaction_percentage;
  delete w.two_fold.unresolved_row_keys;
  return w;
}

test('the absent-as-null allowlist is exactly the five known None fields and cannot be widened at runtime', () => {
  assert.deepEqual([...ABSENT_AS_NULL_PATHS], [
    '$.three_fold.unexplained_standard_error',
    '$.three_fold.unresolved_row_keys',
    '$.two_fold.interaction_gap',
    '$.two_fold.interaction_percentage',
    '$.two_fold.unresolved_row_keys',
  ]);
  assert.ok(Object.isFrozen(ABSENT_AS_NULL_PATHS));
  assert.throws(() => {
    'use strict';
    ABSENT_AS_NULL_PATHS.push('$.two_fold.unexplained_standard_error');
  });
  assert.ok(!ABSENT_AS_NULL_PATHS.includes('$.two_fold.unexplained_standard_error'), 'the two-fold standard error must be compared');
});

test('the payload holds both decompositions', () => {
  assert.deepEqual([...CASES], ['three_fold', 'two_fold']);
  assert.deepEqual(Object.keys(NATIVE), [...CASES]);
});

test('real-shaped payload: null vs absent on the allowlisted paths passes and compares 75 numeric leaves', () => {
  const r = diffNativeWasm(NATIVE, wasmFrom(NATIVE));
  assert.deepEqual(r.mismatches, []);
  assert.equal(r.numericLeafCount, 75);
  assert.deepEqual(coverageProblems(r), []);
});

test('the floor and the required paths describe the real payload exactly', () => {
  const r = diffNativeWasm(NATIVE, wasmFrom(NATIVE));
  assert.equal(NUMERIC_LEAF_FLOOR, r.numericLeafCount);
  assert.deepEqual([...REQUIRED_NUMERIC_PATHS].sort(), [...r.numericPaths].sort());
});

test('the required paths include coefficient-level numbers and the bootstrap standard error', () => {
  const need = new Set(REQUIRED_NUMERIC_PATHS);
  for (const p of [
    '$.two_fold.unexplained_standard_error',
    '$.two_fold.detailed_explained[1].estimate',
    '$.two_fold.detailed_explained[3].ci_upper',
    '$.two_fold.detailed_unexplained[0].std_err',
    '$.two_fold.detailed_unexplained[3].p_value',
    '$.three_fold.interaction_gap',
  ]) {
    assert.ok(need.has(p), `${p} must be required`);
  }
});

test('the previous three-fold-only payload now fails coverage (it compared no coefficient and no standard error)', () => {
  const r = diffNativeWasm({ three_fold: NATIVE.three_fold }, { three_fold: wasmFrom(NATIVE).three_fold });
  assert.deepEqual(r.mismatches, []);
  assert.equal(r.numericLeafCount, 18);
  const problems = coverageProblems(r);
  assert.ok(problems.some((p) => p.includes('floor is 75')));
  assert.ok(problems.some((p) => p.includes('$.two_fold.detailed_explained[0].estimate')));
  assert.ok(problems.some((p) => p.includes('$.two_fold.unexplained_standard_error')));
});

test('null on the wasm side and absent on the native side also passes on an allowlisted path', () => {
  const native = wasmFrom(NATIVE);
  const wasm = structuredClone(NATIVE);
  assert.deepEqual(diffNativeWasm(native, wasm).mismatches, []);
});

test('a number against an absent key fails, even on an allowlisted path', () => {
  const native = structuredClone(NATIVE);
  native.three_fold.unexplained_standard_error = 0.0123;
  const r = diffNativeWasm(native, wasmFrom(NATIVE));
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.three_fold\.unexplained_standard_error: key present in only one/);
});

test('a wasm build that drops the two-fold standard error fails', () => {
  const w = wasmFrom(NATIVE);
  delete w.two_fold.unexplained_standard_error;
  const r = diffNativeWasm(NATIVE, w);
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.two_fold\.unexplained_standard_error: key present in only one/);
});

test('the two-fold standard error 2e-6 off fails, 5e-7 off passes', () => {
  const off = (delta) => {
    const w = wasmFrom(NATIVE);
    w.two_fold.unexplained_standard_error += delta;
    return diffNativeWasm(NATIVE, w);
  };
  assert.match(off(2e-6).mismatches[0], /\$\.two_fold\.unexplained_standard_error/);
  assert.deepEqual(off(5e-7).mismatches, []);
});

test('drift in one detail coefficient fails, naming the row and the field', () => {
  const w = wasmFrom(NATIVE);
  w.two_fold.detailed_unexplained[1].estimate += 3e-6;
  const r = diffNativeWasm(NATIVE, w);
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.two_fold\.detailed_unexplained\[1\]\.estimate/);
});

test('a detail row with a different name, or a missing detail row, fails', () => {
  const renamed = wasmFrom(NATIVE);
  renamed.two_fold.detailed_explained[2].name = 'tenure';
  assert.match(diffNativeWasm(NATIVE, renamed).mismatches[0], /detailed_explained\[2\]\.name/);
  const short = wasmFrom(NATIVE);
  short.two_fold.detailed_explained.pop();
  assert.match(diffNativeWasm(NATIVE, short).mismatches[0], /length native=4 wasm=3/);
});

test('null against an absent key fails on a path that is not allowlisted', () => {
  const native = structuredClone(NATIVE);
  native.two_fold.some_other_field = null;
  const r = diffNativeWasm(native, wasmFrom(NATIVE));
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.two_fold\.some_other_field: key present in only one/);
});

test('an allowlisted path of the other case is not tolerated (interaction_gap is a number in three_fold)', () => {
  const w = wasmFrom(NATIVE);
  delete w.three_fold.interaction_gap;
  const r = diffNativeWasm(NATIVE, w);
  assert.equal(r.mismatches.length, 1);
  assert.match(r.mismatches[0], /\$\.three_fold\.interaction_gap: key present in only one/);
});

test('a numeric leaf 2e-6 off fails, 5e-7 off passes', () => {
  const off = (delta) => {
    const w = wasmFrom(NATIVE);
    w.three_fold.data_summary.group_a_mean += delta;
    return diffNativeWasm(NATIVE, w);
  };
  const bad = off(2e-6);
  assert.equal(bad.mismatches.length, 1);
  assert.match(bad.mismatches[0], /\$\.three_fold\.data_summary\.group_a_mean/);
  assert.deepEqual(off(5e-7).mismatches, []);
});

test('a count that differs by one fails', () => {
  const w = wasmFrom(NATIVE);
  w.two_fold.analysed_target_count = 149;
  assert.equal(diffNativeWasm(NATIVE, w).mismatches.length, 1);
});

test('a number against a string, and a NaN on one side only, fail', () => {
  const w = wasmFrom(NATIVE);
  w.three_fold.total_gap = String(w.three_fold.total_gap);
  assert.equal(diffNativeWasm(NATIVE, w).mismatches.length, 1);
  const n = wasmFrom(NATIVE);
  n.two_fold.total_gap = Number.NaN;
  assert.equal(diffNativeWasm(NATIVE, n).mismatches.length, 1);
});

test('two empty objects agree but fail the floor and every required path', () => {
  const r = diffNativeWasm({}, {});
  assert.deepEqual(r.mismatches, []);
  assert.equal(r.numericLeafCount, 0);
  const problems = coverageProblems(r);
  assert.match(problems[0], /only 0 numeric leaves compared, floor is 75/);
  assert.equal(problems.length, 1 + REQUIRED_NUMERIC_PATHS.length);
});

test('a payload that loses one required numeric path on both sides fails coverage', () => {
  const native = structuredClone(NATIVE);
  delete native.two_fold.analysed_reference_count;
  const r = diffNativeWasm(native, wasmFrom(native));
  assert.deepEqual(r.mismatches, []);
  const problems = coverageProblems(r);
  assert.ok(problems.some((p) => p.includes('$.two_fold.analysed_reference_count')));
  assert.ok(problems.some((p) => p.includes('floor is 75')));
});

test('a payload whose two-fold detail rows are empty on both sides fails coverage', () => {
  const native = structuredClone(NATIVE);
  native.two_fold.detailed_explained = [];
  native.two_fold.detailed_unexplained = [];
  const r = diffNativeWasm(native, wasmFrom(native));
  assert.deepEqual(r.mismatches, []);
  assert.ok(coverageProblems(r).some((p) => p.includes('$.two_fold.detailed_explained[0].estimate')));
});
