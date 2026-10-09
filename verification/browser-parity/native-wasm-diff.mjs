// D3 (0014-close round-1) / INV-02 cross-platform leg: recursive per-field diff between the
// native decompose_inner() JSON and the wasm decompose() JSON. Numeric leaves compare with
// |native - wasm| <= tolerance; every other leaf (string/bool/null) compares byte-equal (===).
//
// 0119-MERIDIAN S3: the comparator must not be able to pass vacuously.
//   * `serde_wasm_bindgen` drops `None` fields, so native `null` shows up as an ABSENT key in the
//     wasm output. That is the only asymmetry tolerated, and only at the paths in
//     ABSENT_AS_NULL_PATHS, and only when the side that has the key holds literally `null`.
//     A number (or anything else) against an absent key is a mismatch.
//   * The result reports how many numeric leaves were compared and at which paths, so a caller can
//     assert a floor (NUMERIC_LEAF_FLOOR) and a set of required paths (REQUIRED_NUMERIC_PATHS):
//     two empty objects diff to zero mismatches and must still fail the run.
//
// Kept in its own module (not inlined in parity.spec.mjs) so `node --test` can exercise it without
// a browser; importing parity.spec.mjs directly fails outside Playwright's test runner.
export const DEFAULT_TOLERANCE = 1e-6;

// The payload holds two decompositions of the same data, because each leaves out what the other has:
// `three_fold` has the interaction term but no per-predictor detail and no standard error; `two_fold` has the
// per-predictor detail rows and the bootstrap standard error but no interaction (0119 review F2: comparing the
// three-fold payload alone covered 18 scalars and not one coefficient-level number).
export const CASES = Object.freeze(['three_fold', 'two_fold']);

// Paths where one side may omit a key whose other side is `null` (TRUST-4): `serde_wasm_bindgen` drops `None`.
// Frozen and asserted verbatim by native-wasm-diff.test.mjs: widening it is a deliberate, reviewed change.
// Every entry is a field the engine leaves `None` for that case; the two_fold standard error is NOT here, so
// a wasm build that drops it fails as number-vs-absent.
export const ABSENT_AS_NULL_PATHS = Object.freeze([
  '$.three_fold.unexplained_standard_error',
  '$.three_fold.unresolved_row_keys',
  '$.two_fold.interaction_gap',
  '$.two_fold.interaction_percentage',
  '$.two_fold.unresolved_row_keys',
]);

const SCALAR_FIELDS = [
  'total_gap',
  'explained_gap',
  'unexplained_gap',
  'explained_percentage',
  'unexplained_percentage',
  'data_summary.total_count',
  'data_summary.group_a_count',
  'data_summary.group_b_count',
  'data_summary.group_a_mean',
  'data_summary.group_b_mean',
  'run_metadata.bootstrap_reps_requested',
  'run_metadata.bootstrap_reps_succeeded',
  'run_metadata.bootstrap_reps_discarded',
  'analysed_reference_count',
  'analysed_target_count',
  'adjustments_on_excluded_rows',
];
const DETAIL_FIELDS = ['estimate', 'std_err', 'p_value', 'ci_lower', 'ci_upper'];
// Fixture predictors (education, experience, tenure) plus the intercept the engine reports as a row.
const DETAIL_ROWS = 4;

function detailPaths(group) {
  const out = [];
  for (let i = 0; i < DETAIL_ROWS; i += 1) for (const f of DETAIL_FIELDS) out.push(`$.two_fold.${group}[${i}].${f}`);
  return out;
}

// Numeric paths that must be present in a native/wasm comparison, whatever else the payload holds.
export const REQUIRED_NUMERIC_PATHS = Object.freeze([
  ...SCALAR_FIELDS.map((f) => `$.three_fold.${f}`),
  '$.three_fold.interaction_gap',
  '$.three_fold.interaction_percentage',
  ...SCALAR_FIELDS.map((f) => `$.two_fold.${f}`),
  '$.two_fold.unexplained_standard_error',
  ...detailPaths('detailed_explained'),
  ...detailPaths('detailed_unexplained'),
]);

// Numeric leaves compared today (counted on engine/examples/native_baseline.rs output, 2026-10-09):
// 18 three-fold + 57 two-fold (16 scalars, the standard error, 8 detail rows x 5 numbers).
// Lower this only with a reason recorded in the 0119 issue Log.
export const NUMERIC_LEAF_FLOOR = 75;

function walk(native, wasm, path, tolerance, out) {
  if (typeof native === 'number' && typeof wasm === 'number') {
    out.numericPaths.push(path);
    const nativeNaN = Number.isNaN(native);
    const wasmNaN = Number.isNaN(wasm);
    if (nativeNaN || wasmNaN) {
      if (nativeNaN !== wasmNaN) {
        out.mismatches.push(`${path}: native=${native} wasm=${wasm} (NaN on only one side)`);
      }
      return;
    }
    const diff = Math.abs(native - wasm);
    if (diff > tolerance) {
      out.mismatches.push(`${path}: |native-wasm|=${diff} > ${tolerance} (native=${native}, wasm=${wasm})`);
    }
    return;
  }

  if (Array.isArray(native) && Array.isArray(wasm)) {
    if (native.length !== wasm.length) {
      out.mismatches.push(`${path}: length native=${native.length} wasm=${wasm.length}`);
      return;
    }
    native.forEach((v, i) => walk(v, wasm[i], `${path}[${i}]`, tolerance, out));
    return;
  }

  if (native !== null && wasm !== null && typeof native === 'object' && typeof wasm === 'object') {
    const keys = new Set([...Object.keys(native), ...Object.keys(wasm)]);
    for (const k of keys) {
      const childPath = `${path}.${k}`;
      const inNative = k in native;
      const inWasm = k in wasm;
      if (inNative && inWasm) {
        walk(native[k], wasm[k], childPath, tolerance, out);
        continue;
      }
      const present = inNative ? native[k] : wasm[k];
      if (ABSENT_AS_NULL_PATHS.includes(childPath) && present === null) {
        continue; // null on one side, key dropped on the other: the one tolerated asymmetry
      }
      out.mismatches.push(`${childPath}: key present in only one of native/wasm`);
    }
    return;
  }

  // Non-numeric leaves (string/bool/null) and type mismatches: byte-equal.
  if (native !== wasm) {
    out.mismatches.push(`${path}: native=${JSON.stringify(native)} wasm=${JSON.stringify(wasm)}`);
  }
}

// Returns { mismatches, numericLeafCount, numericPaths }. Empty `mismatches` alone does not mean the
// payloads agree: pass the result to coverageProblems() as well.
export function diffNativeWasm(native, wasm, path = '$', tolerance = DEFAULT_TOLERANCE) {
  const out = { mismatches: [], numericPaths: [] };
  walk(native, wasm, path, tolerance, out);
  return { mismatches: out.mismatches, numericLeafCount: out.numericPaths.length, numericPaths: out.numericPaths };
}

// Problems that make a comparison meaningless: too few numeric leaves, or a required path never compared.
export function coverageProblems(
  result,
  floor = NUMERIC_LEAF_FLOOR,
  required = REQUIRED_NUMERIC_PATHS
) {
  const problems = [];
  if (result.numericLeafCount < floor) {
    problems.push(`only ${result.numericLeafCount} numeric leaves compared, floor is ${floor}`);
  }
  const seen = new Set(result.numericPaths);
  for (const p of required) {
    if (!seen.has(p)) problems.push(`required numeric path not compared: ${p}`);
  }
  return problems;
}
