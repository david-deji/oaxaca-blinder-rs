use crate::{ComponentResult, OaxacaResults, TwoFoldResults};
use pyo3::prelude::*;

#[pyclass(name = "ComponentResult")]
#[derive(Clone)]
pub struct PyComponentResult {
    inner: ComponentResult,
}

#[pymethods]
impl PyComponentResult {
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    #[getter]
    fn estimate(&self) -> f64 {
        *self.inner.estimate()
    }

    #[getter]
    fn std_err(&self) -> f64 {
        *self.inner.std_err()
    }

    #[getter]
    fn p_value(&self) -> f64 {
        *self.inner.p_value()
    }

    #[getter]
    fn ci_lower(&self) -> f64 {
        *self.inner.ci_lower()
    }

    #[getter]
    fn ci_upper(&self) -> f64 {
        *self.inner.ci_upper()
    }

    fn __repr__(&self) -> String {
        format!(
            "ComponentResult(name={}, estimate={})",
            self.name(),
            self.estimate()
        )
    }
}

#[pyclass(name = "TwoFoldResults")]
#[derive(Clone)]
pub struct PyTwoFoldResults {
    #[pyo3(get)]
    aggregate: Vec<PyComponentResult>,
    #[pyo3(get)]
    detailed_explained: Vec<PyComponentResult>,
    #[pyo3(get)]
    detailed_unexplained: Vec<PyComponentResult>,
    #[pyo3(get)]
    detailed_selection: Vec<PyComponentResult>,
}

impl From<&TwoFoldResults> for PyTwoFoldResults {
    fn from(results: &TwoFoldResults) -> Self {
        PyTwoFoldResults {
            aggregate: results
                .aggregate()
                .iter()
                .map(|c| PyComponentResult { inner: c.clone() })
                .collect(),
            detailed_explained: results
                .detailed_explained()
                .iter()
                .map(|c| PyComponentResult { inner: c.clone() })
                .collect(),
            detailed_unexplained: results
                .detailed_unexplained()
                .iter()
                .map(|c| PyComponentResult { inner: c.clone() })
                .collect(),
            detailed_selection: results
                .detailed_selection()
                .iter()
                .map(|c| PyComponentResult { inner: c.clone() })
                .collect(),
        }
    }
}

#[pyclass(name = "OaxacaResults")]
pub struct PyOaxacaResults {
    #[pyo3(get)]
    total_gap: f64,
    #[pyo3(get)]
    two_fold: PyTwoFoldResults,
    #[pyo3(get)]
    n_a: usize,
    #[pyo3(get)]
    n_b: usize,
    inner: OaxacaResults,
}

impl From<OaxacaResults> for PyOaxacaResults {
    fn from(results: OaxacaResults) -> Self {
        PyOaxacaResults {
            total_gap: *results.total_gap(),
            two_fold: PyTwoFoldResults::from(results.two_fold()),
            n_a: *results.n_a(),
            n_b: *results.n_b(),
            inner: results,
        }
    }
}

#[pymethods]
impl PyOaxacaResults {
    /// Generates a plot of the decomposition results.
    ///
    /// Args:
    ///     kind (str, optional): Type of plot ('bar' or 'waterfall'). Defaults to 'bar'.
    ///
    /// Returns:
    ///     matplotlib.figure.Figure: The generated figure.
    #[pyo3(signature = (kind="bar"))]
    fn plot(&self, py: Python, kind: &str) -> PyResult<PyObject> {
        let plt = py.import_bound("matplotlib.pyplot")?;
        let fig = plt.call_method0("figure")?;
        let ax = fig.call_method0("gca")?;

        let explained = self
            .two_fold
            .aggregate
            .iter()
            .find(|c| c.name() == "explained")
            .map(|c| c.estimate())
            .unwrap_or(0.0);
        let unexplained = self
            .two_fold
            .aggregate
            .iter()
            .find(|c| c.name() == "unexplained")
            .map(|c| c.estimate())
            .unwrap_or(0.0);

        // Simplified plotting logic for 'bar'
        if kind == "bar" {
            let labels = vec!["Explained", "Unexplained"];
            let values = vec![explained, unexplained];
            ax.call_method1("bar", (labels, values))?;
            ax.call_method1("set_title", ("Oaxaca-Blinder Decomposition",))?;
            ax.call_method1("set_ylabel", ("Gap Contribution",))?;
        }

        Ok(fig.into())
    }

    /// Interprets the results in plain English.
    fn interpret(&self) -> String {
        let explained = self
            .two_fold
            .aggregate
            .iter()
            .find(|c| c.name() == "explained")
            .map(|c| c.estimate())
            .unwrap_or(0.0);
        let unexplained = self
            .two_fold
            .aggregate
            .iter()
            .find(|c| c.name() == "unexplained")
            .map(|c| c.estimate())
            .unwrap_or(0.0);
        let total = self.total_gap;

        let exp_pct = (explained / total) * 100.0;
        let unexp_pct = (unexplained / total) * 100.0;

        format!(
            "The total gap is {:.4}. \n\
            {:.1}% of this gap is explained by differences in endowments (observables), \
            while {:.1}% is unexplained (coefficients/discrimination).",
            total, exp_pct, unexp_pct
        )
    }
}

use crate::OaxacaBuilder;
use polars::prelude::DataFrame;
use pyo3_polars::PyDataFrame;
use std::collections::HashMap;

#[pyclass(name = "OaxacaBlinder")]
pub struct PyOaxacaBlinder {
    dataframe: DataFrame,
    outcome: String,
    group: String,
    reference_group: String,
    predictors: Vec<String>,
    categorical_predictors: Vec<String>,
    bootstrap_reps: usize,
    weights: Option<String>,
    weights_kind: Option<String>,
    selection_outcome: Option<String>,
    selection_predictors: Vec<String>,
}

#[pymethods]
impl PyOaxacaBlinder {
    #[new]
    #[pyo3(
        signature = (dataframe, outcome, group, reference_group, predictors, categorical_predictors=Vec::new(), bootstrap_reps=100, weights=None, weights_kind=None, selection_outcome=None, selection_predictors=None)
    )]
    fn new(
        dataframe: PyDataFrame,
        outcome: String,
        group: String,
        reference_group: String,
        predictors: Vec<String>,
        categorical_predictors: Vec<String>,
        bootstrap_reps: usize,
        weights: Option<String>,
        weights_kind: Option<String>,
        selection_outcome: Option<String>,
        selection_predictors: Option<Vec<String>>,
    ) -> Self {
        let df: DataFrame = dataframe.0;
        Self {
            dataframe: df,
            outcome,
            group,
            reference_group,
            predictors,
            categorical_predictors,
            bootstrap_reps,
            weights,
            weights_kind,
            selection_outcome,
            selection_predictors: selection_predictors.unwrap_or_default(),
        }
    }

    fn fit(&self) -> PyResult<PyOaxacaResults> {
        let builder = self._create_builder();
        let results = builder
            .run()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(results.into())
    }

    fn fit_quantile(&self, quantile: f64) -> PyResult<PyOaxacaResults> {
        let builder = self._create_builder();
        let results = builder
            .decompose_quantile(quantile)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(results.into())
    }

    fn optimize_budget(&self, budget: f64, target_gap: f64) -> PyResult<Vec<HashMap<String, f64>>> {
        let mut builder = self._create_builder();
        builder.bootstrap_reps(0); // No bootstrap needed for budget optimization
        let results = builder
            .run()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let adjustments = results.optimize_budget(budget, target_gap);
        let mut py_adjustments = Vec::new();
        for adj in adjustments {
            let mut map = HashMap::new();
            map.insert("index".to_string(), adj.index as f64);
            map.insert("original_residual".to_string(), adj.original_residual);
            map.insert("adjustment".to_string(), adj.adjustment);
            py_adjustments.push(map);
        }
        Ok(py_adjustments)
    }
}

impl PyOaxacaBlinder {
    fn _create_builder(&self) -> OaxacaBuilder {
        let pred_refs: Vec<&str> = self.predictors.iter().map(|s| s.as_str()).collect();
        let cat_refs: Vec<&str> = self
            .categorical_predictors
            .iter()
            .map(|s| s.as_str())
            .collect();

        let mut builder = OaxacaBuilder::new(
            self.dataframe.clone(),
            &self.outcome,
            &self.group,
            &self.reference_group,
        );
        builder
            .predictors(&pred_refs)
            .categorical_predictors(&cat_refs)
            // Same rule as the CLI and the engine (0120-MERIDIAN T1): categorical predictors are
            // normalised. This module is not compiled today (`pub mod python` is commented out
            // in lib.rs); the call is here so re-enabling it does not fork the rule.
            .normalize_all_categoricals()
            .bootstrap_reps(self.bootstrap_reps);

        if let Some(w) = &self.weights {
            builder.weights(w);
            // `weights_kind` is "frequency" or "relative"; anything else (or absent) is left for
            // the builder to refuse with WEIGHTS_KIND_REQUIRED (0120-MERIDIAN S9).
            if let Some(kind) = self
                .weights_kind
                .as_deref()
                .and_then(|k| oaxaca_blinder::WeightsKind::parse_name(k).ok())
            {
                builder.weights_kind(kind);
            }
        }
        if let Some(so) = &self.selection_outcome {
            let sp_refs: Vec<&str> = self
                .selection_predictors
                .iter()
                .map(|s| s.as_str())
                .collect();
            builder.heckman_selection(so, &sp_refs);
        }
        builder
    }
}

/// Performs DFL Reweighting.
#[pyfunction]
#[pyo3(signature = (csv_path, outcome, group, reference_group, predictors))]
fn run_dfl_from_csv(
    csv_path: &str,
    outcome: &str,
    group: &str,
    reference_group: &str,
    predictors: Vec<String>,
) -> PyResult<PyDflResult> {
    use crate::run_dfl;
    use polars::prelude::*;

    let df = LazyCsvReader::new(csv_path)
        .finish()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("Failed to read CSV: {}", e))
        })?
        .collect()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to parse CSV: {}", e))
        })?;

    let result = run_dfl(&df, outcome, group, reference_group, &predictors).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("DFL failed: {:?}", e))
    })?;

    Ok(PyDflResult {
        grid: result.grid,
        density_a: result.density_a,
        density_b: result.density_b,
        density_b_counterfactual: result.density_b_counterfactual,
    })
}

#[pyclass]
struct PyDflResult {
    #[pyo3(get)]
    grid: Vec<f64>,
    #[pyo3(get)]
    density_a: Vec<f64>,
    #[pyo3(get)]
    density_b: Vec<f64>,
    #[pyo3(get)]
    density_b_counterfactual: Vec<f64>,
}

#[pymethods]
impl PyDflResult {
    fn plot(&self, py: Python) -> PyResult<PyObject> {
        let plt = py.import_bound("matplotlib.pyplot")?;
        let fig = plt.call_method0("figure")?;
        let ax = fig.call_method0("gca")?;

        ax.call_method1("plot", (self.grid.clone(), self.density_a.clone()))?; // Label A
        ax.call_method1("plot", (self.grid.clone(), self.density_b.clone()))?; // Label B
        ax.call_method1(
            "plot",
            (self.grid.clone(), self.density_b_counterfactual.clone()),
        )?; // Label Counterfactual

        ax.call_method1(
            "legend",
            (vec!["Group A", "Group B", "Group B (Counterfactual)"],),
        )?;
        ax.call_method1("set_title", ("DFL Decomposition: Density Estimates",))?;

        Ok(fig.into())
    }
}

/// Performs matching using the Matching Engine.
#[pyfunction]
#[pyo3(signature = (csv_path, treatment, outcome, covariates, k=1, method="euclidean"))]
fn match_units(
    csv_path: &str,
    treatment: &str,
    outcome: &str,
    covariates: Vec<String>,
    k: usize,
    method: &str,
) -> PyResult<Vec<f64>> {
    use crate::MatchingEngine;
    use polars::prelude::*;

    let df = LazyCsvReader::new(csv_path)
        .finish()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("Failed to read CSV: {}", e))
        })?
        .collect()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to parse CSV: {}", e))
        })?;

    let cov_refs: Vec<&str> = covariates.iter().map(|s| s.as_str()).collect();
    let engine = MatchingEngine::new(df, treatment, outcome, &cov_refs);

    let weights = if method == "psm" {
        engine.match_psm(k).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Matching failed: {:?}", e))
        })?
    } else {
        let use_mahalanobis = method == "mahalanobis";
        engine.run_matching(k, use_mahalanobis).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Matching failed: {:?}", e))
        })?
    };

    Ok(weights)
}

/// Python module for oaxaca_blinder
#[pymodule]
fn oaxaca_blinder(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyOaxacaBlinder>()?;
    m.add_class::<PyOaxacaResults>()?;
    m.add_class::<PyTwoFoldResults>()?;
    m.add_class::<PyComponentResult>()?;
    m.add_function(wrap_pyfunction!(run_dfl_from_csv, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_akm, m)?)?;
    m.add_function(wrap_pyfunction!(match_units, m)?)?;
    m.add_class::<PyAkmResult>()?;
    m.add_class::<PyDflResult>()?;
    Ok(())
}

#[pyclass]
struct PyAkmResult {
    beta: Vec<f64>,
    worker_effects: HashMap<String, f64>,
    firm_effects: HashMap<String, f64>,
    r2: f64,
}

#[pymethods]
impl PyAkmResult {
    #[getter]
    fn beta(&self) -> Vec<f64> {
        self.beta.clone()
    }

    #[getter]
    fn worker_effects(&self) -> HashMap<String, f64> {
        self.worker_effects.clone()
    }

    #[getter]
    fn firm_effects(&self) -> HashMap<String, f64> {
        self.firm_effects.clone()
    }

    #[getter]
    fn r2(&self) -> f64 {
        self.r2
    }
}

/// Estimates the AKM model from a CSV file.
#[pyfunction]
#[pyo3(signature = (csv_path, outcome, worker_col, firm_col, controls=None, tolerance=1e-8, max_iters=1000))]
fn estimate_akm(
    csv_path: &str,
    outcome: &str,
    worker_col: &str,
    firm_col: &str,
    controls: Option<Vec<String>>,
    tolerance: f64,
    max_iters: usize,
) -> PyResult<PyAkmResult> {
    use crate::AkmBuilder;
    use polars::prelude::*;

    let df = LazyCsvReader::new(csv_path)
        .finish()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("Failed to read CSV: {}", e))
        })?
        .collect()
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to parse CSV: {}", e))
        })?;

    let mut builder = AkmBuilder::new(df, outcome, worker_col, firm_col)
        .tolerance(tolerance)
        .max_iters(max_iters);

    if let Some(c) = controls {
        let refs: Vec<&str> = c.iter().map(|s| s.as_str()).collect();
        builder = builder.controls(&refs);
    }

    let result = builder.run().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("AKM estimation failed: {:?}", e))
    })?;

    // Convert DataFrames to HashMaps for Python
    let mut worker_effects = HashMap::new();
    let w_ids = result
        .worker_effects
        .column(worker_col)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?
        .cast(&DataType::String)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    let w_effs = result
        .worker_effects
        .column("effect")
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?
        .f64()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    let w_ids_utf8 = w_ids
        .str()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    for (id_opt, eff_opt) in w_ids_utf8.into_iter().zip(w_effs.into_iter()) {
        if let (Some(id), Some(eff)) = (id_opt, eff_opt) {
            worker_effects.insert(id.to_string(), eff);
        }
    }

    let mut firm_effects = HashMap::new();
    let f_ids = result
        .firm_effects
        .column(firm_col)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?
        .cast(&DataType::String)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    let f_effs = result
        .firm_effects
        .column("effect")
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?
        .f64()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    let f_ids_utf8 = f_ids
        .str()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

    for (id_opt, eff_opt) in f_ids_utf8.into_iter().zip(f_effs.into_iter()) {
        if let (Some(id), Some(eff)) = (id_opt, eff_opt) {
            firm_effects.insert(id.to_string(), eff);
        }
    }

    Ok(PyAkmResult {
        beta: result.beta.iter().cloned().collect(),
        worker_effects,
        firm_effects,
        r2: result.r2,
    })
}
