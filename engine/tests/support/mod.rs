//! Shared test support for 0118-MERIDIAN: Fixture F and its independent oracles.
//!
//! WHY THIS FILE EXISTS. The pre-0118 engine paired each employee's name with the NEXT
//! employee's fair wage whenever one cell was blank, and its own tests pinned that output as
//! expected. Any test that compares engine output with other engine output cannot see a shift
//! of dollars between employees. Every expected number a test takes from this module is
//! therefore computed HERE, from the fixture's own cells and the fixture's own formula (or, for
//! the noisy variant, from an ordinary-least-squares fit written below in plain `std`, sharing
//! no code with the engine). Nothing in this file calls the engine.
//!
//! Included by integration tests with `mod support;` and by the `oaxaca_blinder` crate's tests
//! with `#[path = "../../engine/tests/support/mod.rs"] mod support;`, so it depends on `std`
//! only.
//!
//! # Fixture F
//!
//! 100 employees, `Name` unique. Columns of the committed CSV
//! (`engine/tests/fixtures/0118-fixture-f.csv`): `Name, Salary, Gender, Experience, Level`.
//!
//! * Reference group `Male` (60 rows) is paid exactly
//!   `Salary = 30000 + 1000*Experience + 4000*Level` (the NOISY variant adds a known residual
//!   of up to +/- 240).
//! * Target group `Female` (40 rows) is paid that formula wage minus a distinct, known,
//!   non-monotone shortfall (`shortfall`), negative for nine employees who are overpaid.
//! * Groups are interleaved (target at raw rows 5m+1 and 5m+3), so raw ordinal, group-local
//!   ordinal and analysed ordinal all differ once a cell is blank.
//! * The last raw row (99) is a reference employee. The last target employee (raw row 98) is
//!   underpaid by 2650.
//!
//! Optional columns, appended by the builder methods: `Dept` (3 levels, no effect on pay, for
//! categorical-predictor variants), `Weight` (for weights variants) and `EmployeeID` (first
//! column, qualifies as a row-key column).
//!
//! Blank variants are made with [`FixtureF::blank`]; a physically blank line with
//! [`FixtureF::blank_line_before`].

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

pub const REFERENCE: &str = "Male";
pub const TARGET: &str = "Female";
pub const N_ROWS: usize = 100;
/// z for a two-sided 95% interval, `Normal(0,1).inverse_cdf(0.975)`.
pub const Z_95: f64 = 1.959_963_984_540_054;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Col {
    Name,
    Salary,
    Gender,
    Experience,
    Level,
    Dept,
    Weight,
    EmployeeId,
}

impl Col {
    pub fn header(self) -> &'static str {
        match self {
            Col::Name => "Name",
            Col::Salary => "Salary",
            Col::Gender => "Gender",
            Col::Experience => "Experience",
            Col::Level => "Level",
            Col::Dept => "Dept",
            Col::Weight => "Weight",
            Col::EmployeeId => "EmployeeID",
        }
    }

    /// The reason string the engine reports when a blank in this column excludes a row.
    pub fn reason(self) -> &'static str {
        match self {
            Col::Salary => "outcome",
            Col::Gender => "groupValue",
            Col::Experience | Col::Level => "numericPredictor",
            Col::Dept => "categoricalPredictor",
            Col::Weight => "weights",
            Col::Name | Col::EmployeeId => panic!("{:?} is never a model column", self),
        }
    }
}

const FIRST: [&str; 10] = [
    "Alice",
    "Bruno",
    "Chloe",
    "Daniel",
    "Elise",
    "Felix",
    "Gabrielle",
    "Hugo",
    "Ines",
    "Julien",
];
const LAST: [&str; 10] = [
    "Tremblay",
    "Gagnon",
    "Roy",
    "Cote",
    "Bouchard",
    "Gauthier",
    "Morin",
    "Lavoie",
    "Fortin",
    "Pelletier",
];
const DEPTS: [&str; 3] = ["Ops", "Sales", "Tech"];

#[derive(Clone, Debug)]
pub struct FixtureRow {
    pub name: String,
    pub employee_id: String,
    pub gender: &'static str,
    pub experience: i64,
    pub level: i64,
    pub dept: &'static str,
    pub weight: f64,
    pub salary: i64,
}

/// `true` for the 40 target (Female) rows.
pub fn is_target_row(i: usize) -> bool {
    matches!(i % 5, 1 | 3)
}

/// The reference pay formula.
pub fn formula(experience: i64, level: i64) -> f64 {
    30000.0 + 1000.0 * experience as f64 + 4000.0 * level as f64
}

/// How far below the formula wage the `k`-th target employee (k = 0..40, in raw order) is paid.
/// Distinct across k (17 is invertible mod 41), not monotone in k, negative (overpaid) for nine
/// employees, never zero, and positive (2150) for the last target employee.
pub fn shortfall(k: usize) -> i64 {
    (((k as i64) * 17 + 28) % 41) * 100 - 850
}

fn noise(i: usize) -> i64 {
    (((i as i64) * 29) % 13 - 6) * 40
}

#[derive(Clone, Debug)]
pub struct FixtureF {
    pub rows: Vec<FixtureRow>,
    noisy: bool,
    with_dept: bool,
    with_weight: bool,
    with_employee_id: bool,
    blanks: BTreeSet<(usize, Col)>,
    texts: BTreeMap<(usize, Col), String>,
    blank_line_before: Option<usize>,
}

impl FixtureF {
    fn build(noisy: bool) -> Self {
        let mut rows = Vec::with_capacity(N_ROWS);
        let mut target_k = 0usize;
        for i in 0..N_ROWS {
            let experience = ((i * 7 + 3) % 23) as i64;
            let level = 1 + ((i * 3 + i / 4) % 6) as i64;
            let wage = formula(experience, level) as i64;
            let target = is_target_row(i);
            let salary = if target {
                let s = wage - shortfall(target_k);
                target_k += 1;
                s
            } else if noisy {
                wage + noise(i)
            } else {
                wage
            };
            rows.push(FixtureRow {
                name: format!("{} {}", FIRST[i % 10], LAST[i / 10]),
                employee_id: format!("E{:04}", 1000 + i * 7),
                gender: if target { TARGET } else { REFERENCE },
                experience,
                level,
                dept: DEPTS[(i * 7 / 3 + i / 2) % 3],
                weight: 1.0 + (i % 4) as f64 * 0.5,
                salary,
            });
        }
        Self {
            rows,
            noisy,
            with_dept: false,
            with_weight: false,
            with_employee_id: false,
            blanks: BTreeSet::new(),
            texts: BTreeMap::new(),
            blank_line_before: None,
        }
    }

    /// Reference rows paid exactly the formula.
    pub fn exact() -> Self {
        Self::build(false)
    }

    /// Reference rows paid the formula plus a known residual.
    pub fn noisy() -> Self {
        Self::build(true)
    }

    pub fn with_dept(mut self) -> Self {
        self.with_dept = true;
        self
    }

    pub fn with_weight(mut self) -> Self {
        self.with_weight = true;
        self
    }

    pub fn with_employee_id(mut self) -> Self {
        self.with_employee_id = true;
        self
    }

    /// Leaves `col` blank on raw row `row`. The column must be emitted (e.g. call
    /// `with_dept()` before blanking `Col::Dept`).
    pub fn blank(mut self, row: usize, col: Col) -> Self {
        self.blanks.insert((row, col));
        self
    }

    /// Replaces the cell at (`row`, `col`) with literal text, e.g. `"N/A"` in a numeric column.
    pub fn cell_text(mut self, row: usize, col: Col, text: &str) -> Self {
        self.texts.insert((row, col), text.to_string());
        self
    }

    /// Inserts a physically blank line immediately before data row `row`.
    pub fn blank_line_before(mut self, row: usize) -> Self {
        self.blank_line_before = Some(row);
        self
    }

    pub fn is_noisy(&self) -> bool {
        self.noisy
    }

    fn columns(&self) -> Vec<Col> {
        let mut cols = Vec::new();
        if self.with_employee_id {
            cols.push(Col::EmployeeId);
        }
        cols.extend([
            Col::Name,
            Col::Salary,
            Col::Gender,
            Col::Experience,
            Col::Level,
        ]);
        if self.with_dept {
            cols.push(Col::Dept);
        }
        if self.with_weight {
            cols.push(Col::Weight);
        }
        cols
    }

    fn cell(&self, row: usize, col: Col) -> String {
        if self.blanks.contains(&(row, col)) {
            return String::new();
        }
        if let Some(text) = self.texts.get(&(row, col)) {
            return text.clone();
        }
        let r = &self.rows[row];
        match col {
            Col::Name => r.name.clone(),
            Col::Salary => r.salary.to_string(),
            Col::Gender => r.gender.to_string(),
            Col::Experience => r.experience.to_string(),
            Col::Level => r.level.to_string(),
            Col::Dept => r.dept.to_string(),
            Col::Weight => format!("{:.1}", r.weight),
            Col::EmployeeId => r.employee_id.clone(),
        }
    }

    pub fn csv(&self) -> String {
        let cols = self.columns();
        let mut out = cols
            .iter()
            .map(|c| c.header())
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for i in 0..self.rows.len() {
            if self.blank_line_before == Some(i) {
                out.push('\n');
            }
            out.push_str(
                &cols
                    .iter()
                    .map(|&c| self.cell(i, c))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            out.push('\n');
        }
        out
    }

    pub fn csv_bytes(&self) -> Vec<u8> {
        self.csv().into_bytes()
    }

    // ---- oracle: row sets -------------------------------------------------------------

    /// Raw ordinals of every reference row (blank or not), ascending.
    pub fn reference_ordinals(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| !is_target_row(i))
            .collect()
    }

    /// Raw ordinals of every target row (blank or not), ascending.
    pub fn target_ordinals(&self) -> Vec<usize> {
        (0..self.rows.len()).filter(|&i| is_target_row(i)).collect()
    }

    /// The columns the model reads for a request: outcome, group, both numeric predictors, and
    /// optionally the categorical predictor and the weights column.
    pub fn model_cols(categorical: bool, weighted: bool) -> Vec<Col> {
        let mut cols = vec![Col::Salary, Col::Gender, Col::Experience, Col::Level];
        if categorical {
            cols.push(Col::Dept);
        }
        if weighted {
            cols.push(Col::Weight);
        }
        cols
    }

    /// The blank model columns on `row`, in `model` order.
    pub fn blank_model_cols(&self, row: usize, model: &[Col]) -> Vec<Col> {
        model
            .iter()
            .copied()
            .filter(|&c| self.blanks.contains(&(row, c)))
            .collect()
    }

    /// Complete-case reference rows (raw ordinals) for `model`.
    pub fn analysed_reference(&self, model: &[Col]) -> Vec<usize> {
        self.reference_ordinals()
            .into_iter()
            .filter(|&i| self.blank_model_cols(i, model).is_empty() && !self.gender_blank(i))
            .collect()
    }

    /// Complete-case target rows (raw ordinals) for `model`.
    pub fn analysed_target(&self, model: &[Col]) -> Vec<usize> {
        self.target_ordinals()
            .into_iter()
            .filter(|&i| self.blank_model_cols(i, model).is_empty() && !self.gender_blank(i))
            .collect()
    }

    fn gender_blank(&self, row: usize) -> bool {
        self.blanks.contains(&(row, Col::Gender))
    }

    /// Every excluded row (raw ordinal) with its blank model columns, ascending. A row whose
    /// group value is blank is excluded too, and is neither reference nor target.
    pub fn excluded(&self, model: &[Col]) -> Vec<(usize, Vec<Col>)> {
        (0..self.rows.len())
            .filter_map(|i| {
                let cols = self.blank_model_cols(i, model);
                if cols.is_empty() {
                    None
                } else {
                    Some((i, cols))
                }
            })
            .collect()
    }

    // ---- oracle: cells ----------------------------------------------------------------

    /// The Salary cell of raw row `i` as the CSV carries it (`None` when blank).
    pub fn salary_cell(&self, i: usize) -> Option<f64> {
        if self.blanks.contains(&(i, Col::Salary)) {
            None
        } else {
            Some(self.rows[i].salary as f64)
        }
    }

    /// Fair wage by the reference formula, from the row's own Experience and Level. Valid as the
    /// engine's expected fair wage only for the EXACT variant, where the reference fit is the
    /// formula itself.
    pub fn formula_wage(&self, i: usize) -> f64 {
        formula(self.rows[i].experience, self.rows[i].level)
    }

    pub fn name(&self, i: usize) -> &str {
        &self.rows[i].name
    }

    pub fn features(&self, i: usize) -> Vec<f64> {
        vec![
            1.0,
            self.rows[i].experience as f64,
            self.rows[i].level as f64,
        ]
    }

    /// Oracle fit of the reference regression (intercept, Experience, Level) over `rows`
    /// (raw ordinals), from the cells.
    pub fn reference_fit(&self, rows: &[usize]) -> OlsFit {
        let x: Vec<Vec<f64>> = rows.iter().map(|&i| self.features(i)).collect();
        let y: Vec<f64> = rows.iter().map(|&i| self.rows[i].salary as f64).collect();
        ols(&x, &y)
    }

    /// Oracle fair wage of row `i` under a fit from [`reference_fit`](Self::reference_fit).
    pub fn fair_wage(&self, fit: &OlsFit, i: usize) -> f64 {
        dot(&fit.beta, &self.features(i))
    }

    /// Oracle 95% prediction interval `(lower, upper)` for row `i`, the same quantity the
    /// engine's `calculate_interval` returns: `fair -/+ z * sqrt(sigma2 * (1 + x' (X'X)^-1 x))`.
    pub fn interval(&self, fit: &OlsFit, i: usize) -> (f64, f64) {
        let fair = self.fair_wage(fit, i);
        if fit.sigma2 <= 1e-9 {
            return (fair, fair);
        }
        let x = self.features(i);
        let h = quad_form(&fit.xtx_inv, &x);
        let margin = Z_95 * (fit.sigma2 * (1.0 + h)).sqrt();
        (fair - margin, fair + margin)
    }
}

// ---- ordinary least squares, std only ---------------------------------------------------

#[derive(Clone, Debug)]
pub struct OlsFit {
    pub beta: Vec<f64>,
    pub xtx_inv: Vec<Vec<f64>>,
    pub sigma2: f64,
    pub n: usize,
    pub p: usize,
}

pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn quad_form(m: &[Vec<f64>], x: &[f64]) -> f64 {
    let mut total = 0.0;
    for (i, row) in m.iter().enumerate() {
        total += x[i] * dot(row, x);
    }
    total
}

/// Gauss-Jordan inverse with partial pivoting.
pub fn invert(mut a: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    let n = a.len();
    let mut inv: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&r1, &r2| a[r1][col].abs().partial_cmp(&a[r2][col].abs()).unwrap())
            .unwrap();
        assert!(a[pivot][col].abs() > 1e-12, "singular matrix in oracle OLS");
        a.swap(col, pivot);
        inv.swap(col, pivot);
        let d = a[col][col];
        for j in 0..n {
            a[col][j] /= d;
            inv[col][j] /= d;
        }
        for r in 0..n {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for j in 0..n {
                        a[r][j] -= f * a[col][j];
                        inv[r][j] -= f * inv[col][j];
                    }
                }
            }
        }
    }
    inv
}

/// OLS by the normal equations. `x` rows already carry any intercept column.
pub fn ols(x: &[Vec<f64>], y: &[f64]) -> OlsFit {
    let n = x.len();
    let p = x[0].len();
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for (row, &yi) in x.iter().zip(y) {
        for a in 0..p {
            xty[a] += row[a] * yi;
            for b in 0..p {
                xtx[a][b] += row[a] * row[b];
            }
        }
    }
    let xtx_inv = invert(xtx);
    let beta: Vec<f64> = xtx_inv.iter().map(|r| dot(r, &xty)).collect();
    let rss: f64 = x
        .iter()
        .zip(y)
        .map(|(row, &yi)| {
            let e = yi - dot(&beta, row);
            e * e
        })
        .sum();
    let dof = n as f64 - p as f64;
    let sigma2 = if dof > 0.0 { rss / dof } else { 0.0 };
    OlsFit {
        beta,
        xtx_inv,
        sigma2,
        n,
        p,
    }
}

/// t statistic of the group dummy in the pooled regression
/// `y ~ 1 + group + Experience + Level`, where `group` is 0 for reference rows and 1 for target
/// rows and `rows` lists (raw ordinal, is_target, outcome) triples, reference rows first. This
/// is the quantity `calculate_efficient_frontier` reports at each budget.
pub fn pooled_group_t(fixture: &FixtureF, rows: &[(usize, bool, f64)]) -> f64 {
    let x: Vec<Vec<f64>> = rows
        .iter()
        .map(|&(i, is_target, _)| {
            vec![
                1.0,
                if is_target { 1.0 } else { 0.0 },
                fixture.rows[i].experience as f64,
                fixture.rows[i].level as f64,
            ]
        })
        .collect();
    let y: Vec<f64> = rows.iter().map(|&(_, _, w)| w).collect();
    let fit = ols(&x, &y);
    let se = (fit.sigma2 * fit.xtx_inv[1][1]).sqrt();
    fit.beta[1] / se
}

/// Compares two floats to the cent.
pub fn same_to_the_cent(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}
