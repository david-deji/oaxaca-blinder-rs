//! Tau-varying quantile-regression validation (0014-MERIDIAN, AC-5).
//!
//! The existing QR unit tests use perfectly-linear data where every quantile has the SAME
//! slope — tau-insensitive, so a broken tau would pass. This suite uses a heteroskedastic
//! location-scale design  Y = b0 + b1*X + (1 + c*X)*U,  U~N(0,1),  X>0,  whose true slope at
//! quantile tau is analytically tau-VARYING:  beta1(tau) = b1 + c*z_tau,  z_tau = qnorm(tau).
//!
//! OFFLINE: the committed synthetic sample (X, Y) and both goldens (`quantreg::rq()` slopes +
//! analytic slopes) live in `trust_goldens_r.json.qr_location_scale`. The engine's `solve_qr`
//! (via the `qr_coefficients` wrapper) fits Y ~ [1, X] and must match rq() to rel 1e-4 and the
//! analytic slope to finite-sample tolerance — AND pass the discrimination assertion.

use oaxaca_blinder::qr_coefficients;
use serde_json::Value;
use std::collections::BTreeMap;

const GOLDEN: &str = "tests/fixtures/trust_goldens_r.json";
const RQ_REL_TOL: f64 = 1e-4; // LP-to-LP (engine clarabel vs quantreg), matches existing QR tol
const ANALYTIC_ABS_TOL: f64 = 0.06; // finite-sample sampling error at n>=5000 (from QR asymptotic SE)

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(GOLDEN).unwrap()).unwrap()
}
fn as_f64_vec(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

#[test]
fn ac5_qr_location_scale_tau_varying() {
    let g = golden();
    let qr = &g["qr_location_scale"];
    assert_eq!(
        qr["available"].as_bool(),
        Some(true),
        "QR golden must be present (regenerate on an R+quantreg machine)"
    );

    let xs = as_f64_vec(&qr["X"]);
    let ys = as_f64_vec(&qr["Y"]);
    assert_eq!(xs.len(), ys.len());
    // design rows [1, x] so beta = [intercept, slope]; the solver adds no intercept.
    let design: Vec<Vec<f64>> = xs.iter().map(|&x| vec![1.0, x]).collect();
    let c = qr["c"].as_f64().unwrap();

    let rq_slope = qr["rq_slope"].as_object().unwrap();
    let true_slope = qr["true_slope"].as_object().unwrap();

    let mut engine_slope: BTreeMap<String, f64> = BTreeMap::new();
    for (key, rq_val) in rq_slope {
        // key like "tau_0.1" -> tau 0.1
        let tau: f64 = key
            .trim_start_matches("tau_")
            .parse()
            .expect("tau key parses");
        let beta = qr_coefficients(&design, &ys, tau).expect("engine QR solve");
        let slope = beta[1];
        engine_slope.insert(key.clone(), slope);

        // engine vs quantreg::rq() — LP to LP, rel 1e-4
        let rqv = rq_val.as_f64().unwrap();
        let rel = RQ_REL_TOL * rqv.abs().max(1.0);
        assert!(
            (slope - rqv).abs() <= rel,
            "[{key}] engine slope {slope:.8} vs rq() {rqv:.8} diff {:.2e} > {rel:.1e}",
            (slope - rqv).abs()
        );

        // engine vs analytic beta1(tau)=b1+c*z_tau — finite-sample abs tol
        let truev = true_slope[key].as_f64().unwrap();
        assert!((slope - truev).abs() <= ANALYTIC_ABS_TOL,
            "[{key}] engine slope {slope:.6} vs analytic {truev:.6} diff {:.4} > {ANALYTIC_ABS_TOL}",
            (slope - truev).abs());
    }

    // ---- discrimination: a tau-insensitive implementation cannot pass this ----
    // beta1(0.9)-beta1(0.1) >= c*(z_0.9 - z_0.1)*0.8 ; z_0.9-z_0.1 = 2*qnorm(0.9) = 2.5631.
    let s90 = engine_slope.get("tau_0.9").expect("tau_0.9 present");
    let s10 = engine_slope.get("tau_0.1").expect("tau_0.1 present");
    let z_spread = 2.0 * 1.2815515594465483; // qnorm(0.9) - qnorm(0.1)
    let threshold = c * z_spread * 0.8;
    assert!(
        s90 - s10 >= threshold,
        "discrimination failed: slope(0.9)-slope(0.1)={:.4} < {threshold:.4} (tau-insensitive?)",
        s90 - s10
    );
}
