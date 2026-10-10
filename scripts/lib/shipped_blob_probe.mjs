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
// 0122-MERIDIAN adds two checks on the engine's own committed Fixture F (engine/tests/fixtures/0118-fixture-f.csv,
// whose reference group is paid exactly 30000 + 1000 * Experience + 4000 * Level, so every expected figure is
// worked out from the file's cells and never read from the engine):
//
//   target_gap_rule          a reachable group target costs n x (target - start gap) under Greedy and Equitable alike
//                            and lands the group on the target; at a 2 %, 3 % and 5 % threshold the lowest reachable
//                            gap is the start gap plus the eligible shortfalls over n (not the old
//                            -mean(max(0, wage - fair))), a target above it is refused as unreachable and pays the
//                            same amounts as no target.
//   reference_raise_closure  with the reference group raised too (five reference people paid under their line), the
//                            two costs are the sums of the rows by source, the share is cost / need of the compared
//                            group, and the gap left equals an independent refit of the reference group's adjusted
//                            wages (and a VERIFY of the same schedule) - not the old arithmetic that credited the
//                            reference raises to the compared group.
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


// ---- 0122: Fixture F, worked out from its cells ---------------------------------------------------
const fixtureFText = readFileSync(join(import.meta.dirname, '..', '..', 'engine', 'tests', 'fixtures', '0118-fixture-f.csv'), 'utf8')
const fRequest = (text, over = {}) => ({
  csv_data: enc(text),
  outcome_variable: 'Salary',
  group_variable: 'Gender',
  reference_group: 'Male',
  predictors: ['Experience', 'Level'],
  categorical_predictors: null,
  three_fold: false,
  quantile: null,
  reference_coefficients: 'GroupB',
  bootstrap_reps: 0,
  forensic_mode: true,
  ...over,
})
const fOptimize = (text, over = {}) => ({
  ...fRequest(text),
  budget: 0,
  target_gap: null,
  target: 'Reference',
  strategy: 'Greedy',
  min_gap_pct: 0,
  adjust_both_groups: false,
  confidence_level: 0.95,
  range_target: 'Midpoint',
  ...over,
})
/** Every row of Fixture F as its cells give it; `blank` rows are left out of the model by the engine. */
function fRows(text) {
  const c = parse(text)
  const ix = (n) => c.header.indexOf(n)
  return c.rows.map((r, ordinal) => {
    const blank = [r[ix('Salary')], r[ix('Experience')], r[ix('Level')]].some((x) => x === '')
    const pay = Number(r[ix('Salary')])
    const exp = Number(r[ix('Experience')])
    const lvl = Number(r[ix('Level')])
    return { ordinal, name: r[ix('Name')], gender: r[ix('Gender')], pay, exp, lvl, blank, fair: 30000 + 1000 * exp + 4000 * lvl }
  })
}
/** The same file with the first `n` reference (Male) people paid 3 000 under their own line. */
function lowerReference(text, n) {
  const c = parse(text)
  const g = c.header.indexOf('Gender')
  const sal = c.header.indexOf('Salary')
  let k = 0
  for (const r of c.rows) if (r[g] === 'Male' && r[sal] !== '' && k++ < n) r[sal] = String(Number(r[sal]) - 3000)
  return csvOf(c)
}
const sum = (xs) => xs.reduce((a, b) => a + b, 0)
const near = (a, b, tol) => Number.isFinite(a) && Number.isFinite(b) && Math.abs(a - b) <= tol

/** The compared group's figures and the eligible set under a threshold, from the cells. */
function comparedFigures(rows, pct) {
  const people = rows.filter((r) => r.gender === 'Female' && !r.blank)
  const n = people.length
  const short = (p) => Math.max(0, p.fair - p.pay)
  const ties = people.filter((p) => short(p) > 0 && Math.abs(short(p) / p.pay - pct) < 1e-9).length
  const eligible = people.filter((p) => short(p) > 0 && short(p) / p.pay > pct)
  const excludedCount = people.filter((p) => short(p) > 0).length - eligible.length
  const g0 = sum(people.map((p) => p.pay - p.fair)) / n
  const need = sum(people.map(short))
  const eligibleNeed = sum(eligible.map(short))
  return {
    n, g0, need, eligibleNeed, ties, excludedCount,
    eligibleCount: eligible.length,
    floor: g0 + eligibleNeed / n,
    // the arithmetic 0122 replaced (plan V2 plant): the mean overpay, signed negative, ignoring the threshold and the start gap
    oldFloor: -sum(people.map((p) => Math.max(0, p.pay - p.fair))) / n,
  }
}

/** target_gap_rule on one artifact: the committed Fixture F, V1 and V2 of the plan. */
function targetGapRule(engine) {
  const rows = fRows(fixtureFText)
  const base = comparedFigures(rows, 0)
  const T = base.g0 / 2
  const run = (over) => engine.optimize(fOptimize(fixtureFText, over))
  const greedy = run({ target_gap: T })
  const equitable = run({ target_gap: T, strategy: 'Equitable' })
  const wantCost = base.n * (T - base.g0)
  const reachable = {
    target: T, wanted_cost: wantCost,
    greedy_cost: greedy.cost_target, equitable_cost: equitable.cost_target,
    greedy_gap: greedy.new_unexplained_gap, equitable_gap: equitable.new_unexplained_gap,
    reachable_flag: greedy.target_gap_reachable,
    ok: near(greedy.cost_target, wantCost, 1e-6) && near(equitable.cost_target, wantCost, 1e-6) && near(greedy.new_unexplained_gap, T, 1e-6)
      && near(equitable.new_unexplained_gap, T, 1e-6) && greedy.target_gap_reachable === true && equitable.target_gap_reachable === true,
  }
  const thresholds = []
  for (const pct of [0, 0.02, 0.03, 0.05]) {
    const f = comparedFigures(rows, pct)
    const withTarget = run({ target_gap: 0, min_gap_pct: pct })
    const without = run({ target_gap: null, min_gap_pct: pct })
    const wantReachable = 0 <= f.floor + 1e-9
    const sameAmounts = withTarget.adjustments.length === without.adjustments.length
      && withTarget.adjustments.every((a, i) => a.index === without.adjustments[i].index && near(a.adjustment, without.adjustments[i].adjustment, 1e-9))
    const row = {
      min_gap_pct: pct, ties_with_threshold: f.ties, eligible_people: f.eligibleCount, hand_excluded_people: f.excludedCount, engine_excluded_people: withTarget.threshold_excluded_count, hand_floor: f.floor, engine_floor: withTarget.best_reachable_gap,
      old_rule_floor: f.oldFloor, hand_cost_eligible_need: f.eligibleNeed, engine_cost: withTarget.cost_target,
      wanted_reachable: wantReachable, engine_reachable: withTarget.target_gap_reachable, same_amounts_as_no_target: sameAmounts,
    }
    row.ok = f.ties === 0 && near(withTarget.best_reachable_gap, f.floor, 1e-6) && withTarget.target_gap_reachable === wantReachable
      && withTarget.threshold_excluded_count === f.excludedCount
      && (wantReachable ? near(withTarget.new_unexplained_gap, 0, 1e-6) : sameAmounts && near(withTarget.cost_target, f.eligibleNeed, 1e-6))
    thresholds.push(row)
  }
  // teeth: the comparator must see (a) the old floor rule, which differs from the right one at a threshold, and (b) a 0.01 nudge
  const at5 = thresholds.find((t) => t.min_gap_pct === 0.05)
  const sees_old_rule = thresholds.some((t) => t.min_gap_pct > 0 && !near(t.engine_floor, t.old_rule_floor, 1e-6))
  const sees_nudge = at5 !== undefined && !near(at5.engine_floor + 0.01, at5.hand_floor, 1e-6)
  const unreachableCovered = thresholds.some((t) => t.wanted_reachable === false && t.engine_reachable === false)
  return {
    ok: reachable.ok && thresholds.every((t) => t.ok) && sees_old_rule && sees_nudge && unreachableCovered,
    reachable, thresholds, comparator_sees_the_old_floor_rule: sees_old_rule, comparator_sees_a_nudge: sees_nudge, an_unreachable_target_was_exercised: unreachableCovered,
  }
}

/** reference_raise_closure on one artifact: V3 and V5 of the plan, on a file where reference people sit under their line. */
function referenceRaiseClosure(engine) {
  const text = lowerReference(fixtureFText, 5)
  const rows = fRows(text)
  const res = engine.optimize(fOptimize(text, { adjust_both_groups: true }))
  const adj = new Map(res.adjustments.map((a) => [a.index, a]))
  const byGender = (g) => sum(res.adjustments.filter((a) => rows[a.index].gender === g).map((a) => a.adjustment))
  const costCompared = byGender('Female')
  const costReference = byGender('Male')
  const sourcesAgree = res.adjustments.every((a) => (a.source === 'Reference') === (rows[a.index].gender === 'Male'))
  // the independent refit: the reference group's adjusted wages, their own line, the compared group's mean shortfall against it
  const adjusted = (r) => r.pay + (adj.get(r.ordinal)?.adjustment ?? 0)
  const men = rows.filter((r) => r.gender === 'Male' && !r.blank)
  const women = rows.filter((r) => r.gender === 'Female' && !r.blank)
  const beta = ols(men.map((r) => [1, r.exp, r.lvl]), men.map(adjusted))
  const gapOracle = sum(women.map((r) => adjusted(r) - (beta[0] + beta[1] * r.exp + beta[2] * r.lvl))) / women.length
  const verify = engine.verify_adjustments({
    ...fRequest(text),
    adjustments: res.adjustments.map((a) => ({ index: a.index, row_key: a.row_key, value: a.adjustment, predictor_overrides: null })),
  })
  const start = sum(women.map((r) => r.pay - r.fair)) / women.length
  const creditedToCompared = start + res.total_cost / women.length // the arithmetic 0122 replaced
  const f = {
    cost_compared_by_rows: costCompared, cost_reference_by_rows: costReference,
    engine_cost_target: res.cost_target, engine_cost_reference: res.cost_reference, engine_total_cost: res.total_cost,
    reference_people_raised: res.adjustments.filter((a) => a.source === 'Reference' && a.adjustment > 0).length,
    independent_refit_gap: gapOracle, engine_new_unexplained_gap: res.new_unexplained_gap, verify_gap: verify.unexplained_gap,
    old_arithmetic_gap: creditedToCompared, closure: res.closure, need_target: res.need_target,
  }
  const sees_old = !near(creditedToCompared, res.new_unexplained_gap, 1e-6)
  const sees_nudge = !near(res.new_unexplained_gap + 0.01, gapOracle, 1e-6)
  return {
    ok: sourcesAgree && f.reference_people_raised > 0 && near(res.cost_target, costCompared, 1e-6) && near(res.cost_reference, costReference, 1e-6)
      && near(res.total_cost, costCompared + costReference, 1e-6) && near(res.closure, res.cost_target / res.need_target, 1e-9)
      && near(res.new_unexplained_gap, gapOracle, 1e-6) && near(verify.unexplained_gap, gapOracle, 1e-6) && sees_old && sees_nudge,
    sources_agree_with_the_file: sourcesAgree, ...f, comparator_sees_the_old_arithmetic: sees_old, comparator_sees_a_nudge: sees_nudge,
  }
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
    r.checks.target_gap_rule = targetGapRule(engine)
    r.checks.reference_raise_closure = referenceRaiseClosure(engine)
    r.checks.t8_zero_budget_identity = { ok: t8Ok && refOk && teeth, pooled: t8, reference: ref, comparator_sees_a_shift: teeth, tolerance_engine_vs_engine: 1e-9, tolerance_vs_independent_fit: 1e-8 }
  } catch (e) {
    r.error = String(e?.stack ?? e).slice(0, 600)
    out.ok = false
  }
  for (const c of Object.values(r.checks)) if (!c.ok || c.comparator_sees_a_nudge === false) out.ok = false
  out.artifacts[name] = r
}
console.log(JSON.stringify(out))
