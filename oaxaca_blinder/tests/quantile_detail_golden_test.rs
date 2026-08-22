//! Per-predictor RIF quantile-detail golden — R `ddecompose` oracle (0014-MERIDIAN, AC-6).
//!
//! Validates the SHIPPED one-stage RIF-OLS quantile decomposition
//! (`OaxacaBuilder::decompose_quantile`) against R `ddecompose::ob_decompose(..., reweighting=FALSE)`
//! — an independent, field-standard reference (ddecompose computes its own composition/structure
//! effects; it does NOT mirror the engine source). OFFLINE: reads `employers_trust_fixture.csv` +
//! `trust_goldens_r.json`.
//!
//! Three assertion layers, strongest first (a wrong engine result must fail at least one):
//!  1. KEY-SET guard — engine's per-predictor detail key set == ddecompose golden's key set, per
//!     tau, asserted BEFORE any value comparison. Defeats the vacuous-pass failure: without it, a
//!     renamed/empty `detailed_*` Vec would skip every comparison and a zero-initialised max-diff
//!     accumulator would pass `<= tol` on ZERO comparisons (agentic failure mode #9). Mirrors the
//!     design-column SET guard in `trust_golden_r_test.rs` (AC-3).
//!  2. AGGREGATE vs ddecompose — engine two-fold aggregate composition/structure vs ddecompose's,
//!     per tau. This is the headline "which effect drives the tail gap" number and the primary,
//!     best-conditioned oracle gate at the well-estimated quantiles.
//!  3. Per-predictor vs ddecompose — every covariate, TAU-DEPENDENT tolerance (see below).
//!
//! Plus self-consistency (1e-9): engine per-predictor detail sums to the engine's OWN aggregate
//! (RIF-algebra regression guard; NON-diagnostic of statistical correctness by itself — both sides
//! consume the same inputs — hence layers 1-3 carry the correctness burden).
//!
//! TAU-DEPENDENT tolerance (council MJ-3, measured-then-pinned). The engine's inline density
//! (`rif.rs`: bw.nrd0 bandwidth, nearest-rank IQR, exact single-point Gaussian kernel) and
//! ddecompose's (bw.nrd0, 512-pt FFT grid + interpolation) agree to ~1e-5 where the density is
//! well estimated (median + lower tail) but diverge at the UPPER tail, where the RIF's 1/f(q_tau)
//! term amplifies the density disagreement. A single flat tail-sized tolerance would blind the
//! median/lower-tail comparison (per-predictor magnitudes there are ~1e-3..5e-3, i.e. SMALLER than
//! a 2e-2 flat bound — no discriminating power). So the tolerance is tight at the well-conditioned
//! quantiles and loosened only at the tail, where the divergence is an intrinsic property of two
//! different density estimators, not engine error. Measured max |engine-ddecompose| (point est):
//!   tau=0.10 -> 4.0e-5   tau=0.50 -> 1.5e-5   tau=0.90 -> 1.37e-2

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const FIXTURE: &str = "tests/fixtures/employers_trust_fixture.csv";
const GOLDEN: &str = "tests/fixtures/trust_goldens_r.json";
const SELF_TOL: f64 = 1e-9;

/// Per-predictor engine-vs-ddecompose tolerance. Tight where the two density estimators agree
/// (~1e-5 at median/lower tail — pinned 5e-4, ~10x+ headroom for regeneration variation); loosened
/// at the upper tail where 1/f(q_tau) amplifies an intrinsic density-estimator disagreement.
fn per_predictor_tol(tau: f64) -> f64 {
    if tau >= 0.85 {
        2.0e-2 // upper-tail density-divergence bound (measured 1.37e-2); coarse by necessity
    } else {
        5.0e-4 // well-conditioned quantiles; real discriminating power
    }
}

/// Aggregate composition/structure engine-vs-ddecompose tolerance (measured-then-pinned). The
/// aggregate is the headline "which effect drives the gap" number and is the best-conditioned
/// oracle check at the well-estimated quantiles: measured engine-vs-ddecompose aggregate diff is
/// ~1e-6 at tau=0.10/0.50 (near-exact — pinned 1e-4, ~100x headroom, strong power) but grows to
/// ~1.1e-3 at tau=0.90, where 1/f(q_tau) scales the whole RIF vector by the density disagreement
/// (pinned 5e-3, ~4x headroom). The tight well-conditioned bound is what actually validates the
/// engine's RIF-OLS arithmetic; the tail bound is disclosed density-estimator divergence, not error.
fn aggregate_tol(tau: f64) -> f64 {
    if tau >= 0.85 {
        5.0e-3
    } else {
        1.0e-4
    }
}

fn load_fixture() -> DataFrame {
    LazyCsvReader::new(FIXTURE)
        .with_has_header(true)
        .finish()
        .unwrap()
        .collect()
        .unwrap()
}
fn load_golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(GOLDEN).unwrap()).unwrap()
}
fn est(comps: &[oaxaca_blinder::ComponentResult], name: &str) -> Option<f64> {
    comps.iter().find(|c| c.name == name).map(|c| c.estimate)
}
fn engine_key_set(comps: &[oaxaca_blinder::ComponentResult]) -> BTreeSet<String> {
    comps.iter().map(|c| c.name.clone()).collect()
}
fn golden_key_set(obj: &Map<String, Value>) -> BTreeSet<String> {
    obj.keys().cloned().collect()
}

#[test]
fn ac6_quantile_detail_golden_ddecompose() {
    let golden = load_golden();
    let qd = &golden["quantile_detail"];
    assert_eq!(
        qd["available"].as_bool(),
        Some(true),
        "ddecompose golden must be present (regenerate on an R+ddecompose machine)"
    );
    let per_tau = qd["per_tau"].as_object().expect("per_tau object");

    let taus = [
        (0.10, "quantile_0.1"),
        (0.50, "quantile_0.5"),
        (0.90, "quantile_0.9"),
    ];
    let mut global_max_diff = 0.0f64;

    for (tau, key) in taus {
        let g = &per_tau[key];
        let df = load_fixture();
        let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
        b.predictors(vec!["Age", "Experience_Years"])
            .categorical_predictors(vec!["Education_Level", "Department", "Location"])
            // Explicit (not relying on the constructor default): matches R's `bstar <- bB`.
            // Guards the golden against a future change to the builder's default scheme.
            .reference_coefficients(ReferenceCoefficients::GroupB)
            .bootstrap_reps(1);
        let r = b.decompose_quantile(tau).expect("decompose_quantile");

        let agg_ex = est(&r.two_fold.aggregate, "explained").unwrap();
        let agg_un = est(&r.two_fold.aggregate, "unexplained").unwrap();

        // ---- self-consistency: Σ detail == this call's aggregate (RIF algebra, tight) ----
        let sum_ex: f64 = r
            .two_fold
            .detailed_explained
            .iter()
            .map(|c| c.estimate)
            .sum();
        let sum_un: f64 = r
            .two_fold
            .detailed_unexplained
            .iter()
            .map(|c| c.estimate)
            .sum();
        assert!(
            (sum_ex - agg_ex).abs() < SELF_TOL,
            "[{key}] Σ detailed_explained {sum_ex} != aggregate explained {agg_ex}"
        );
        assert!(
            (sum_un - agg_un).abs() < SELF_TOL,
            "[{key}] Σ detailed_structure {sum_un} != aggregate unexplained {agg_un}"
        );

        let gcomp = g["detailed_composition"].as_object().unwrap();
        let gstruct = g["detailed_structure"].as_object().unwrap();

        // ---- LAYER 1: KEY-SET guard (defeats vacuous pass) ----
        assert_eq!(
            engine_key_set(&r.two_fold.detailed_explained),
            golden_key_set(gcomp),
            "[{key}] engine composition key set != ddecompose golden key set (naming/coverage regression)"
        );
        assert_eq!(
            engine_key_set(&r.two_fold.detailed_unexplained),
            golden_key_set(gstruct),
            "[{key}] engine structure key set != ddecompose golden key set"
        );

        // ---- LAYER 2: AGGREGATE vs ddecompose (well-conditioned primary gate) ----
        let g_agg_comp = g["aggregate_composition"].as_f64().unwrap();
        let g_agg_struct = g["aggregate_structure"].as_f64().unwrap();
        let atol = aggregate_tol(tau);
        assert!(
            (agg_ex - g_agg_comp).abs() <= atol,
            "[{key}] aggregate composition: engine {agg_ex:.6} vs ddecompose {g_agg_comp:.6} \
             diff {:.3e} > tol {atol:.1e}",
            (agg_ex - g_agg_comp).abs()
        );
        assert!(
            (agg_un - g_agg_struct).abs() <= atol,
            "[{key}] aggregate structure: engine {agg_un:.6} vs ddecompose {g_agg_struct:.6} \
             diff {:.3e} > tol {atol:.1e}",
            (agg_un - g_agg_struct).abs()
        );

        // ---- LAYER 3: per-predictor vs ddecompose (tau-dependent; EVERY key compared) ----
        let ptol = per_predictor_tol(tau);
        let mut tau_max_diff = 0.0f64;
        let mut compared = 0usize;
        for (name, gv) in gcomp {
            // Key-set guard above guarantees presence; panic (not skip) if the invariant is broken.
            let ev = est(&r.two_fold.detailed_explained, name).unwrap_or_else(|| {
                panic!("[{key}] composition '{name}' missing post key-set guard")
            });
            let d = (ev - gv.as_f64().unwrap()).abs();
            tau_max_diff = tau_max_diff.max(d);
            compared += 1;
            assert!(
                d <= ptol,
                "[{key}] composition '{name}': engine {ev:.6} vs ddecompose {:.6} diff {d:.3e} > tol {ptol:.1e}",
                gv.as_f64().unwrap()
            );
        }
        for (name, gv) in gstruct {
            let ev = est(&r.two_fold.detailed_unexplained, name)
                .unwrap_or_else(|| panic!("[{key}] structure '{name}' missing post key-set guard"));
            let d = (ev - gv.as_f64().unwrap()).abs();
            tau_max_diff = tau_max_diff.max(d);
            compared += 1;
            assert!(
                d <= ptol,
                "[{key}] structure '{name}': engine {ev:.6} vs ddecompose {:.6} diff {d:.3e} > tol {ptol:.1e}",
                gv.as_f64().unwrap()
            );
        }
        // Vacuous-pass backstop: every golden key on both sides must have been compared.
        assert_eq!(
            compared,
            gcomp.len() + gstruct.len(),
            "[{key}] compared {compared} != expected {} — vacuous-pass guard",
            gcomp.len() + gstruct.len()
        );
        assert!(compared > 0, "[{key}] zero comparisons made");

        global_max_diff = global_max_diff.max(tau_max_diff);
        eprintln!(
            "[{key}] compared {compared} covariates; per-pred max |engine-ddecompose| = {tau_max_diff:.3e} \
             (tol {ptol:.1e}); aggregate engine comp/struct = {agg_ex:.6}/{agg_un:.6} vs ddecompose {g_agg_comp:.6}/{g_agg_struct:.6}"
        );
    }
    eprintln!(
        "AC-6 global per-predictor max |engine-ddecompose| across all tau = {global_max_diff:.3e}"
    );
}
