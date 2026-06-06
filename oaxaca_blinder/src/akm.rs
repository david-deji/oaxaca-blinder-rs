use nalgebra::{DMatrix, DVector};
use polars::prelude::*;
use std::collections::{HashMap, HashSet};

/// Error type for AKM operations
#[derive(Debug)]
pub enum AkmError {
    PolarsError(PolarsError),
    ColumnNotFound(String),
    NotEnoughData(String),
    ConvergenceFailed(String),
}

impl From<PolarsError> for AkmError {
    fn from(err: PolarsError) -> Self {
        AkmError::PolarsError(err)
    }
}

impl std::fmt::Display for AkmError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AkmError::PolarsError(e) => write!(f, "Polars error: {}", e),
            AkmError::ColumnNotFound(s) => write!(f, "Column not found: {}", s),
            AkmError::NotEnoughData(s) => write!(f, "Not enough data: {}", s),
            AkmError::ConvergenceFailed(s) => write!(
                f,
                "Convergence failed: {}. Try increasing max_iters or checking for collinearity.",
                s
            ),
        }
    }
}

impl std::error::Error for AkmError {}

/// Result of the AKM estimation
#[derive(Debug, Clone)]
pub struct AkmResult {
    pub beta: DVector<f64>,
    pub worker_effects: DataFrame,
    pub firm_effects: DataFrame,
    pub r2: f64,
}

/// Builder for AKM estimation
pub struct AkmBuilder {
    dataframe: DataFrame,
    outcome: String,
    worker_col: String,
    firm_col: String,
    controls: Vec<String>,
    tolerance: f64,
    max_iters: usize,
}

impl AkmBuilder {
    pub fn new(dataframe: DataFrame, outcome: &str, worker_col: &str, firm_col: &str) -> Self {
        Self {
            dataframe,
            outcome: outcome.to_string(),
            worker_col: worker_col.to_string(),
            firm_col: firm_col.to_string(),
            controls: Vec::new(),
            tolerance: 1e-8,
            max_iters: 1000,
        }
    }

    pub fn controls<I, S>(&mut self, controls: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.controls = controls.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn tolerance(&mut self, tol: f64) -> &mut Self {
        self.tolerance = tol;
        self
    }

    pub fn max_iters(&mut self, iters: usize) -> &mut Self {
        self.max_iters = iters;
        self
    }

    pub fn run(&self) -> Result<AkmResult, AkmError> {
        // 1. Find Largest Connected Set
        let df_connected =
            find_largest_connected_set(&self.dataframe, &self.worker_col, &self.firm_col)?;

        if df_connected.height() == 0 {
            return Err(AkmError::NotEnoughData(
                "No connected set found".to_string(),
            ));
        }

        // 2. Solve AKM
        solve_akm(
            df_connected,
            &self.outcome,
            &self.worker_col,
            &self.firm_col,
            &self.controls,
            self.tolerance,
            self.max_iters,
        )
    }
}

/// Union-Find data structure for finding connected components
struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] != i {
            self.parent[i] = self.find(self.parent[i]);
        }
        self.parent[i]
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);

        if root_i != root_j {
            if self.size[root_i] < self.size[root_j] {
                self.parent[root_i] = root_j;
                self.size[root_j] += self.size[root_i];
            } else {
                self.parent[root_j] = root_i;
                self.size[root_i] += self.size[root_j];
            }
        }
    }
}

/// Finds the largest connected set of workers and firms.
fn find_largest_connected_set(
    df: &DataFrame,
    worker_col: &str,
    firm_col: &str,
) -> Result<DataFrame, AkmError> {
    let workers = df.column(worker_col)?.cast(&DataType::String)?;
    let firms = df.column(firm_col)?.cast(&DataType::String)?;

    let unique_workers = workers.unique()?.sort(SortOptions {
        descending: false,
        nulls_last: false,
        ..Default::default()
    })?;
    let unique_firms = firms.unique()?.sort(SortOptions {
        descending: false,
        nulls_last: false,
        ..Default::default()
    })?;

    let n_workers = unique_workers.len();
    let n_firms = unique_firms.len();

    // Map IDs to integers
    let mut worker_map = HashMap::new();
    for (i, w) in unique_workers.str()?.into_iter().flatten().enumerate() {
        worker_map.insert(w, i);
    }

    let mut firm_map = HashMap::new();
    for (i, f) in unique_firms.str()?.into_iter().flatten().enumerate() {
        firm_map.insert(f, i + n_workers); // Offset firm indices
    }

    let mut uf = UnionFind::new(n_workers + n_firms);

    // Iterate through edges and union
    let worker_iter = workers.str()?.into_iter();
    let firm_iter = firms.str()?.into_iter();

    for (w_opt, f_opt) in worker_iter.zip(firm_iter) {
        if let (Some(w), Some(f)) = (w_opt, f_opt) {
            if let (Some(&w_idx), Some(&f_idx)) = (worker_map.get(w), firm_map.get(f)) {
                uf.union(w_idx, f_idx);
            }
        }
    }

    // Find largest component
    let mut component_sizes = HashMap::new();
    for i in 0..(n_workers + n_firms) {
        let root = uf.find(i);
        *component_sizes.entry(root).or_insert(0) += 1;
    }

    let largest_component_root = component_sizes
        .iter()
        .max_by_key(|&(_, size)| size)
        .map(|(&root, _)| root)
        .ok_or(AkmError::NotEnoughData("Empty graph".to_string()))?;

    // Identify valid workers and firms first
    let mut valid_nodes = HashSet::new();
    for i in 0..(n_workers + n_firms) {
        if uf.find(i) == largest_component_root {
            valid_nodes.insert(i);
        }
    }

    let mask: BooleanChunked = workers
        .str()?
        .into_iter()
        .zip(firms.str()?)
        .map(|(w_opt, f_opt)| {
            if let (Some(w), Some(f)) = (w_opt, f_opt) {
                if let (Some(&w_idx), Some(&f_idx)) = (worker_map.get(w), firm_map.get(f)) {
                    return Some(valid_nodes.contains(&w_idx) && valid_nodes.contains(&f_idx));
                }
            }
            Some(false)
        })
        .collect();

    df.filter(&mask).map_err(AkmError::from)
}

fn solve_akm(
    df: DataFrame,
    outcome: &str,
    worker_col: &str,
    firm_col: &str,
    controls: &[String],
    tolerance: f64,
    max_iters: usize,
) -> Result<AkmResult, AkmError> {
    // 1. Prepare Data
    let workers = df.column(worker_col)?.cast(&DataType::String)?;
    let firms = df.column(firm_col)?.cast(&DataType::String)?;

    let unique_workers = workers.unique()?.sort(SortOptions {
        descending: false,
        nulls_last: false,
        ..Default::default()
    })?;
    let unique_firms = firms.unique()?.sort(SortOptions {
        descending: false,
        nulls_last: false,
        ..Default::default()
    })?;

    let n_workers = unique_workers.len();
    let n_firms = unique_firms.len();

    let mut worker_map = HashMap::new();
    for (i, w) in unique_workers.str()?.into_iter().flatten().enumerate() {
        worker_map.insert(w.to_string(), i as u32);
    }

    let mut firm_map = HashMap::new();
    for (i, f) in unique_firms.str()?.into_iter().flatten().enumerate() {
        firm_map.insert(f.to_string(), i as u32);
    }

    let worker_indices: Vec<u32> = workers
        .str()?
        .into_iter()
        .map(|opt| {
            opt.ok_or_else(|| AkmError::NotEnoughData("Null worker ID encountered".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|id| {
            worker_map
                .get(id)
                .ok_or_else(|| AkmError::NotEnoughData("Worker ID not in map".to_string()))
                .copied()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let firm_indices: Vec<u32> = firms
        .str()?
        .into_iter()
        .map(|opt| {
            opt.ok_or_else(|| AkmError::NotEnoughData("Null firm ID encountered".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|id| {
            firm_map
                .get(id)
                .ok_or_else(|| AkmError::NotEnoughData("Firm ID not in map".to_string()))
                .copied()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let y_series = df.column(outcome)?.f64()?;
    let y: Vec<f64> = y_series
        .into_iter()
        .map(|opt| {
            opt.ok_or_else(|| AkmError::NotEnoughData("Null outcome encountered".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Prepare X (controls)
    let mut x_vectors: Vec<Vec<f64>> = Vec::new();
    for col in controls {
        let s = df.column(col)?.f64()?;
        x_vectors.push(s.into_iter().map(|opt| opt.unwrap_or(0.0)).collect());
    }

    // 2. Demean Y and X (FWL Theorem)
    // We need to demean Y and each column of X to remove worker and firm fixed effects

    let mut y_resid = y.clone();
    demean_vector(
        &mut y_resid,
        &worker_indices,
        &firm_indices,
        n_workers,
        n_firms,
        tolerance,
        max_iters,
    )?;

    let mut x_resids = x_vectors.clone();
    for x_vec in &mut x_resids {
        demean_vector(
            x_vec,
            &worker_indices,
            &firm_indices,
            n_workers,
            n_firms,
            tolerance,
            max_iters,
        )?;
    }

    // 3. Run OLS: y_resid ~ x_resids
    // Construct design matrix for OLS
    let n_obs = y.len();
    let n_controls = controls.len();

    let beta = if n_controls > 0 {
        let x_mat = DMatrix::from_fn(n_obs, n_controls, |r, c| x_resids[c][r]);
        let y_vec = DVector::from_column_slice(&y_resid);

        // OLS: (X'X)^-1 X'Y
        let xt = x_mat.transpose();
        let xtx = &xt * &x_mat;
        let xty = &xt * &y_vec;

        let chol = nalgebra::linalg::Cholesky::new(xtx).ok_or(AkmError::ConvergenceFailed(
            "OLS design matrix is singular".to_string(),
        ))?;
        chol.solve(&xty)
    } else {
        DVector::zeros(0)
    };

    // 4. Recover Fixed Effects
    // R = Y - X * beta
    // R = alpha + psi + epsilon
    // We solve for alpha and psi using alternating projections

    let mut r = y.clone();
    if n_controls > 0 {
        for i in 0..n_obs {
            for j in 0..n_controls {
                r[i] -= x_vectors[j][i] * beta[j];
            }
        }
    }

    let (alpha, psi) = recover_fe(
        &r,
        &worker_indices,
        &firm_indices,
        n_workers,
        n_firms,
        tolerance,
        max_iters,
    )?;

    // 5. Calculate R2
    // TSS = sum((Y - mean(Y))^2)
    // RSS = sum((Y - X*beta - alpha - psi)^2)
    let y_mean = y.iter().sum::<f64>() / n_obs as f64;
    let tss: f64 = y.iter().map(|&val| (val - y_mean).powi(2)).sum();

    let mut rss = 0.0;
    for i in 0..n_obs {
        let w_idx = worker_indices[i] as usize;
        let f_idx = firm_indices[i] as usize;
        let pred = if n_controls > 0 {
            let mut xb = 0.0;
            for j in 0..n_controls {
                xb += x_vectors[j][i] * beta[j];
            }
            xb + alpha[w_idx] + psi[f_idx]
        } else {
            alpha[w_idx] + psi[f_idx]
        };
        rss += (y[i] - pred).powi(2);
    }

    let r2 = 1.0 - (rss / tss);

    // 6. Format Output
    let worker_ids: Vec<String> = unique_workers
        .str()?
        .into_iter()
        .flatten()
        .map(|s| s.to_string())
        .collect();
    let firm_ids: Vec<String> = unique_firms
        .str()?
        .into_iter()
        .flatten()
        .map(|s| s.to_string())
        .collect();

    let worker_effects_df = df!(
        worker_col => worker_ids,
        "effect" => alpha
    )
    .map_err(AkmError::from)?;

    let firm_effects_df = df!(
        firm_col => firm_ids,
        "effect" => psi
    )
    .map_err(AkmError::from)?;

    Ok(AkmResult {
        beta,
        worker_effects: worker_effects_df,
        firm_effects: firm_effects_df,
        r2,
    })
}

/// Iteratively demeans a vector by worker and firm groups (Zig-Zag algorithm)
fn demean_vector(
    vec: &mut [f64],
    worker_indices: &[u32],
    firm_indices: &[u32],
    n_workers: usize,
    n_firms: usize,
    tolerance: f64,
    max_iters: usize,
) -> Result<(), AkmError> {
    let n_obs = vec.len();
    let mut diff = tolerance + 1.0;
    let mut iter = 0;

    // Pre-calculate counts for means
    let mut worker_counts = vec![0usize; n_workers];
    let mut firm_counts = vec![0usize; n_firms];

    for &idx in worker_indices {
        worker_counts[idx as usize] += 1;
    }
    for &idx in firm_indices {
        firm_counts[idx as usize] += 1;
    }

    let mut prev_vec = vec![0.0; n_obs];
    let mut worker_sums = vec![0.0; n_workers];
    let mut firm_sums = vec![0.0; n_firms];

    while diff > tolerance && iter < max_iters {
        prev_vec.copy_from_slice(vec);

        // 1. Demean by Worker
        worker_sums.fill(0.0);
        for i in 0..n_obs {
            worker_sums[worker_indices[i] as usize] += vec[i];
        }

        for i in 0..n_obs {
            let w_idx = worker_indices[i] as usize;
            if worker_counts[w_idx] > 0 {
                vec[i] -= worker_sums[w_idx] / worker_counts[w_idx] as f64;
            }
        }

        // 2. Demean by Firm
        firm_sums.fill(0.0);
        for i in 0..n_obs {
            firm_sums[firm_indices[i] as usize] += vec[i];
        }

        for i in 0..n_obs {
            let f_idx = firm_indices[i] as usize;
            if firm_counts[f_idx] > 0 {
                vec[i] -= firm_sums[f_idx] / firm_counts[f_idx] as f64;
            }
        }

        // Check convergence
        diff = 0.0;
        for i in 0..n_obs {
            diff += (vec[i] - prev_vec[i]).powi(2);
        }
        diff = diff.sqrt();

        iter += 1;
    }

    if iter >= max_iters {
        return Err(AkmError::ConvergenceFailed(format!(
            "demean_vector failed to converge within {} iterations",
            max_iters
        )));
    }

    Ok(())
}

/// Recovers Fixed Effects using alternating projections
fn recover_fe(
    r: &[f64],
    worker_indices: &[u32],
    firm_indices: &[u32],
    n_workers: usize,
    n_firms: usize,
    tolerance: f64,
    max_iters: usize,
) -> Result<(Vec<f64>, Vec<f64>), AkmError> {
    let n_obs = r.len();
    let mut alpha = vec![0.0; n_workers];
    let mut psi = vec![0.0; n_firms];

    let mut worker_counts = vec![0usize; n_workers];
    let mut firm_counts = vec![0usize; n_firms];

    for &idx in worker_indices {
        worker_counts[idx as usize] += 1;
    }
    for &idx in firm_indices {
        firm_counts[idx as usize] += 1;
    }

    let mut diff = tolerance + 1.0;
    let mut iter = 0;

    let mut prev_alpha = vec![0.0; n_workers];
    let mut prev_psi = vec![0.0; n_firms];
    let mut worker_sums = vec![0.0; n_workers];
    let mut firm_sums = vec![0.0; n_firms];

    while diff > tolerance && iter < max_iters {
        prev_alpha.copy_from_slice(&alpha);
        prev_psi.copy_from_slice(&psi);

        // Update Alpha: alpha_i = mean(R - psi | i)
        worker_sums.fill(0.0);
        for i in 0..n_obs {
            let f_idx = firm_indices[i] as usize;
            worker_sums[worker_indices[i] as usize] += r[i] - psi[f_idx];
        }

        for i in 0..n_workers {
            if worker_counts[i] > 0 {
                alpha[i] = worker_sums[i] / worker_counts[i] as f64;
            }
        }

        // Update Psi: psi_j = mean(R - alpha | j)
        firm_sums.fill(0.0);
        for i in 0..n_obs {
            let w_idx = worker_indices[i] as usize;
            firm_sums[firm_indices[i] as usize] += r[i] - alpha[w_idx];
        }

        for j in 0..n_firms {
            if firm_counts[j] > 0 {
                psi[j] = firm_sums[j] / firm_counts[j] as f64;
            }
        }

        // Check convergence
        diff = 0.0;
        for i in 0..n_workers {
            diff += (alpha[i] - prev_alpha[i]).powi(2);
        }
        for j in 0..n_firms {
            diff += (psi[j] - prev_psi[j]).powi(2);
        }
        diff = diff.sqrt();

        iter += 1;
    }

    if iter >= max_iters {
        return Err(AkmError::ConvergenceFailed(format!(
            "recover_fe failed to converge within {} iterations",
            max_iters
        )));
    }

    // Normalize one firm to zero (e.g., first firm) to identify the model
    let ref_val = psi[0];
    for p in psi.iter_mut().take(n_firms) {
        *p -= ref_val;
    }
    for a in &mut alpha {
        *a += ref_val;
    }

    Ok((alpha, psi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demean_vector_convergence_failure() {
        let mut vec = vec![1.0, 2.0, 3.0, 4.0];
        let worker_indices = vec![0, 1, 0, 1];
        let firm_indices = vec![0, 0, 1, 1];
        let n_workers = 2;
        let n_firms = 2;
        // Require extremely low tolerance but give 0 iterations so it fails immediately.
        let result = demean_vector(
            &mut vec,
            &worker_indices,
            &firm_indices,
            n_workers,
            n_firms,
            1e-10,
            0,
        );

        assert!(result.is_err());
        match result {
            Err(AkmError::ConvergenceFailed(msg)) => {
                assert!(msg.contains("demean_vector failed to converge"));
            }
            _ => panic!("Expected ConvergenceFailed error"),
        }
    }

    #[test]
    fn test_recover_fe_convergence_failure() {
        let r = vec![1.0, 2.0, 3.0, 4.0];
        let worker_indices = vec![0, 1, 0, 1];
        let firm_indices = vec![0, 0, 1, 1];
        let n_workers = 2;
        let n_firms = 2;
        let result = recover_fe(
            &r,
            &worker_indices,
            &firm_indices,
            n_workers,
            n_firms,
            1e-10,
            0,
        );

        assert!(result.is_err());
        match result {
            Err(AkmError::ConvergenceFailed(msg)) => {
                assert!(msg.contains("recover_fe failed to converge"));
            }
            _ => panic!("Expected ConvergenceFailed error"),
        }
    }
}
