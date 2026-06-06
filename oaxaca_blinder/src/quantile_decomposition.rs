//! Machado-Mata Quantile Regression Decomposition
//!
//! This module provides the implementation for performing a quantile regression
//! decomposition using the Machado-Mata (2005) simulation-based method.

use getset::Getters;
use ndarray::{Array1, Array2};
use polars::prelude::*;
use rand::distributions::{Distribution, Uniform};
use rand::Rng;
use rayon::prelude::*;
use std::collections::HashMap;

use crate::{
    inference::bootstrap_stats, math::quantile_regression::solve_qr, ComponentResult, OaxacaError,
};

/// The main entry point for configuring and running a Machado-Mata
/// quantile regression decomposition.
#[derive(Debug, Clone)]
pub struct QuantileDecompositionBuilder {
    dataframe: DataFrame,
    outcome: String,
    group: String,
    reference_group: String,
    predictors: Vec<String>,
    categorical_predictors: Vec<String>,
    quantiles: Vec<f64>,
    simulations: usize,
    bootstrap_reps: usize,
}

/// Holds the raw decomposition results for a single quantile from one pass.
#[derive(Debug, Clone, Copy)]
struct DecomposedEffects {
    gap: f64,
    characteristics: f64,
    coefficients: f64,
}

/// Holds all decomposed effects for all target quantiles from one pass.
struct SinglePassResult {
    effects_by_quantile: HashMap<String, DecomposedEffects>,
}

impl QuantileDecompositionBuilder {
    /// Creates a new `QuantileDecompositionBuilder`.
    pub fn new(dataframe: DataFrame, outcome: &str, group: &str, reference_group: &str) -> Self {
        Self {
            dataframe,
            outcome: outcome.to_string(),
            group: group.to_string(),
            reference_group: reference_group.to_string(),
            predictors: Vec::new(),
            categorical_predictors: Vec::new(),
            quantiles: vec![0.1, 0.25, 0.5, 0.75, 0.9],
            simulations: 200,
            bootstrap_reps: 20,
        }
    }

    pub fn predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn categorical_predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.categorical_predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn quantiles(&mut self, quantiles: &[f64]) -> &mut Self {
        self.quantiles = quantiles.to_vec();
        self
    }

    pub fn simulations(&mut self, reps: usize) -> &mut Self {
        self.simulations = reps;
        self
    }

    pub fn bootstrap_reps(&mut self, reps: usize) -> &mut Self {
        self.bootstrap_reps = reps;
        self
    }

    #[allow(clippy::type_complexity)]
    fn prepare_data(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
    ) -> Result<(Array2<f64>, Array1<f64>, Vec<String>), OaxacaError> {
        let y_series = df.column(&self.outcome)?.f64()?;
        let y_vec: Vec<f64> = y_series
            .into_iter()
            .map(|opt| {
                opt.ok_or_else(|| {
                    OaxacaError::InvalidGroupVariable("Null outcome encountered".to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let y = Array1::from_vec(y_vec);

        let mut final_predictors: Vec<String> = vec!["__ob_intercept__".to_string()];
        final_predictors.extend_from_slice(&self.predictors);
        final_predictors.extend_from_slice(all_dummy_names);

        let mut x_df = df.select(&self.predictors)?;
        let intercept_series = Series::new("__ob_intercept__".into(), vec![1.0; df.height()]);
        x_df.with_column(intercept_series)?;

        for name in all_dummy_names {
            if df
                .get_column_names()
                .iter()
                .any(|s| s.as_str() == name.as_str())
            {
                x_df.with_column(df.column(name)?.clone())?;
            } else {
                let zero_series = Series::new(name.into(), vec![0.0; df.height()]);
                x_df.with_column(zero_series)?;
            }
        }

        let x_df_selected = x_df.select(final_predictors)?;
        let x = x_df_selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
        let final_names = x_df_selected
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        Ok((x, y, final_names))
    }

    fn create_dummies_manual(&self, series: &Series) -> Result<DataFrame, OaxacaError> {
        let unique_vals = series.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        let mut dummy_vars: Vec<Series> = Vec::new();

        for val in unique_vals.str()?.into_iter().flatten().skip(1) {
            let dummy_name = format!("{}_{}", series.name(), val);
            let ca = series.equal(val)?;
            let mut dummy_series = ca.into_series();
            dummy_series = dummy_series.cast(&DataType::Float64)?;
            dummy_series.rename(dummy_name.as_str().into());
            dummy_vars.push(dummy_series);
        }
        let cols: Vec<Column> = dummy_vars.into_iter().map(Column::Series).collect();
        DataFrame::new(cols).map_err(OaxacaError::from)
    }

    /// (Private) Calculates the empirical quantile of a slice of numbers.
    fn empirical_quantile(data: &mut [f64], quantile: f64) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        data.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let index = (data.len() as f64 * quantile) as usize;
        data[index.min(data.len() - 1)]
    }

    fn run_single_pass(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
    ) -> Result<SinglePassResult, OaxacaError> {
        let unique_groups = df.column(&self.group)?.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        if unique_groups.len() < 2 {
            return Err(OaxacaError::InvalidGroupVariable(
                "Not enough groups".to_string(),
            ));
        }
        let group_b_name = self.reference_group.as_str();
        let group_a_name = unique_groups.str()?.get(0).unwrap_or(group_b_name);
        let group_a_name = if group_a_name == group_b_name {
            unique_groups.str()?.get(1).unwrap_or("")
        } else {
            group_a_name
        };

        let df_a = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_a_name)?,
        )?;
        let df_b = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_b_name)?,
        )?;
        if df_a.height() < 2 || df_b.height() < 2 {
            return Err(OaxacaError::InvalidGroupVariable(
                "One group has insufficient data".to_string(),
            ));
        }

        let (x_a, y_a, _) = self.prepare_data(&df_a, all_dummy_names)?;
        let (x_b, y_b, _) = self.prepare_data(&df_b, all_dummy_names)?;

        let mut rng = rand::thread_rng();
        let uniform_dist = Uniform::from(0.01..0.99);
        let random_quantiles: Vec<f64> = (0..self.simulations)
            .map(|_| uniform_dist.sample(&mut rng))
            .collect();

        let betas_a: Vec<Array1<f64>> = random_quantiles
            .par_iter()
            .filter_map(|&tau| solve_qr(&x_a, &y_a, tau).ok())
            .map(Array1::from)
            .collect();
        let betas_b: Vec<Array1<f64>> = random_quantiles
            .par_iter()
            .filter_map(|&tau| solve_qr(&x_b, &y_b, tau).ok())
            .map(Array1::from)
            .collect();

        if betas_a.len() < self.simulations / 2 || betas_b.len() < self.simulations / 2 {
            return Err(OaxacaError::NalgebraError(
                "Failed to estimate a sufficient number of quantile regressions.".to_string(),
            ));
        }

        let num_successful_sims = std::cmp::min(betas_a.len(), betas_b.len());

        let mut y_aa_vec = Vec::with_capacity(num_successful_sims);
        let mut y_bb_vec = Vec::with_capacity(num_successful_sims);
        let mut y_ab_vec = Vec::with_capacity(num_successful_sims);

        let mut rng_resample = rand::thread_rng();

        for i in 0..num_successful_sims {
            let rand_idx_a = rng_resample.gen_range(0..x_a.nrows());
            let rand_idx_b = rng_resample.gen_range(0..x_b.nrows());

            let x_a_i = x_a.row(rand_idx_a);
            let x_b_i = x_b.row(rand_idx_b);

            let beta_a_i = &betas_a[i];
            let beta_b_i = &betas_b[i];

            y_aa_vec.push(x_a_i.dot(beta_a_i));
            y_bb_vec.push(x_b_i.dot(beta_b_i));
            y_ab_vec.push(x_a_i.dot(beta_b_i));
        }

        let mut effects_by_quantile = HashMap::new();
        for &tau in &self.quantiles {
            let q_aa = Self::empirical_quantile(&mut y_aa_vec, tau);
            let q_bb = Self::empirical_quantile(&mut y_bb_vec, tau);
            let q_ab = Self::empirical_quantile(&mut y_ab_vec, tau);

            let effects = DecomposedEffects {
                gap: q_aa - q_bb,
                characteristics: q_ab - q_bb,
                coefficients: q_aa - q_ab,
            };
            let key = format!("q{}", (tau * 100.0) as u32);
            effects_by_quantile.insert(key, effects);
        }

        Ok(SinglePassResult {
            effects_by_quantile,
        })
    }

    pub fn run(&self) -> Result<QuantileDecompositionResults, OaxacaError> {
        let mut cols_to_select = vec![self.outcome.clone(), self.group.clone()];
        cols_to_select.extend(self.predictors.clone());
        cols_to_select.extend(self.categorical_predictors.clone());

        // 1. Clean data first to ensure quantiles are calculated on non-null data
        let mut df = self.dataframe.select(&cols_to_select)?;

        let mut all_dummy_names = Vec::new();
        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let dummies = self.create_dummies_manual(series.as_materialized_series())?;
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        let unique_groups = self
            .dataframe
            .column(&self.group)?
            .unique()?
            .sort(SortOptions {
                descending: false,
                nulls_last: false,
                ..Default::default()
            })?;
        let group_b_name = self.reference_group.as_str();
        let group_a_name_temp = unique_groups
            .str()?
            .get(0)
            .unwrap_or(self.reference_group.as_str());
        let group_a_name = if group_a_name_temp == group_b_name {
            unique_groups.str()?.get(1).unwrap_or("")
        } else {
            group_a_name_temp
        };

        let point_estimates = self.run_single_pass(&df, &all_dummy_names)?;

        let group_a_name_owned = group_a_name.to_string();
        let group_b_name_owned = group_b_name.to_string();

        let df_a_global = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_a_name_owned.as_str())?,
        )?;
        let df_b_global = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_b_name_owned.as_str())?,
        )?;

        let bootstrap_results: Vec<SinglePassResult> = (0..self.bootstrap_reps)
            .into_par_iter()
            .filter_map(|_| {
                let df_a = df_a_global.clone();
                let df_b = df_b_global.clone();

                let sample_a = df_a
                    .sample_n_literal(df_a.height(), true, false, None)
                    .ok()?;
                let sample_b = df_b
                    .sample_n_literal(df_b.height(), true, false, None)
                    .ok()?;

                let sample_df = sample_a.vstack(&sample_b).ok()?;

                self.run_single_pass(&sample_df, &all_dummy_names).ok()
            })
            .collect();

        let mut final_results = HashMap::new();
        for (key, point) in &point_estimates.effects_by_quantile {
            let gap_dist: Vec<f64> = bootstrap_results
                .iter()
                .filter_map(|r| r.effects_by_quantile.get(key).map(|e| e.gap))
                .collect();
            let char_dist: Vec<f64> = bootstrap_results
                .iter()
                .filter_map(|r| r.effects_by_quantile.get(key).map(|e| e.characteristics))
                .collect();
            let coeff_dist: Vec<f64> = bootstrap_results
                .iter()
                .filter_map(|r| r.effects_by_quantile.get(key).map(|e| e.coefficients))
                .collect();

            let (gap_std_err, gap_p_val, (gap_ci_low, gap_ci_high)) =
                bootstrap_stats(&gap_dist, point.gap);
            let (char_std_err, char_p_val, (char_ci_low, char_ci_high)) =
                bootstrap_stats(&char_dist, point.characteristics);
            let (coeff_std_err, coeff_p_val, (coeff_ci_low, coeff_ci_high)) =
                bootstrap_stats(&coeff_dist, point.coefficients);

            let detail = QuantileDecompositionDetail {
                total_gap: ComponentResult {
                    name: "Total Gap".to_string(),
                    estimate: point.gap,
                    std_err: gap_std_err,
                    t_stat: if gap_std_err.abs() > 1e-9 {
                        point.gap / gap_std_err
                    } else {
                        0.0
                    },
                    p_value: gap_p_val,
                    ci_lower: gap_ci_low,
                    ci_upper: gap_ci_high,
                },
                characteristics_effect: ComponentResult {
                    name: "Characteristics".to_string(),
                    estimate: point.characteristics,
                    std_err: char_std_err,
                    t_stat: if char_std_err.abs() > 1e-9 {
                        point.characteristics / char_std_err
                    } else {
                        0.0
                    },
                    p_value: char_p_val,
                    ci_lower: char_ci_low,
                    ci_upper: char_ci_high,
                },
                coefficients_effect: ComponentResult {
                    name: "Coefficients".to_string(),
                    estimate: point.coefficients,
                    std_err: coeff_std_err,
                    t_stat: if coeff_std_err.abs() > 1e-9 {
                        point.coefficients / coeff_std_err
                    } else {
                        0.0
                    },
                    p_value: coeff_p_val,
                    ci_lower: coeff_ci_low,
                    ci_upper: coeff_ci_high,
                },
            };
            final_results.insert(key.clone(), detail);
        }

        Ok(QuantileDecompositionResults {
            results_by_quantile: final_results,
            n_a: self
                .dataframe
                .filter(
                    &self
                        .dataframe
                        .column(&self.group)?
                        .as_materialized_series()
                        .equal(group_a_name)?,
                )?
                .height(),
            n_b: self
                .dataframe
                .filter(
                    &self
                        .dataframe
                        .column(&self.group)?
                        .as_materialized_series()
                        .equal(group_b_name)?,
                )?
                .height(),
        })
    }
}

/// Holds all the results from the quantile regression decomposition.
#[derive(Debug, Getters)]
#[getset(get = "pub")]
pub struct QuantileDecompositionResults {
    /// A map where keys are quantile labels (e.g., "q10") and values are the
    /// decomposition results for that quantile.
    results_by_quantile: HashMap<String, QuantileDecompositionDetail>,
    /// The number of observations in the advantaged group (Group A).
    n_a: usize,
    /// The number of observations in the reference group (Group B).
    n_b: usize,
}

impl QuantileDecompositionResults {
    /// Prints a summary of the quantile decomposition.
    #[cfg(feature = "display")]
    pub fn summary(&self) {
        use comfy_table::{Cell, Table};

        println!("Machado-Mata Quantile Decomposition Results");
        println!("============================================");
        println!("Group A (Advantaged): {} observations", self.n_a);
        println!("Group B (Reference):  {} observations", self.n_b);

        let mut sorted_quantiles: Vec<_> = self.results_by_quantile.keys().collect();
        sorted_quantiles.sort();

        for quantile_key in sorted_quantiles {
            let results = self
                .results_by_quantile
                .get(quantile_key)
                .expect("Quantile key should exist in results");
            println!("\n--- Decomposition for Quantile: {} ---", quantile_key);

            let mut table = Table::new();
            table.set_header(vec![
                "Component",
                "Estimate",
                "Std. Err.",
                "p-value",
                "95% CI",
            ]);

            let components = vec![
                results.total_gap(),
                results.characteristics_effect(),
                results.coefficients_effect(),
            ];

            for component in components {
                let ci = format!("[{:.3}, {:.3}]", component.ci_lower(), component.ci_upper());
                table.add_row(vec![
                    Cell::new(component.name()),
                    Cell::new(format!("{:.4}", component.estimate())),
                    Cell::new(format!("{:.4}", component.std_err())),
                    Cell::new(format!("{:.4}", component.p_value())),
                    Cell::new(ci),
                ]);
            }
            println!("{}", table);
        }
    }
}

/// Holds the decomposition results for a single quantile.
#[derive(Debug, Getters, Clone)]
#[getset(get = "pub")]
pub struct QuantileDecompositionDetail {
    /// The total difference in the outcome at this quantile.
    total_gap: ComponentResult,
    /// The component of the gap explained by differences in characteristics.
    characteristics_effect: ComponentResult,
    /// The component of the gap explained by differences in coefficients (returns).
    coefficients_effect: ComponentResult,
}
