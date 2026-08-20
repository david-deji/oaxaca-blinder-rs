// D3 (0014-close round-1) / INV-02 cross-platform leg: recursive per-field diff between the
// native decompose_inner() JSON and the wasm decompose() JSON. Numeric leaves compare with
// |native - wasm| <= tolerance; every other leaf (string/bool/null) compares byte-equal (===).
// Returns an array of human-readable mismatch descriptions — empty means the two payloads match
// within tolerance.
//
// Kept in its own module (not inlined in parity.spec.mjs) so it can be exercised by a plain
// `node` unit run against native-CLI-only fixtures when a real browser/WASM run isn't available
// — importing parity.spec.mjs directly fails outside Playwright's test runner because it calls
// `test()` at module scope.
//
// No non-numeric-field exemptions are needed today: `RunMetadata.seed` is serialized as a
// decimal string but is the same DEFAULT_SEED on both sides, so it still compares byte-equal
// under the default rule.
export const DEFAULT_TOLERANCE = 1e-6;

export function diffNativeWasm(native, wasm, path = '$', tolerance = DEFAULT_TOLERANCE) {
  const mismatches = [];

  if (typeof native === 'number' && typeof wasm === 'number') {
    const nativeNaN = Number.isNaN(native);
    const wasmNaN = Number.isNaN(wasm);
    if (nativeNaN || wasmNaN) {
      if (nativeNaN !== wasmNaN) {
        mismatches.push(`${path}: native=${native} wasm=${wasm} (NaN on only one side)`);
      }
      return mismatches;
    }
    const diff = Math.abs(native - wasm);
    if (diff > tolerance) {
      mismatches.push(`${path}: |native-wasm|=${diff} > ${tolerance} (native=${native}, wasm=${wasm})`);
    }
    return mismatches;
  }

  if (Array.isArray(native) && Array.isArray(wasm)) {
    if (native.length !== wasm.length) {
      mismatches.push(`${path}: length native=${native.length} wasm=${wasm.length}`);
      return mismatches;
    }
    native.forEach((v, i) => mismatches.push(...diffNativeWasm(v, wasm[i], `${path}[${i}]`, tolerance)));
    return mismatches;
  }

  if (native !== null && wasm !== null && typeof native === 'object' && typeof wasm === 'object') {
    const keys = new Set([...Object.keys(native), ...Object.keys(wasm)]);
    for (const k of keys) {
      if (!(k in native) || !(k in wasm)) {
        mismatches.push(`${path}.${k}: key present in only one of native/wasm`);
        continue;
      }
      mismatches.push(...diffNativeWasm(native[k], wasm[k], `${path}.${k}`, tolerance));
    }
    return mismatches;
  }

  // Non-numeric leaves (string/bool/null) and type mismatches: byte-equal.
  if (native !== wasm) {
    mismatches.push(`${path}: native=${JSON.stringify(native)} wasm=${JSON.stringify(wasm)}`);
  }
  return mismatches;
}
