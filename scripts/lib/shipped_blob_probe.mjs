// shipped_blob_probe.mjs: 0120-MERIDIAN engine receipt, the part that runs the SHIPPED blobs.
//
// Usage: node scripts/lib/shipped_blob_probe.mjs <app frontend/src dir>
//
// Loads the two blobs the app ships (src/wasm, src/wasm-threaded), exactly as the app's real-blob specs do
// (initSync over the _bg.wasm bytes), and runs on the engine repo's own 10 000-row employers file:
//
//   relabel   Engineering is renamed zz_Engineering (it stops being the alphabetical base): every Department
//             row of detailed_unexplained maps one to one and agrees to 1e-9.
//   carve-out the first 20 Engineering rows become a new department Legal: every untouched department moves
//             by less than 1e-4 (the oracle test measured 3.4e-6 for population shares, 4.7e-3 for equal shares).
//   T8        optimize(target Pooled) starts from the gap decompose(Pooled) reports, and both equal the group
//             coefficient of an independent pooled least-squares fit with a target-group indicator; the same
//             identity for Reference/GroupB against an independent fit on the reference group alone.
//
// Each comparison is also run once on data with one figure nudged and must report it, so a pass is not a
// comparator that sees nothing. Prints one JSON object on stdout; exit 0 always (the caller reads "ok").
import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

globalThis.self ??= globalThis
globalThis.addEventListener ??= () => {}
globalThis.removeEventListener ??= () => {}

const frontendSrc = resolve(process.argv[2] ?? '')
const fixtures = join(import.meta.dirname, '..', '..', 'oaxaca_blinder', 'tests', 'fixtures')
const employersText = readFileSync(join(fixtures, 'employers_trust_fixture.csv'), 'utf8')
const enc = (t) => new TextEncoder().encode(t)

function parse(text) {
  const lines = text.split(/\r?\n/).filter(Boolean)
  return { header: lines[0].split(','), rows: lines.slice(1).map((l) => l.split(',')) }
}
const csvOf = ({ header, rows }) => [header.join(','), ...rows.map((r) => r.join(','))].join('\n') + '\n'
function editDepartment(text, fn) {
  const c = parse(text)
  const i = c.header.indexOf('Department')
  fn(c.rows, i)
  return csvOf(c)
}
const renamed = editDepartment(employersText, (rows, i) => rows.forEach((r) => { if (r[i] === 'Engineering') r[i] = 'zz_Engineering' }))
const carved = editDepartment(employersText, (rows, i) => {
  let left = 20
  for (const r of rows) if (left > 0 && r[i] === 'Engineering') { r[i] = 'Legal'; left -= 1 }
})

const request = (text, over = {}) => ({
  csv_data: enc(text),
  outcome_variable: 'log_salary',
  group_variable: 'Gender',
  reference_group: 'Male',
  predictors: ['Age', 'Experience_Years'],
  categorical_predictors: ['Department'],
  three_fold: false,
  quantile: null,
  reference_coefficients: 'GroupB',
  bootstrap_reps: 50,
  forensic_mode: true,
  ...over,
})
const optimizeRequest = (text, over = {}) => ({
  csv_data: enc(text),
  outcome_variable: 'log_salary',
  group_variable: 'Gender',
  reference_group: 'Male',
  predictors: ['Age', 'Experience_Years'],
  categorical_predictors: ['Department'],
  budget: 0,
  target_gap: null,
  target: 'Reference',
  strategy: 'Greedy',
  min_gap_pct: 0,
  forensic_mode: true,
  adjust_both_groups: false,
  confidence_level: 0.95,
  range_target: 'Midpoint',
  ...over,
})

/** name -> estimate for the Department levels of a detailed_unexplained list. */
const deptRows = (d) => Object.fromEntries(d.detailed_unexplained.filter((e) => e.name.startsWith('Department_')).map((e) => [e.name, e.estimate]))

function relabelDiff(before, after) {
  let worst = 0
  const missing = []
  for (const [name, x] of Object.entries(before)) {
    const to = name === 'Department_Engineering' ? 'Department_zz_Engineering' : name
    if (!(to in after)) { missing.push(name); continue }
    worst = Math.max(worst, Math.abs(after[to] - x))
  }
  return { worst, missing, rows: Object.keys(before).length, same: Object.keys(before).length === Object.keys(after).length }
}
function carveDiff(before, after) {
  let worst = 0
  const missing = []
  for (const [name, x] of Object.entries(before)) {
    if (name === 'Department_Engineering') continue
    if (!(name in after)) { missing.push(name); continue }
    worst = Math.max(worst, Math.abs(after[name] - x))
  }
  return { worst, missing, hasLegal: 'Department_Legal' in after, hasDonor: 'Department_Engineering' in after }
}

// ---- independent least squares over the CSV (no engine call) -------------------------------------
function solve(A, b) {
  const n = b.length
  const M = A.map((r, i) => [...r, b[i]])
  for (let c = 0; c < n; c++) {
    let p = c
    for (let r = c + 1; r < n; r++) if (Math.abs(M[r][c]) > Math.abs(M[p][c])) p = r
    ;[M[c], M[p]] = [M[p], M[c]]
    for (let r = c + 1; r < n; r++) {
      const f = M[r][c] / M[c][c]
      for (let k = c; k <= n; k++) M[r][k] -= f * M[c][k]
    }
  }
  const x = new Array(n).fill(0)
  for (let r = n - 1; r >= 0; r--) {
    let s = M[r][n]
    for (let k = r + 1; k < n; k++) s -= M[r][k] * x[k]
    x[r] = s / M[r][r]
  }
  return x
}
function ols(X, y) {
  const k = X[0].length
  const A = Array.from({ length: k }, () => new Array(k).fill(0))
  const b = new Array(k).fill(0)
  for (let i = 0; i < X.length; i++) {
    for (let a = 0; a < k; a++) {
      b[a] += X[i][a] * y[i]
      for (let c = a; c < k; c++) A[a][c] += X[i][a] * X[i][c]
    }
  }
  for (let a = 0; a < k; a++) for (let c = 0; c < a; c++) A[a][c] = A[c][a]
  return solve(A, b)
}
function independent(text) {
  const { header, rows } = parse(text)
  const ix = (n) => header.indexOf(n)
  const depts = [...new Set(rows.map((r) => r[ix('Department')]))].sort().slice(1)
  const design = (r, extra) => [1, Number(r[ix('Age')]), Number(r[ix('Experience_Years')]), ...depts.map((d) => (r[ix('Department')] === d ? 1 : 0)), ...extra]
  const y = (r) => Number(r[ix('log_salary')])
  const female = rows.filter((r) => r[ix('Gender')] === 'Female')
  const pooledBeta = ols(rows.map((r) => design(r, [r[ix('Gender')] === 'Female' ? 1 : 0])), rows.map(y))
  const gamma = pooledBeta[pooledBeta.length - 1]
  // Reference line: fit on the Male rows alone, read the compared group's mean shortfall against it.
  const male = rows.filter((r) => r[ix('Gender')] === 'Male')
  const bm = ols(male.map((r) => design(r, [])), male.map(y))
  const dot = (a, b) => a.reduce((s, v, i) => s + v * b[i], 0)
  const refGap = female.reduce((s, r) => s + (y(r) - dot(design(r, []), bm)), 0) / female.length
  return { gamma, refGap }
}

async function load(dir, threaded) {
  const mod = await import(pathToFileURL(join(frontendSrc, dir, 'pay_equity_engine.js')).href)
  mod.initSync({ module: readFileSync(join(frontendSrc, dir, 'pay_equity_engine_bg.wasm')) })
  void threaded
  return mod
}

const out = { ok: true, frontend_src_given: Boolean(process.argv[2]), artifacts: {} }
const indep = independent(employersText)
out.independent = indep

for (const [name, dir, threaded] of [['sequential', 'wasm', false], ['threaded', 'wasm-threaded', true]]) {
  const r = { checks: {} }
  try {
    const engine = await load(dir, threaded)
    const base = deptRows(engine.decompose(request(employersText)))
    const re = deptRows(engine.decompose(request(renamed)))
    const ca = deptRows(engine.decompose(request(carved)))
    const rd = relabelDiff(base, re)
    const cd = carveDiff(base, ca)
    // the same comparators on data with one figure nudged
    const reN = { ...re, Department_HR: re.Department_HR + 0.01 }
    const caN = { ...ca, Department_HR: ca.Department_HR + 0.01 }
    const rdN = relabelDiff(base, reN)
    const cdN = carveDiff(base, caN)
    r.checks.relabel = {
      ok: rd.worst <= 1e-9 && rd.missing.length === 0 && rd.same,
      rows: rd.rows, worst_abs_difference: rd.worst, limit: 1e-9,
      comparator_sees_a_nudge: rdN.worst > 1e-9,
    }
    r.checks.carve_out = {
      ok: cd.worst < 1e-4 && cd.missing.length === 0 && cd.hasLegal && cd.hasDonor,
      untouched_departments: Object.keys(base).length - 1, worst_untouched_move: cd.worst, limit: 1e-4,
      comparator_sees_a_nudge: cdN.worst >= 1e-4,
    }
    // T8 and the Reference identity
    const dPool = engine.decompose(request(employersText, { reference_coefficients: 'Pooled' }))
    const oPool = engine.optimize(optimizeRequest(employersText, { target: 'Pooled' }))
    const dRef = engine.decompose(request(employersText, { reference_coefficients: 'GroupB' }))
    const oRef = engine.optimize(optimizeRequest(employersText, { target: 'Reference' }))
    const close = (a, b, tol) => Math.abs(a - b) <= tol * Math.max(1, Math.abs(b))
    const t8 = {
      decompose_pooled: dPool.unexplained_gap,
      optimize_pooled_original: oPool.original_unexplained_gap,
      independent_group_indicator_coefficient: indep.gamma,
      decompose_scheme: dPool.run_metadata?.reference_coefficients_used,
    }
    const ref = {
      decompose_groupb: dRef.unexplained_gap,
      optimize_reference_original: oRef.original_unexplained_gap,
      independent_mean_shortfall_against_reference_line: indep.refGap,
      decompose_scheme: dRef.run_metadata?.reference_coefficients_used,
    }
    const t8Ok = close(t8.decompose_pooled, t8.optimize_pooled_original, 1e-9) && close(t8.decompose_pooled, indep.gamma, 1e-8) && t8.decompose_scheme === 'Pooled'
    const refOk = close(ref.decompose_groupb, ref.optimize_reference_original, 1e-9) && close(ref.decompose_groupb, indep.refGap, 1e-8) && ref.decompose_scheme === 'GroupB'
    // teeth: the pre-T8 behaviour (Pooled optimiser without the indicator) would not equal gamma; the comparator must see a 1% shift
    const teeth = !close(t8.optimize_pooled_original * 1.01, t8.decompose_pooled, 1e-9) && !close(ref.optimize_reference_original + 0.01, ref.decompose_groupb, 1e-9)
    r.checks.t8_zero_budget_identity = { ok: t8Ok && refOk && teeth, pooled: t8, reference: ref, comparator_sees_a_shift: teeth, tolerance_engine_vs_engine: 1e-9, tolerance_vs_independent_fit: 1e-8 }
  } catch (e) {
    r.error = String(e?.stack ?? e).slice(0, 600)
    out.ok = false
  }
  for (const c of Object.values(r.checks)) if (!c.ok || c.comparator_sees_a_nudge === false) out.ok = false
  out.artifacts[name] = r
}
console.log(JSON.stringify(out))
