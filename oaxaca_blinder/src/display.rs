#[cfg(feature = "display")]
use comfy_table::{Cell, Table};

use crate::types::OaxacaResults;

#[cfg(feature = "display")]
impl OaxacaResults {
    /// Prints a formatted summary of the decomposition results to the console.
    pub fn summary(&self) {
        print!("{}", self.format_summary());
    }

    /// Formats a summary of the decomposition results into a String.
    pub fn format_summary(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        let _ = writeln!(out, "Oaxaca-Blinder Decomposition Results");
        let _ = writeln!(out, "========================================");
        let _ = writeln!(out, "Group A (Advantaged): {} observations", self.n_a);
        let _ = writeln!(out, "Group B (Reference):  {} observations", self.n_b);
        let _ = writeln!(out, "Total Gap: {:.4}", self.total_gap);
        let _ = writeln!(out);

        let mut two_fold_table = Table::new();
        two_fold_table.set_header(vec![
            "Component",
            "Estimate",
            "Std. Err.",
            "p-value",
            "95% CI",
        ]);
        for component in self.two_fold.aggregate() {
            let ci = format!("[{:.3}, {:.3}]", component.ci_lower(), component.ci_upper());
            two_fold_table.add_row(vec![
                Cell::new(component.name()),
                Cell::new(format!("{:.4}", component.estimate())),
                Cell::new(format!("{:.4}", component.std_err())),
                Cell::new(format!("{:.4}", component.p_value())),
                Cell::new(ci),
            ]);
        }
        let _ = writeln!(out, "Two-Fold Decomposition");
        let _ = writeln!(out, "{}", two_fold_table);

        let mut explained_table = Table::new();
        explained_table.set_header(vec![
            "Variable",
            "Contribution",
            "Std. Err.",
            "p-value",
            "95% CI",
        ]);
        for component in self.two_fold.detailed_explained() {
            let ci = format!("[{:.3}, {:.3}]", component.ci_lower(), component.ci_upper());
            explained_table.add_row(vec![
                Cell::new(component.name()),
                Cell::new(format!("{:.4}", component.estimate())),
                Cell::new(format!("{:.4}", component.std_err())),
                Cell::new(format!("{:.4}", component.p_value())),
                Cell::new(ci),
            ]);
        }
        let _ = writeln!(out, "\nDetailed Decomposition (Explained)");
        let _ = writeln!(out, "{}", explained_table);

        let mut unexplained_table = Table::new();
        unexplained_table.set_header(vec![
            "Variable",
            "Contribution",
            "Std. Err.",
            "p-value",
            "95% CI",
        ]);
        for component in self.two_fold.detailed_unexplained() {
            let ci = format!("[{:.3}, {:.3}]", component.ci_lower(), component.ci_upper());
            unexplained_table.add_row(vec![
                Cell::new(component.name()),
                Cell::new(format!("{:.4}", component.estimate())),
                Cell::new(format!("{:.4}", component.std_err())),
                Cell::new(format!("{:.4}", component.p_value())),
                Cell::new(ci),
            ]);
        }
        let _ = writeln!(out, "\nDetailed Decomposition (Unexplained)");
        let _ = writeln!(out, "{}", unexplained_table);

        out
    }
}

impl OaxacaResults {
    /// Exports the results to a LaTeX table fragment.
    pub fn to_latex(&self) -> String {
        let mut latex = String::new();
        latex.push_str("\\begin{table}[ht]\n");
        latex.push_str("\\centering\n");
        latex.push_str("\\begin{tabular}{lcccc}\n");
        latex.push_str("\\hline\n");
        latex.push_str("Component & Estimate & Std. Err. & p-value & 95\\% CI \\\\\n");
        latex.push_str("\\hline\n");
        latex.push_str("\\multicolumn{5}{l}{\\textit{Two-Fold Decomposition}} \\\\\n");

        for component in self.two_fold.aggregate() {
            latex.push_str(&format!(
                "{} & {:.4} & {:.4} & {:.4} & [{:.3}, {:.3}] \\\\\n",
                component.name(),
                component.estimate(),
                component.std_err(),
                component.p_value(),
                component.ci_lower(),
                component.ci_upper()
            ));
        }
        latex.push_str("\\hline\n");
        latex.push_str("\\end{tabular}\n");
        latex.push_str("\\caption{Oaxaca-Blinder Decomposition Results}\n");
        latex.push_str("\\label{tab:oaxaca_results}\n");
        latex.push_str("\\end{table}\n");
        latex
    }

    /// Exports the results to a Markdown table.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("### Oaxaca-Blinder Decomposition Results\n\n");
        md.push_str("| Component | Estimate | Std. Err. | p-value | 95% CI |\n");
        md.push_str("|---|---|---|---|---|\n");

        for component in self.two_fold.aggregate() {
            md.push_str(&format!(
                "| {} | {:.4} | {:.4} | {:.4} | [{:.3}, {:.3}] |\n",
                component.name(),
                component.estimate(),
                component.std_err(),
                component.p_value(),
                component.ci_lower(),
                component.ci_upper()
            ));
        }
        md
    }

    /// Exports the results to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(all(test, feature = "display"))]
mod tests {
    use super::*;
    use crate::rng::RunMetadata;
    use crate::types::{ComponentResult, DecompositionDetail, TwoFoldResults};
    use nalgebra::DVector;

    fn mock_results() -> OaxacaResults {
        let comp = |name: &str, est: f64, se: f64, p: f64| ComponentResult {
            name: name.to_string(),
            estimate: est,
            std_err: se,
            t_stat: if se.abs() > 1e-9 { est / se } else { 0.0 },
            p_value: p,
            ci_lower: est - 1.96 * se,
            ci_upper: est + 1.96 * se,
        };

        OaxacaResults {
            total_gap: 0.5,
            two_fold: TwoFoldResults {
                aggregate: vec![
                    comp("explained", 0.3, 0.05, 0.001),
                    comp("unexplained", 0.2, 0.04, 0.01),
                ],
                detailed_explained: vec![
                    comp("education", 0.2, 0.03, 0.005),
                    comp("experience", 0.1, 0.02, 0.02),
                ],
                detailed_unexplained: vec![
                    comp("education", 0.05, 0.01, 0.05),
                    comp("intercept", 0.15, 0.03, 0.01),
                ],
                detailed_selection: vec![],
            },
            three_fold: DecompositionDetail {
                aggregate: vec![],
                detailed: vec![],
            },
            n_a: 100,
            n_b: 150,
            residuals: vec![],
            xa_mean: DVector::zeros(0),
            xb_mean: DVector::zeros(0),
            beta_star: DVector::zeros(0),
            run_metadata: RunMetadata::new(42, 100, 100, 0),
        }
    }

    #[test]
    fn test_format_summary_output() {
        let results = mock_results();
        let summary = results.format_summary();

        // Check headers & observation counts
        assert!(summary.contains("Oaxaca-Blinder Decomposition Results"));
        assert!(summary.contains("========================================"));
        assert!(summary.contains("Group A (Advantaged): 100 observations"));
        assert!(summary.contains("Group B (Reference):  150 observations"));
        assert!(summary.contains("Total Gap: 0.5000"));

        // Check section titles
        assert!(summary.contains("Two-Fold Decomposition"));
        assert!(summary.contains("Detailed Decomposition (Explained)"));
        assert!(summary.contains("Detailed Decomposition (Unexplained)"));

        // Check component values and formatting
        assert!(summary.contains("explained"));
        assert!(summary.contains("0.3000"));
        assert!(summary.contains("0.0500"));
        assert!(summary.contains("0.0010"));
        assert!(summary.contains("[0.202, 0.398]"));

        assert!(summary.contains("unexplained"));
        assert!(summary.contains("0.2000"));

        assert!(summary.contains("education"));
        assert!(summary.contains("experience"));
        assert!(summary.contains("intercept"));
    }

    #[test]
    fn test_summary_executes_without_panic() {
        let results = mock_results();
        results.summary();
    }
}
