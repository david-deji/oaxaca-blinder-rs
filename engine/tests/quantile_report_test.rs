//! 0120-MERIDIAN S8 / T16 / V8: percentile mode reports the ACTUAL percentile gap beside the
//! RIF model total, and says when ties or a coarse step grid make the percentile unreliable.
//!
//! Oracle: R `quantile(type = 7)` per group, the empirical CDF at it and the exact count of rows
//! tied at it (`verification/gen_diag_goldens.R`), on the employers fixture (39 distinct
//! salaries in 10 000 rows, where the model total at p10 and p90 is not the percentile gap) and
//! on a 4-step pay grid against a continuous group.

#[path = "../../oaxaca_blinder/tests/support/diag_golden.rs"]
mod diag_golden;

use diag_golden::*;
use pay_equity_engine::analysis::decompose_inner;
use pay_equity_engine::types::*;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

fn golden() -> &'static DiagGolden {
    static G: OnceLock<DiagGolden> = OnceLock::new();
    G.get_or_init(|| DiagGolden::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")))
}

fn request(case: &str, tau: Option<f64>) -> DecompositionRequest {
    let c = golden().block(&["quantiles", case]);
    let (fixture, predictors) = match case {
        "employers" => (
            "employers_trust_fixture.csv",
            vec!["Age", "Experience_Years"],
        ),
        "grid" => ("diag_grid.csv", vec!["x"]),
        _ => panic!("{case}"),
    };
    DecompositionRequest {
        csv_data: golden().csv(fixture),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["reference_group"].as_str().unwrap().to_string(),
        predictors: predictors.into_iter().map(String::from).collect(),
        categorical_predictors: None,
        three_fold: None,
        quantile: tau,
        reference_coefficients: Some("GroupB".to_string()),
        bootstrap_reps: Some(0),
    }
}

fn want(case: &str, tau: &str) -> &'static Value {
    golden().block(&["quantiles", case, "taus", tau])
}

/// Warnings the thresholds should raise, derived from R's numbers (5% ties; ECDF offset above
/// 0.01 and above 1/n, the most a tie-free group of n rows can be off by discreteness).
fn expected_group_warnings(w: &Value, tau: f64) -> Vec<(WarningCode, String)> {
    let mut out = Vec::new();
    for (label, g) in [("reference", &w["reference"]), ("target", &w["target"])] {
        if f(g, "tie_share") > 0.05 {
            out.push((WarningCode::TieShare, label.to_string()));
        }
        let line = 0.01_f64.max(1.0 / g["count"].as_f64().unwrap());
        if (f(g, "ecdf_at_quantile") - tau).abs() > line {
            out.push((WarningCode::EcdfOffset, label.to_string()));
        }
    }
    out.sort_by_key(|(c, s)| (format!("{c:?}"), s.clone()));
    out
}

fn report_warnings(res: &DecompositionResult) -> Vec<(WarningCode, String)> {
    let mut v: Vec<_> = res
        .warnings
        .iter()
        .filter(|w| matches!(w.code, WarningCode::TieShare | WarningCode::EcdfOffset))
        .map(|w| (w.code, w.subject.clone().unwrap()))
        .collect();
    v.sort_by_key(|(c, s)| (format!("{c:?}"), s.clone()));
    v
}

fn check_group(label: &str, got: &QuantileGroupReport, w: &Value, tau: f64) {
    assert_eq!(
        got.count as i64,
        w["count"].as_i64().unwrap(),
        "{label} count"
    );
    assert_close(
        &format!("{label} quantile"),
        got.quantile_value,
        f(w, "quantile_value"),
        1e-12,
    );
    assert_close(
        &format!("{label} F_n(q)"),
        got.ecdf_at_quantile,
        f(w, "ecdf_at_quantile"),
        1e-12,
    );
    assert_close(
        &format!("{label} F_n(q) - tau"),
        got.ecdf_offset,
        f(w, "ecdf_at_quantile") - tau,
        1e-12,
    );
    // The tie share is an exact count over the group's rows.
    assert_eq!(
        (got.tie_share * got.count as f64).round() as i64,
        w["tie_count"].as_i64().unwrap(),
        "{label}: rows tied at the percentile"
    );
    assert_close(
        &format!("{label} tie share"),
        got.tie_share,
        f(w, "tie_share"),
        1e-12,
    );
}

#[test]
fn v8_percentile_gap_ecdf_and_ties_equal_r_on_the_employers_fixture() {
    for (key, tau) in [("0.1", 0.1), ("0.5", 0.5), ("0.9", 0.9)] {
        let res = decompose_inner(request("employers", Some(tau))).unwrap();
        let report = res
            .quantile_report
            .as_ref()
            .expect("percentile run carries a report");
        let w = want("employers", key);
        assert_eq!(report.tau, tau);
        assert_close(
            &format!("tau {tau} quantile_gap"),
            report.quantile_gap,
            f(w, "quantile_gap"),
            1e-12,
        );
        check_group(
            &format!("tau {tau} reference"),
            &report.reference,
            &w["reference"],
            tau,
        );
        check_group(
            &format!("tau {tau} target"),
            &report.target,
            &w["target"],
            tau,
        );
        // The RIF total is the number the model decomposes and total_gap carries: unchanged.
        assert_eq!(report.rif_total, res.total_gap);
        assert_eq!(
            report_warnings(&res),
            expected_group_warnings(w, tau),
            "tau {tau}: tie / ECDF warnings"
        );
    }
}

#[test]
fn v8_the_headline_and_the_model_total_part_company_at_the_tails() {
    // The re-ground's finding: with 39 distinct salaries the p10 and p90 percentile gaps are
    // exactly 0, while the mean-RIF difference the card used to show is not.
    let p10 = decompose_inner(request("employers", Some(0.1))).unwrap();
    let r10 = p10.quantile_report.unwrap();
    assert_eq!(
        r10.quantile_gap, 0.0,
        "R says the p10 percentile gap is zero"
    );
    assert!(
        (r10.rif_total + 0.0121).abs() < 5e-4,
        "model total at p10 should be about -0.0121 (target minus reference), got {}",
        r10.rif_total
    );
    let p90 = decompose_inner(request("employers", Some(0.9))).unwrap();
    let r90 = p90.quantile_report.unwrap();
    assert_eq!(r90.quantile_gap, 0.0);
    assert!(
        r90.rif_total.abs() < 5e-4 && r90.rif_total != 0.0,
        "{}",
        r90.rif_total
    );
    // At the median the model total is half the true gap.
    let p50 = decompose_inner(request("employers", Some(0.5))).unwrap();
    let r50 = p50.quantile_report.unwrap();
    assert!(
        (r50.quantile_gap / r50.rif_total - 2.0).abs() < 0.1,
        "{r50:?}"
    );
    // total_gap and the percentile gap share an orientation: target minus reference.
    assert!(r50.quantile_gap < 0.0 && r50.rif_total < 0.0);
}

#[test]
fn v8_a_step_grid_fires_the_tie_warning_and_a_continuous_group_does_not() {
    // 30 vs 30 rows. At the median the grid group's 26-step holds 40% of its rows.
    let res = decompose_inner(request("grid", Some(0.5))).unwrap();
    let report = res.quantile_report.as_ref().unwrap();
    let w = want("grid", "0.5");
    check_group("grid p50 target", &report.target, &w["target"], 0.5);
    check_group(
        "grid p50 reference",
        &report.reference,
        &w["reference"],
        0.5,
    );
    assert_eq!(report.target.tie_share, 0.4);
    assert_eq!(report.reference.tie_share, 0.0);
    let ties: Vec<_> = res
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::TieShare)
        .collect();
    assert_eq!(ties.len(), 1, "{:?}", res.warnings);
    assert_eq!(ties[0].subject.as_deref(), Some("target"));
    assert_close("tie warning value", ties[0].value, 0.4, 1e-12);
    assert_eq!(ties[0].threshold, 0.05);
    assert!(res
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::EcdfOffset && w.subject.as_deref() == Some("target")));
    // Nothing fires for the continuous group at any percentile.
    for tau in [0.5, 0.9] {
        let r = decompose_inner(request("grid", Some(tau))).unwrap();
        assert!(
            r.warnings
                .iter()
                .all(|w| w.subject.as_deref() != Some("reference")
                    || !matches!(w.code, WarningCode::TieShare | WarningCode::EcdfOffset)),
            "{:?}",
            r.warnings
        );
    }
    // At p90 the grid group's percentile falls between two steps: no tie, no offset.
    let p90 = decompose_inner(request("grid", Some(0.9))).unwrap();
    assert!(report_warnings(&p90).is_empty(), "{:?}", p90.warnings);
    assert_eq!(
        report_warnings(&p90),
        expected_group_warnings(want("grid", "0.9"), 0.9)
    );
}

#[test]
fn v8_a_mean_run_has_no_percentile_report() {
    let res = decompose_inner(request("employers", None)).unwrap();
    assert!(res.quantile_report.is_none());
    let json = serde_json::to_value(&res).unwrap();
    assert!(json.get("quantile_report").is_none());
    assert!(res
        .warnings
        .iter()
        .all(|w| !matches!(w.code, WarningCode::TieShare | WarningCode::EcdfOffset)));
    // ...and a percentile run does, with the fields the card reads.
    let q =
        serde_json::to_value(decompose_inner(request("employers", Some(0.5))).unwrap()).unwrap();
    for k in ["tau", "quantile_gap", "rif_total", "reference", "target"] {
        assert!(!q["quantile_report"][k].is_null(), "{k}");
    }
    for k in [
        "count",
        "quantile_value",
        "ecdf_at_quantile",
        "ecdf_offset",
        "tie_share",
    ] {
        assert!(!q["quantile_report"]["target"][k].is_null(), "{k}");
    }
}

#[test]
fn v8_the_rif_total_is_the_pre_change_number() {
    // The null-free golden pins the noisy-fixture percentile runs at 1e-9 against the pre-0118
    // engine; here the same property on the employers fixture: the total is the RIF mean
    // difference, untouched by the report, and equals the library's own total_gap.
    let res = decompose_inner(request("employers", Some(0.5))).unwrap();
    let rep = res.quantile_report.unwrap();
    assert_eq!(rep.rif_total, res.total_gap);
    let sum = res.explained_gap + res.unexplained_gap;
    assert!(
        (sum - res.total_gap).abs() < 1e-9,
        "{sum} vs {}",
        res.total_gap
    );
}

// ---- E-REV-2: discreteness is not a step grid -------------------------------------------------

/// A group of `n` strictly increasing (tie-free) salaries.
fn distinct(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 50_000.0 + 137.0 * i as f64 + (i * i) as f64)
        .collect()
}

#[test]
fn e_rev_2_small_tie_free_groups_do_not_raise_the_ecdf_warning() {
    use pay_equity_engine::support::quantile_report;
    // F_n(q_tau) - tau is up to 1/n by discreteness alone, with no tied value anywhere. The old
    // fixed 0.01 line fired for every one of these groups at some percentile.
    let mut old_line_would_have_fired = 0;
    for n in [8usize, 11, 15, 23, 37, 49, 60, 100] {
        for tau in [0.1, 0.5, 0.9] {
            let (rep, warnings) = quantile_report(tau, &distinct(60), &distinct(n), 0.0);
            assert!(
                rep.target.tie_share <= 1.0 / n as f64,
                "n={n} tau={tau}: at most the one row that q lands on"
            );
            if rep.target.ecdf_offset.abs() > 0.01 {
                old_line_would_have_fired += 1;
            }
            assert!(
                warnings
                    .iter()
                    .all(|w| w.code != WarningCode::EcdfOffset
                        || w.subject.as_deref() != Some("target")),
                "n={n} tau={tau}: offset {} raised the warning {warnings:?}",
                rep.target.ecdf_offset
            );
            assert!(
                rep.target.ecdf_offset.abs() <= 1.0 / n as f64 + 1e-12,
                "n={n} tau={tau}: a tie-free offset is at most 1/n, got {}",
                rep.target.ecdf_offset
            );
        }
    }
    // the gate can fail: the fixed line did fire on these inputs (n = 8, 11, 15, 23, 37, 49 ...)
    assert!(
        old_line_would_have_fired >= 10,
        "only {old_line_would_have_fired} of 24 cases exceed 0.01; the test would pass on the old line"
    );
}

#[test]
fn e_rev_2_the_23_woman_roster_is_silent_end_to_end_and_a_tied_one_still_fires() {
    // 23 women with 23 different salaries against 40 men, through the engine entry point.
    fn csv(women: &[f64]) -> Vec<u8> {
        let mut out = String::from("y,x,g\n");
        for (i, y) in distinct(40).iter().enumerate() {
            out.push_str(&format!("{y},{},Male\n", 10 + i % 9));
        }
        for (i, y) in women.iter().enumerate() {
            out.push_str(&format!("{y},{},Female\n", 10 + i % 9));
        }
        out.into_bytes()
    }
    let run = |women: &[f64], tau: f64| {
        decompose_inner(DecompositionRequest {
            csv_data: csv(women),
            outcome_variable: "y".to_string(),
            group_variable: "g".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["x".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: Some(tau),
            reference_coefficients: Some("GroupB".to_string()),
            bootstrap_reps: Some(0),
        })
        .unwrap()
    };
    for tau in [0.1, 0.5, 0.9] {
        let res = run(&distinct(23), tau);
        assert!(
            report_warnings(&res).is_empty(),
            "tau={tau}: tie-free roster raised {:?}",
            res.warnings
        );
    }
    // Nine of the 23 women share the median salary: a step, not discreteness. Both signals fire.
    let mut stepped = distinct(23);
    let median = stepped[11];
    for y in stepped.iter_mut().skip(8).take(9) {
        *y = median;
    }
    stepped.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let res = run(&stepped, 0.5);
    let fired = report_warnings(&res);
    assert!(
        fired.contains(&(WarningCode::TieShare, "target".to_string())),
        "{fired:?}"
    );
    assert!(
        fired.contains(&(WarningCode::EcdfOffset, "target".to_string())),
        "{fired:?}"
    );
    let w = res
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::EcdfOffset)
        .unwrap();
    assert!(
        w.threshold >= 1.0 / 23.0 - 1e-12,
        "the line that applied is reported: {w:?}"
    );
}
