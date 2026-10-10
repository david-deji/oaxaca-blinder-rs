# Oaxaca-Blinder Decomposition in Rust

> **Context Anchor:** This component is a pure statistical logic engine of the Macro System. For the overarching architecture and vision, see [SYSTEM_ARCHITECTURE](../SYSTEM_ARCHITECTURE.md) and [MISSION](../MISSION.md).

[![crates.io](https://img.shields.io/crates/v/oaxaca_blinder.svg)](https://crates.io/crates/oaxaca_blinder)
[![docs.rs](https://docs.rs/oaxaca_blinder/badge.svg)](https://docs.rs/oaxaca_blinder)
[![CI](https://img.shields.io/github/actions/workflow/status/YOUR_ORG/oaxaca-blinder-rs/ci.yml?branch=main&label=CI)](https://github.com/YOUR_ORG/oaxaca-blinder-rs/actions)
[![Release](https://img.shields.io/github/actions/workflow/status/YOUR_ORG/oaxaca-blinder-rs/release.yml?label=Release)](https://github.com/YOUR_ORG/oaxaca-blinder-rs/actions)

A high-performance Rust library for performing Oaxaca-Blinder decomposition, designed for economists, data scientists, and HR analysts. It decomposes the gap in an outcome variable (like wage) between two groups into "explained" (characteristics) and "unexplained" (discrimination/coefficients) components.

### Python Bindings

The library includes Python bindings exposed via `pyo3`.

**Note for Python 3.13+ users:**
If you are building from source on Python 3.13 or newer, you may need to enable ABI3 forward compatibility if you encounter build errors. We have enabled this via `.cargo/config.toml` for development, but for distribution builds, ensure:
```bash
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
```

To build and install the Python package:
```bash
maturin develop --features python
```
The Python API supports standard Oaxaca-Blinder decomposition, Quantile decomposition (RIF-Regression), and AKM estimation.
Beyond standard decomposition, it supports **Quantile Decomposition (RIF & Machado-Mata)**, **AKM (Abowd-Kramarz-Margolis) Models**, **Propensity Score Matching**, **DFL Reweighting**, and **Budget Optimization** for policy simulation.

<details>
<summary><strong>🚀 Feature Support</strong></summary>

| Feature | Support |
| :--- | :---: |
| **OLS Mean Decomposition** | ✅ |
| **Quantile Decomposition (Machado-Mata)** | ✅ |
| **Quantile Decomposition (RIF Regression)** | ✅ |
| **Categorical Normalization (Yun)** | ✅ |
| **Bootstrapped Standard Errors** | ✅ |
| **Budget simulation (deprecated library method; the product path is the engine's `optimize`)** | ✅ |
| **Wage Scale Design (Grade/Step)** | ✅ |
| **JMP Decomposition (Time Series)** | ✅ |
| **DFL Reweighting (Counterfactuals)** | ✅ |
| **Sample Weights** | ✅  |
| **Heckman Correction (Selection Bias)** | ✅ |
| **AKM (Worker-Firm Fixed Effects)** | ✅ |
| **Matching (Euclidean, Mahalanobis, PSM)** | ✅ |

</details>

---

<details>
<summary><strong>🏆 Why Use This Library?</strong></summary>

Most economists rely on the `oaxaca` R package or `statsmodels` in Python. While excellent, they have limitations that this library addresses:

1.  **🚀 Speed**: Written in Rust with parallelized bootstrapping (Rayon). It is **20-30x faster** than R and **10x faster** than Python for large datasets (see Benchmarks).
2.  **📦 All-in-One Toolkit**: In R, you need `oaxaca` for decomposition, `rifreg` for quantiles, `MatchIt` for matching, and `lfe` for AKM. In Python, `statsmodels` lacks built-in RIF, Matching, and AKM. This library unifies **all** of them into a single, consistent API.
3.  **🛡️ Type Safety**: Rust's strict type system prevents common data errors (like silent `NaN` propagation) that can plague dynamic languages.
4.  **🧠 Pay-gap remedy costing**: the `pay-equity-engine` crate costs a remedy for HR departments (who is raised, to which line, for how much, and what it does to the group's gap). The library's `OaxacaResults::optimize_budget` is deprecated in favour of it.
5.  **🐍 Python & CLI Support**: You don't need to know Rust. Use the high-performance engine directly from Python or the command line.
6.  **⚡ Parallelized Inference**: Bootstrapping standard errors for Oaxaca decompositions is computationally intensive. This library uses **Rayon** to parallelize this across all CPU cores, reducing wait times from minutes to seconds.

</details>

---

<details>
<summary><strong>🖥️ Command Line Interface (CLI)</strong></summary>

Don't want to write Rust code? You can use the `oaxaca-cli` tool directly from your terminal to analyze CSV files.

### Installation

```bash
cargo install oaxaca_blinder --features cli
```

### Usage

**Basic Decomposition:**
```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education experience --categorical sector
```

**Using R-style Formula:**
```bash
oaxaca-cli --data wage.csv --group gender --reference F \
    --formula "wage ~ education + experience + C(sector)"
```

**With Sample Weights (WLS):**
```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education experience \
    --weights sampling_weight --weights-kind relative
```
`--weights-kind` is required with `--weights`: `frequency` for whole-number counts (`2` is the row twice, standard errors and
p-values included: a bootstrap replicate draws `sum(w)` employees; a fractional value is refused, naming the row), `relative` for FTE or survey weights (rescaled to the row count).

**With Heckman Correction (Selection Bias):**
```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education experience \
    --selection-outcome employed \
    --selection-predictors education experience age marital_status
```

**Export Results:**
```bash
oaxaca-cli --data wage.csv ... --output-json results.json --output-markdown report.md
```

**Generate HTML Report:**
```bash
oaxaca-cli report \
  --data wage.csv \
  --outcome wage \
  --group gender \
  --reference F \
  --predictors education,experience \
  --categorical sector \
  --output report.html
```

Supports both `--analysis-type mean` (default) and `--analysis-type quantile`.

</details>

---

<details>
<summary><strong>⚡ Quick Start</strong></summary>

Add to `Cargo.toml`:

```toml
[dependencies]
oaxaca_blinder = "0.1.0"
polars = { version = "0.38", features = ["lazy", "csv"] }
```

### Basic OLS Decomposition

```rust
use polars::prelude::*;
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let df = df!(
        "wage" => &[25.0, 30.0, 35.0, 40.0, 45.0, 20.0, 22.0, 28.0, 32.0, 38.0],
        "education" => &[16.0, 18.0, 14.0, 20.0, 16.0, 12.0, 14.0, 16.0, 12.0, 18.0],
        "gender" => &["M", "M", "M", "M", "M", "F", "F", "F", "F", "F"]
    )?;

    let results = OaxacaBuilder::new(df, "wage", "gender", "F")
        .predictors(&["education"])
        .reference_coefficients(ReferenceCoefficients::Pooled)
        .run()?;

    results.summary();
    Ok(())
}
```

### Python Example



```python
import polars as pl
from oaxaca_blinder import OaxacaBlinder

# Load data with Polars
df = pl.read_csv("wage.csv")

# Initialize the Estimator
model = OaxacaBlinder(
    dataframe=df,
    outcome="wage",
    group="gender",
    reference_group="F",
    predictors=["education", "experience"],
    categorical_predictors=["sector"]
)

# Run Mean Decomposition
results = model.fit()

print(f"Total Gap: {results.total_gap:.4f}")
print(f"Explained: {results.two_fold.aggregate.explained.estimate:.4f}")
print(f"Unexplained: {results.two_fold.aggregate.unexplained.estimate:.4f}")

# Run Quantile Decomposition (RIF)
q_results = model.fit_quantile(0.9)
print(f"90th Percentile Gap: {q_results.total_gap:.4f}")

# Run the deprecated library budget method (library only; see the engine's `optimize` for the product remedy)
# "How do we close the gap to 0 with a $100,000 budget?"
adjustments = model.optimize_budget(budget=100000.0, target_gap=0.0)
```

</details>

---

<details>
<summary><strong>💰 Policy Simulation: Budget Optimization</strong></summary>

**Deprecated library method.** `OaxacaResults::optimize_budget` spends a budget on the disadvantaged group's largest negative residuals, largest first, until the raw total gap reaches a target. It is kept for existing callers and is deprecated: the remedy the Meridian product runs is the engine's `optimize` (crate `pay-equity-engine`), which raises each person below a fitted pay line up to it and reports the cost, who is still below the line, and the group's gap on one sign. Library-only example:

```rust
// Scenario: You have $200,000 to reduce the gap to 5% (deprecated; library only)
#[allow(deprecated)]
let adjustments = results.optimize_budget(200_000.0, 0.05);

for adj in adjustments {
    println!("Give ${:.2} raise to employee #{}", adj.adjustment, adj.index);
}
```

</details>

<details>
<summary><strong>📐 Policy Simulation: Wage Scale Design</strong></summary>

**"The Structural Fix"**

Sometimes, you can't just give individual raises. You need to redesign the entire pay scale (Grades & Steps) to be fair, subject to budget constraints.

This optimization engine finds the optimal `min_step_diff` (increment between steps) and `min_grade_diff` (increment between grades) to minimize the total cost while ensuring no current employee takes a pay cut.

```rust
use optimization_engine::WageScaleProblem;

let problem = WageScaleProblem::new(dataframe, budget, grades, steps, min_wage);

// Solves for optimal structural parameters
let solution = problem.solve()?;
```

</details>

---

<details>
<summary><strong>📊 Quantile Decomposition Strategies</strong></summary>

The library supports two robust methods for decomposing the wage gap across the distribution:

| Method | Best For... | Builder |
| :--- | :--- | :--- |
| **Machado-Mata (Simulation)** | Constructing full counterfactual distributions and "glass ceiling" analysis. | `QuantileDecompositionBuilder` |
| **RIF Regression (Analytical)** | Fast, detailed decomposition of specific quantiles (e.g., "Why is the 90th percentile gap so large?"). | `OaxacaBuilder::decompose_quantile(0.9)` |

### Example: RIF Decomposition

```rust
// Fast decomposition of the 90th percentile gap
let results = OaxacaBuilder::new(df, "wage", "gender", "F")
    .predictors(&["education", "experience"])
    .decompose_quantile(0.9)?;
```

### CLI Example

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education experience \
    --analysis-type quantile --quantiles 0.1,0.5,0.9
```

*Note: Python bindings for quantile decomposition are coming soon.*

</details>

---

<details>
<summary><strong>📈 Visualizing DFL Reweighting</strong></summary>

**DiNardo-Fortin-Lemieux (DFL)** reweighting (Rust Only) is a non-parametric alternative that allows you to visualize what the wage distribution of Group B would look like if they had the characteristics of Group A.

The `run_dfl` function returns density vectors perfect for plotting in Python (matplotlib) or Rust (plotters).

```rust
use oaxaca_blinder::run_dfl;

let dfl = run_dfl(&df, "wage", "gender", "F", &["education", "experience"])?;

// dfl.grid                   <- X-axis (Wage levels)
// dfl.density_a              <- Actual Group A Density
// dfl.density_b              <- Actual Group B Density
// dfl.density_b_counterfactual <- "What B would earn with A's characteristics"
```

*Tip: Plot `density_b` vs `density_b_counterfactual` to visualize the "explained" gap.*

</details>

---

<details>
<summary><strong>⏱️ Benchmarks</strong></summary>

Designed for performance, utilizing Rust's speed and parallelization (Rayon) for bootstrapping.

**Performance vs Python (`statsmodels`) vs R (`oaxaca`)**
*Dataset: 100k rows, 10 predictors*

| Reps | Rust (`oaxaca_blinder`) | Python (`statsmodels`) | R (`oaxaca`) |
| :--- | :--- | :--- | :--- |
| **1 (Raw)** | **0.14s** 🚀 | 0.15s | ? |
| **100** | **0.76s** 🚀 | N/A | ? |
| **500** | **3.11s** 🚀 | N/A | ~119.4s |

*Rust's raw decomposition is significantly faster than statsmodels, and the bootstrap performance is orders of magnitude faster than R.*

</details>


---
<details>
<summary><strong>Matching Engine</strong></summary>

The library includes a high-performance Matching Engine for causal inference, supporting Euclidean, Mahalanobis, and Propensity Score Matching (PSM).

### Rust Example

```rust
use oaxaca_blinder::MatchingEngine;
use polars::prelude::*;

// Load data...
let engine = MatchingEngine::new(df, "treatment", "outcome", &["age", "education"]);

// 1-Nearest Neighbor Matching with Mahalanobis distance
let weights = engine.run_matching(1, true)?;
```

### Python Example

```python
import oaxaca_blinder

# Match units
weights = oaxaca_blinder.match_units(
    "data.csv",
    treatment="treatment",
    outcome="wage",
    covariates=["education", "experience"],
    k=1,
    method="mahalanobis" # or "euclidean", "psm"
)
```

### CLI Example

```bash
oaxaca-cli --data wage.csv --outcome wage --group treatment --reference 0 \
  --predictors education,experience \
  --analysis-type match --matching-method mahalanobis --k-neighbors 1
```

</details>




---

## 📚 Theory & Methodology

<details>
<summary><strong>Deep Dive: The Indexing Problem & Reference Groups</strong></summary>

The decomposition depends on the choice of the non-discriminatory coefficient vector $\beta^*$. The general decomposition equation is:

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?\Delta\bar{Y}=\underbrace{(\bar{X}_A-\bar{X}_B)'\beta^*}_{\text{Explained}}+\underbrace{\bar{X}_A'(\beta_A-\beta^*)+\bar{X}_B'(\beta^*-\beta_B)}_{\text{Unexplained}}" alt="Oaxaca Decomposition Equation" />
</div>

This library supports:

-   **GroupA / GroupB**: Uses $\beta_A$ (the compared, non-reference group) or $\beta_B$ (the group named by `reference_group`) as $\beta^*$.
-   **Pooled**: $\beta^*$ from one regression on both groups WITH a group indicator, whose coefficient is dropped from $\beta^*$ and is exactly the unexplained gap (Jann 2008 `pooled`; Fortin 2008; Elder, Goddeeris & Haider). Stata `oaxaca, pooled`; R `oaxaca` weight -2 for the aggregate.
-   **PooledNoIndicator**: $\beta^*$ from one regression on both groups WITHOUT an indicator (Neumark 1988; Stata `omega`; R `oaxaca` weight -1). `ReferenceCoefficients::Neumark` is a deprecated alias that computes `Pooled`, not this.
-   **Weighted (Cotton)**: Uses a weighted average: $\beta^* = w\beta_A + (1-w)\beta_B$ with $w$ the share of group A.

`reference_coefficients` is required at every engine boundary (WASM `decompose` / `verify_adjustments`, MCP, the engine crate). An absent value or any string other than `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator`, `Weighted` is an error; there is no default scheme.

</details>

<details>
<summary><strong>Deep Dive: Categorical Variables (Normalization)</strong></summary>

A dummy-coded categorical drops one level, so every other level's detailed contribution is measured against whichever level sorts first: rename that level and the per-level rows change while every aggregate stays the same. `normalize()` re-expresses the coefficients under a restriction $\sum_k s_k \beta_k = 0$ over ALL $k$ levels (the dropped level's coefficient is 0 before the transform), so every level, the dropped one included, is a deviation from a share-weighted average of the levels, and the intercept absorbs the average. Predictions and the aggregate decomposition do not change, and all $k$ levels are emitted.

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?\tilde{\beta}_{j}=\beta_{j}-c,\quad\tilde{\beta}_{0}=-c,\quad\tilde{\beta}_{\text{intercept}}=\beta_{\text{intercept}}+c,\quad c=\sum_{j\neq\text{base}}s_j\beta_j" alt="Normalization" />
</div>

Two conventions for $s$ (`normalization_convention`):

-   **Population share** (default; 0120-MERIDIAN D1): $s_k$ is the level's share of the pooled analysed rows of both groups, summed weights when a weights column is set. One share vector per run is applied to $\beta_A$, $\beta_B$, the pooled fit and the weighted mix alike, which keeps explained + unexplained = gap exact. Adding a small department changes only that department's row. No package implements this restriction by hand; it is checked against a base-R weighted-effect-coding refit (`verification/gen_norm_goldens.R`).
-   **Equal share**: $s_k = 1/m$. This is Yun (2005) / Gardeazabal & Ugidos (2004) and what Stata `categorical()`, R `oaxaca` (third formula part) and `ddecompose(normalize_factors = TRUE)` print; checked against `ddecompose` and R `oaxaca` to $10^{-10}$ ($2\times10^{-10}$ on the 10 000-row fixture).

The library keeps `normalize()` opt-in (a builder that never calls it returns raw treatment coding). The engine (WASM, MCP) and the CLI normalise every categorical predictor on every run; `oaxaca-cli --normalization none` returns raw coding. The three-fold aggregate is always computed from the raw coefficient vectors, because it is invariant to the coding and the normalised one is not. With a Heckman selection model normalisation is skipped on A, B and the pooled fit together and `run_metadata.normalization` says so. `run_metadata.normalization` records the convention, the share basis and the point sample's shares; bootstrap replicates recompute theirs from each replicate's own pooled resample.

</details>

<details>
<summary><strong>Deep Dive: JMP Decomposition</strong></summary>

The **Juhn-Murphy-Pierce (JMP)** method decomposes the *change* in the gap over time (or between distributions) into three components:

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?\Delta\bar{Y}=\underbrace{\Delta&space;X\beta}_{\text{Quantity&space;Effect}}+\underbrace{X\Delta\beta}_{\text{Price&space;Effect}}+\underbrace{\Delta\epsilon}_{\text{Gap&space;Effect}}" alt="JMP Decomposition Equation" />
</div>

1.  **Quantity Effect**: Changes in observable characteristics ($X$).
2.  **Price Effect**: Changes in returns to characteristics ($\beta$).
3.  **Gap Effect**: Changes in the distribution of unobserved residuals.

</details>

<details>
<summary><strong>Deep Dive: Abowd-Kramarz-Margolis (AKM) Model</strong></summary>

The AKM model decomposes wage variation into individual and firm-specific components:

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?y_{it}=\alpha_i+\psi_{J(i,t)}+x_{it}'\beta+\epsilon_{it}" alt="AKM Equation" />
</div>

- $\alpha_i$: Person fixed effect (unobserved ability).
- $\psi_{J(i,t)}$: Firm fixed effect (pay premium).
- $x_{it}$: Time-varying covariates.

**Identification**: The model is identified only within the **Largest Connected Set (LCS)** of workers and firms linked by mobility. This library automatically extracts the LCS using a graph-based approach (BFS) before estimation.

</details>

<details>
<summary><strong>Deep Dive: Propensity Score Matching (PSM)</strong></summary>

PSM estimates the Average Treatment Effect on the Treated (ATT) by matching treated units to control units with similar probabilities of treatment:

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?ATT=E[Y_{1i}-Y_{0i}|D_i=1]" alt="ATT Equation" />
</div>

1.  **Propensity Score**: $e(x) = P(D=1|X=x)$ estimated via Logistic Regression.
2.  **Matching**: Nearest Neighbor matching on the logit of the propensity score.
3.  **Balance**: Ensures that the distribution of covariates is similar between treated and matched control groups.

</details>

<details>
<summary><strong>Deep Dive: DFL Reweighting</strong></summary>

**DiNardo, Fortin, and Lemieux (1996)** proposed a non-parametric method to decompose the entire distribution of wages. It constructs a **counterfactual density** for Group B (e.g., women) as if they had the characteristics of Group A (e.g., men) by applying a reweighting factor $\Psi(x)$:

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?\Psi(x)=\frac{P(A|x)}{P(B|x)}\cdot\frac{P(B)}{P(A)}" alt="DFL Weight Equation" />
</div>

-   $P(A|x)$: Probability of belonging to Group A given characteristics $x$ (estimated via Probit/Logit).
-   $\Psi(x)$: The weight applied to each observation in Group B.

This allows for visual comparison of the "explained" gap across the entire distribution (e.g., via Kernel Density Estimation).

</details>

<details>
<summary><strong>Deep Dive: Heckman Correction (Selection Bias)</strong></summary>

Wage data is often observed only for those who are employed. If employment status is not random—for instance, if highly educated people are more likely to work—standard OLS decomposition will be biased (Selection Bias).

The **Heckman Two-Step Correction** addresses this:

1.  **Selection Equation (Probit)**: Estimates the probability of employment ($Z\gamma$) and calculates the **Inverse Mills Ratio** ($\lambda$).
2.  **Outcome Equation (OLS)**: Includes $\lambda$ as an additional regressor to correct for the selection bias.

<div align="center">
  <img src="https://latex.codecogs.com/svg.image?y = X\beta + \rho\sigma_u\lambda(Z\gamma) + \epsilon" alt="Heckman Equation" />
</div>

### Rust Example

```rust
// Requires a selection variable (e.g., "employed") and predictors
builder.heckman_selection("employed", &["age", "education", "marital_status"]);
```

### CLI Example

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
  --predictors education experience \
  --selection-outcome employed \
  --selection-predictors age education marital_status
```

</details>

---

## License

This project is licensed under the MIT License.

