#!/usr/bin/env python3
"""Generate the golden-value parity fixture + reference JSON for the Rust engine's
two-fold Oaxaca-Blinder decomposition.

This is REGENERATION TOOLING ONLY (INV-02). It is NOT run at test time — the Rust
parity test (`oaxaca_blinder/tests/parity_test.rs`) reads the committed
`parity_golden.json` offline (no Python, no network, no statsmodels).

Run:
    python verification/gen_parity_golden.py
Requires:
    statsmodels>=0.14, numpy, pandas
Writes (both committed):
    oaxaca_blinder/tests/fixtures/parity_fixture.csv   (seed-42 deterministic dataset)
    oaxaca_blinder/tests/fixtures/parity_golden.json   (statsmodels reference values)

Parameterization match (determined empirically 2026-06-06, R6 of the Track-0 spec):
  The engine splits groups so that A = non-reference group and B = reference group
  (`builder.rs::split_groups` sets group_b = reference_group). With reference_group="F",
  the engine's Group A = "M" (advantaged), Group B = "F" (reference). The engine's
  total_gap = mean(A) - mean(B) = mean(M) - mean(F) > 0.

  statsmodels OaxacaBlinder must therefore run with swap=True so that its "f" group = M
  and "s" group = F, aligning with the engine's A/B. Then:
    engine ReferenceCoefficients::GroupB  (beta_star = beta_B = beta_F)
        == statsmodels two_fold(two_fold_type='self_submitted', submitted_weight=0.0)
    engine ReferenceCoefficients::Pooled  (Neumark, pooled WITH group dummy)
        == statsmodels two_fold(two_fold_type='pooled')
  Both verified to <1e-9 against the engine on this fixture.

  Per-variable contributions: statsmodels' two_fold() exposes only aggregate
  explained/unexplained. The per-variable golden values are computed here directly
  from statsmodels' OWN independently-fitted group OLS models (ob._f_model.params,
  ob._s_model.params) and group means, using the standard Oaxaca per-variable formula.
  This is an oracle independent of the Rust engine (statsmodels fits the OLS; NumPy does
  the arithmetic).

  Intercept naming: statsmodels uses "const"; the engine uses "__ob_intercept__"
  (builder.rs:325). The golden JSON always uses the engine's "__ob_intercept__".
"""
import json
import sys
from datetime import date, timezone, datetime
from pathlib import Path

import numpy as np
import pandas as pd
import statsmodels
from statsmodels.stats.oaxaca import OaxacaBlinder

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURE_PATH = REPO_ROOT / "oaxaca_blinder/tests/fixtures/parity_fixture.csv"
GOLDEN_PATH = REPO_ROOT / "oaxaca_blinder/tests/fixtures/parity_golden.json"

OUTCOME = "log_wage"
GROUP_COL = "gender"
REFERENCE_GROUP = "F"
PREDICTORS = ["education", "experience", "tenure"]
INTERNAL_TOL = 1e-9


def make_fixture() -> pd.DataFrame:
    """Deterministic seed-42 dataset. ~0.91 total log-wage gap with both an explained
    (education/experience endowment) and an unexplained (intercept + coefficient) channel.
    Tenure has equal means across groups -> ~zero explained contribution (verifies a zero
    channel is handled)."""
    rng = np.random.default_rng(42)

    # Group F (reference, n=150)
    edu_f = rng.normal(13.5, 2.0, 150).clip(8, 22)
    exp_f = rng.normal(10.0, 5.0, 150).clip(0, 40)
    ten_f = rng.normal(4.5, 3.0, 150).clip(0, 20)
    log_wage_f = 1.5 + 0.08 * edu_f + 0.02 * exp_f + 0.015 * ten_f + rng.normal(0, 0.15, 150)

    # Group M (advantaged, n=150) — higher intercept + higher returns to education
    edu_m = rng.normal(14.5, 2.0, 150).clip(8, 22)
    exp_m = rng.normal(12.0, 5.0, 150).clip(0, 40)
    ten_m = rng.normal(4.5, 3.0, 150).clip(0, 20)
    log_wage_m = 1.8 + 0.11 * edu_m + 0.025 * exp_m + 0.015 * ten_m + rng.normal(0, 0.15, 150)

    df = pd.DataFrame({
        "gender": ["F"] * 150 + ["M"] * 150,
        "log_wage": np.concatenate([log_wage_f, log_wage_m]),
        "education": np.concatenate([edu_f, edu_m]),
        "experience": np.concatenate([exp_f, exp_m]),
        "tenure": np.concatenate([ten_f, ten_m]),
    })
    FIXTURE_PATH.parent.mkdir(parents=True, exist_ok=True)
    # %.17g preserves full float64 precision so the Rust engine reads byte-identical inputs
    df.to_csv(FIXTURE_PATH, index=False, float_format="%.17g")
    return df


def build_oaxaca(df: pd.DataFrame) -> OaxacaBlinder:
    """statsmodels OaxacaBlinder with swap=True (f=M, s=F) to align with the engine's
    A=non-ref / B=ref convention. exog column order: [is_M, education, experience, tenure];
    bifurcate=0 splits on is_M; hasconst=False -> statsmodels appends the constant last."""
    work = df.copy()
    work["is_M"] = (work[GROUP_COL] == "M").astype(float)
    endog = work[OUTCOME].to_numpy()
    exog = work[["is_M"] + PREDICTORS].to_numpy()
    return OaxacaBlinder(endog, exog, bifurcate=0, hasconst=False, swap=True)


def per_variable(ob: OaxacaBlinder, beta_star: np.ndarray) -> tuple[dict, dict]:
    """Standard Oaxaca per-variable contributions from statsmodels' own group OLS fits.
    Order of params/means: [education, experience, tenure, const]. 'const' -> '__ob_intercept__'."""
    beta_f = np.asarray(ob._f_model.params, dtype=float)  # group f = M = engine A
    beta_s = np.asarray(ob._s_model.params, dtype=float)  # group s = F = engine B
    xf = np.asarray(ob.exog_f_mean, dtype=float)
    xs = np.asarray(ob.exog_s_mean, dtype=float)
    names = PREDICTORS + ["__ob_intercept__"]  # const is appended last by statsmodels

    explained, unexplained = {}, {}
    for i, name in enumerate(names):
        explained[name] = float((xf[i] - xs[i]) * beta_star[i])
        unexplained[name] = float(xf[i] * (beta_f[i] - beta_star[i]) + xs[i] * (beta_star[i] - beta_s[i]))
    return explained, unexplained


def assert_consistent(label: str, explained: float, unexplained: float, gap: float,
                      det_exp: dict, det_unexp: dict) -> None:
    if abs((explained + unexplained) - gap) > INTERNAL_TOL:
        sys.exit(f"[{label}] explained+unexplained ({explained + unexplained}) != gap ({gap})")
    if abs(sum(det_exp.values()) - explained) > INTERNAL_TOL:
        sys.exit(f"[{label}] sum(detailed_explained) != explained")
    if abs(sum(det_unexp.values()) - unexplained) > INTERNAL_TOL:
        sys.exit(f"[{label}] sum(detailed_unexplained) != unexplained")


def main() -> None:
    df = make_fixture()
    df_back = pd.read_csv(FIXTURE_PATH)  # round-trip: oracle sees exactly the committed bytes
    ob = build_oaxaca(df_back)

    gap = float(ob.gap)
    n_a = int(ob.len_f)  # f = M = engine Group A
    n_b = int(ob.len_s)  # s = F = engine Group B

    # --- Primary scheme: GroupB == self_submitted(weight=0.0) ---
    ob.two_fold(two_fold_type="self_submitted", submitted_weight=0.0)
    gb_explained, gb_unexplained = float(ob.explained), float(ob.unexplained)
    beta_star_b = np.asarray(ob._s_model.params, dtype=float)  # beta_star = beta_s = beta_F
    gb_det_exp, gb_det_unexp = per_variable(ob, beta_star_b)
    assert_consistent("GroupB", gb_explained, gb_unexplained, gap, gb_det_exp, gb_det_unexp)

    # --- Cross-check scheme: Pooled == 'pooled' (aggregate only) ---
    ob.two_fold(two_fold_type="pooled")
    pooled_explained, pooled_unexplained = float(ob.explained), float(ob.unexplained)
    if abs((pooled_explained + pooled_unexplained) - gap) > INTERNAL_TOL:
        sys.exit("[Pooled] explained+unexplained != gap")

    golden = {
        "_meta": {
            "generated": date.today().isoformat(),
            "generated_utc": datetime.now(timezone.utc).isoformat(),
            "statsmodels_version": statsmodels.__version__,
            "numpy_version": np.__version__,
            "pandas_version": pd.__version__,
            "python_version": sys.version.split()[0],
            "fixture": "parity_fixture.csv",
            "n_rows": int(len(df_back)),
            "group_col": GROUP_COL,
            "reference_group": REFERENCE_GROUP,
            "outcome": OUTCOME,
            "predictors": PREDICTORS,
            "reference_coefficients": "GroupB",
            "statsmodels_two_fold_type": "self_submitted",
            "statsmodels_submitted_weight": 0.0,
            "statsmodels_swap": True,
            "statsmodels_group_order": "swap=True -> f=M (engine Group A), s=F (engine Group B reference)",
            "tolerance_point_estimates": 1e-6,
            "notes": (
                "Engine A=non-ref(M), B=ref(F). total_gap=mean(M)-mean(F). "
                "GroupB scheme (beta_star=beta_F) == statsmodels self_submitted(weight=0.0,swap=True). "
                "Pooled scheme == statsmodels two_fold_type='pooled'. "
                "Per-variable values computed from statsmodels group OLS fits (independent of the Rust engine). "
                "statsmodels 'const' mapped to engine '__ob_intercept__'. "
                "Bootstrap-derived fields (std_err,t_stat,p_value,ci_*) are NOT frozen (unseeded RNG)."
            ),
        },
        "total_gap": gap,
        "n_a": n_a,
        "n_b": n_b,
        "two_fold": {
            "explained": gb_explained,
            "unexplained": gb_unexplained,
        },
        "detailed_explained": gb_det_exp,
        "detailed_unexplained": gb_det_unexp,
        "cross_check_pooled": {
            "engine_reference_coefficients": "Pooled",
            "statsmodels_two_fold_type": "pooled",
            "explained": pooled_explained,
            "unexplained": pooled_unexplained,
        },
    }

    with open(GOLDEN_PATH, "w") as f:
        json.dump(golden, f, indent=2)
        f.write("\n")

    print(f"Wrote {FIXTURE_PATH}")
    print(f"Wrote {GOLDEN_PATH}")
    print(f"  total_gap          = {gap:.10f}")
    print(f"  GroupB explained   = {gb_explained:.10f}")
    print(f"  GroupB unexplained = {gb_unexplained:.10f}")
    print(f"  Pooled explained   = {pooled_explained:.10f}")
    print(f"  Pooled unexplained = {pooled_unexplained:.10f}")
    print(f"  internal consistency OK (<{INTERNAL_TOL})")


if __name__ == "__main__":
    main()
