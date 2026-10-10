use getset::Getters;
use nalgebra::DVector;
use serde::Serialize;

use crate::decomposition::BudgetAdjustment;
use crate::rng::RunMetadata;

/// Holds results for the two-fold decomposition, including detailed components.
#[derive(Debug, Getters, Serialize)]
#[getset(get = "pub")]
pub struct TwoFoldResults {
    /// Aggregate results for the explained and unexplained components.
    pub aggregate: Vec<ComponentResult>,
    /// Detailed results for the explained component, broken down by variable.
    pub detailed_explained: Vec<ComponentResult>,
    /// Detailed results for the unexplained component, broken down by variable.
    pub detailed_unexplained: Vec<ComponentResult>,
    /// Detailed results for the selection component (Heckman only).
    pub detailed_selection: Vec<ComponentResult>,
}

/// Holds all the results from the Oaxaca-Blinder decomposition.
#[derive(Debug, Getters, Serialize)]
#[getset(get = "pub")]
pub struct OaxacaResults {
    /// The total difference in the mean outcome between the two groups.
    pub total_gap: f64,
    /// The results of the two-fold decomposition.
    pub two_fold: TwoFoldResults,
    /// The results of the three-fold decomposition.
    pub three_fold: DecompositionDetail,
    /// The number of observations in the advantaged group (Group A).
    pub n_a: usize,
    /// The number of observations in the reference group (Group B).
    pub n_b: usize,
    /// The residuals of the reference group (Group B) from the decomposition model.
    /// These represent the "unexplained" part of the outcome for each individual.
    pub residuals: Vec<f64>,
    /// The mean of the predictors for Group A.
    #[serde(skip)]
    pub xa_mean: DVector<f64>,
    /// The mean of the predictors for Group B.
    #[serde(skip)]
    pub xb_mean: DVector<f64>,
    /// The reference coefficients used in the decomposition.
    #[serde(skip)]
    pub beta_star: DVector<f64>,
    /// Provenance of this run: master seed, RNG algorithm/version, and bootstrap-rep
    /// accounting. Serialized so a run is reproducible after the fact.
    pub run_metadata: RunMetadata,
}

impl OaxacaResults {
    pub fn explained(&self) -> Option<&ComponentResult> {
        self.two_fold
            .aggregate()
            .iter()
            .find(|c| c.name == "explained")
    }

    pub fn unexplained(&self) -> Option<&ComponentResult> {
        self.two_fold
            .aggregate()
            .iter()
            .find(|c| c.name == "unexplained")
    }

    pub fn get_summary_table(&self) -> Vec<(&String, &ComponentResult)> {
        self.two_fold
            .aggregate()
            .iter()
            .map(|c| (&c.name, c))
            .collect()
    }

    pub fn get_detailed_table(&self) -> Vec<(String, f64, f64)> {
        let mut map = std::collections::HashMap::new();
        for comp in self.two_fold.detailed_explained() {
            map.entry(comp.name().clone()).or_insert((0.0, 0.0)).0 = *comp.estimate();
        }
        for comp in self.two_fold.detailed_unexplained() {
            map.entry(comp.name().clone()).or_insert((0.0, 0.0)).1 = *comp.estimate();
        }
        map.into_iter().map(|(k, (v1, v2))| (k, v1, v2)).collect()
    }

    /// Optimizes the allocation of a remediation budget to reduce the pay gap.
    ///
    /// This method identifies the individuals in the reference group (Group B) with the largest
    /// negative unexplained residuals (i.e., those who are most underpaid relative to their
    /// observable characteristics) and calculates the necessary adjustments to bring them
    /// closer to their predicted pay, subject to the budget and target gap constraints.
    ///
    /// # Deprecated
    ///
    /// This is not the remedy the Meridian product runs. The engine's `optimize`
    /// (`pay-equity-engine`) raises each compared employee below a fitted pay line up to it, never
    /// above, and reports what that costs and does to the compared group's gap. This method
    /// allocates on the residuals of the decomposition's own fit, with the library's convention
    /// for which group is which, and `target_gap` here is the RAW total gap between the two
    /// groups' mean pay, not the compared group's gap to a pay line.
    ///
    /// # Arguments
    ///
    /// * `budget` - The maximum total amount to spend on adjustments.
    /// * `target_gap` - The desired final pay gap (difference in means).
    ///
    /// # Returns
    ///
    /// A vector of `BudgetAdjustment` structs detailing who should get a raise and how much.
    ///
    /// # Row indices (0118-MERIDIAN S3: out of scope)
    ///
    /// The indices this method returns are positions within the CLEANED group frame (rows with
    /// a blank model cell already dropped), NOT original row ordinals of the input file. They
    /// are therefore not safe to map back to an employee when any row was excluded. No shipped
    /// surface calls this method (the engine's `optimize` has its own allocator that reads
    /// original ordinals from `OaxacaBuilder::get_data_matrices_with_rows`, and the Python
    /// binding in `python.rs` is not compiled). A caller that needs employee identity should use
    /// `get_data_matrices_with_rows` instead.
    #[deprecated(
        since = "0.3.0",
        note = "the product path is the engine's `optimize` (pay-equity-engine); this method \
                allocates on the cleaned-frame residuals of the decomposition, targets the raw \
                `total_gap`, and keeps the library's own group convention"
    )]
    pub fn optimize_budget(&self, budget: f64, target_gap: f64) -> Vec<BudgetAdjustment> {
        // A budget or a target that is not a number pays nothing: `f64::min` returns the other
        // argument when one is NaN, which used to fund the full need (0122-MERIDIAN C-09).
        if !budget.is_finite() || !target_gap.is_finite() {
            return Vec::new();
        }
        let current_gap = self.total_gap;
        // If the gap is already smaller than or equal to the target, no adjustments needed.
        if current_gap <= target_gap {
            return Vec::new();
        }

        let required_reduction = current_gap - target_gap;
        // Total amount needed to reduce the gap by required_reduction is required_reduction * n_b
        let total_needed = required_reduction * self.n_b as f64;

        // We can't spend more than the budget, and we don't need to spend more than total_needed.
        let effective_budget = budget.min(total_needed);

        // Identify underpaid individuals (negative residuals) in Group B
        let mut candidates: Vec<(usize, f64)> = self
            .residuals
            .iter()
            .enumerate()
            .filter(|(_, &r)| r < 0.0)
            .map(|(i, &r)| (i, r))
            .collect();

        // Sort by residual ascending (most negative first).
        // We want to fix the largest underpayments first.
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut adjustments = Vec::new();
        let mut spent = 0.0;

        for (index, residual) in candidates {
            if spent >= effective_budget {
                break;
            }

            // The maximum raise for this individual is the amount to bring their residual to 0.
            let max_raise = -residual;
            let remaining_budget = effective_budget - spent;

            // Give them the full correction or whatever is left in the budget/needed.
            let raise = if max_raise <= remaining_budget {
                max_raise
            } else {
                remaining_budget
            };

            // Avoid tiny adjustments due to floating point precision
            if raise > 1e-9 {
                adjustments.push(BudgetAdjustment {
                    index,
                    original_residual: residual,
                    adjustment: raise,
                });
                spent += raise;
            }
        }

        adjustments
    }
}

/// Represents a component of the decomposition (e.g., two-fold or three-fold).
#[derive(Debug, Getters, Serialize)]
#[getset(get = "pub")]
pub struct DecompositionDetail {
    /// Aggregate results for this decomposition component (e.g., "Explained", "Unexplained").
    pub aggregate: Vec<ComponentResult>,
    /// Detailed results broken down by each predictor variable.
    pub detailed: Vec<ComponentResult>,
}

/// Represents the calculated result for a single component or variable.
#[derive(Debug, Getters, Clone, Serialize)]
#[getset(get = "pub")]
pub struct ComponentResult {
    pub name: String,
    pub estimate: f64,
    pub std_err: f64,
    pub t_stat: f64,
    pub p_value: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}
