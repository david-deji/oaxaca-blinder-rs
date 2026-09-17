use oaxaca_blinder::OaxacaBuilder;
use polars::prelude::*;
use serde_json::Value;

#[test]
fn test_export_methods() -> Result<(), Box<dyn std::error::Error>> {
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
    )?;

    let results = OaxacaBuilder::new(df, "wage", "gender", "F")
        .predictors(vec!["education"])
        .bootstrap_reps(10) // Low reps for speed
        .run()?;

    // Test summary formatting
    let summary = results.format_summary();
    assert!(summary.contains("Oaxaca-Blinder Decomposition Results"));
    assert!(summary.contains("Group A (Advantaged): 5 observations"));
    assert!(summary.contains("Group B (Reference):  5 observations"));
    assert!(summary.contains("Two-Fold Decomposition"));
    assert!(summary.contains("Detailed Decomposition (Explained)"));
    assert!(summary.contains("Detailed Decomposition (Unexplained)"));

    // Test LaTeX
    let latex = results.to_latex();
    assert!(latex.contains("\\begin{table}"));
    assert!(latex.contains("explained"));
    assert!(latex.contains("unexplained"));

    // Test Markdown
    let markdown = results.to_markdown();
    assert!(markdown.contains("| Component | Estimate |"));
    assert!(markdown.contains("| explained |"));

    // Test JSON
    let json_str = results.to_json()?;
    let json: Value = serde_json::from_str(&json_str)?;

    assert!(json.get("total_gap").is_some());
    assert!(json.get("two_fold").is_some());
    assert!(json["two_fold"].get("aggregate").is_some());

    Ok(())
}
