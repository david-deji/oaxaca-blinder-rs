use clap::{CommandFactory, Parser, Subcommand};
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

use std::error::Error;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    run_args: RunArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run an analysis and print results to the console or export to JSON/Markdown
    #[clap(name = "run")]
    Run(Box<RunArgs>),
    /// Generate a static HTML report from an analysis
    Report(ReportArgs),
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum AnalysisType {
    Mean,
    Quantile,
    Akm,
    Match,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum ReferenceType {
    GroupA,
    GroupB,
    Pooled,
    Weighted,
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Path to the input CSV data file
    #[arg(short, long)]
    data: PathBuf,

    /// The name of the column that contains the outcome variable
    #[arg(long)]
    outcome: String,

    /// The name of the column that divides the data into two distinct groups
    #[arg(long)]
    group: String,

    /// The specific value within the group column that identifies the reference group
    #[arg(long)]
    reference: String,

    /// A comma-separated string of column names to be used as numerical predictor variables
    #[arg(long, value_delimiter = ',')]
    predictors: Vec<String>,

    /// A comma-separated string of column names to be treated as categorical variables
    #[arg(long, value_delimiter = ',')]
    categorical: Option<Vec<String>>,

    /// The type of analysis to perform
    #[arg(long, default_value = "mean", value_enum)]
    analysis_type: AnalysisType,

    /// Specifies the reference coefficients for the two-fold decomposition (for mean analysis)
    #[arg(long, default_value = "group-b", value_enum)]
    ref_coeffs: ReferenceType,

    /// A comma-separated string of quantiles to analyze (for quantile analysis)
    #[arg(long, value_delimiter = ',')]
    quantiles: Option<Vec<f64>>,

    /// The number of bootstrap replications for calculating standard errors
    #[arg(long, default_value_t = 50)]
    bootstrap_reps: usize,

    /// The number of simulations for the Machado-Mata algorithm (for quantile analysis)
    #[arg(long, default_value_t = 200)]
    simulations: usize,

    /// R-style formula for the model (e.g., "wage ~ education + experience + C(sector)")
    #[arg(long)]
    formula: Option<String>,

    /// Column name for sample weights (for WLS)
    #[arg(long)]
    weights: Option<String>,

    /// Outcome variable for the selection equation (Heckman correction)
    #[arg(long)]
    selection_outcome: Option<String>,

    /// Predictors for the selection equation (Heckman correction)
    #[arg(long, value_delimiter = ',')]
    selection_predictors: Option<Vec<String>>,

    /// Path to export results as JSON
    #[arg(long)]
    output_json: Option<PathBuf>,

    /// Path to export results as Markdown
    #[arg(long)]
    output_markdown: Option<PathBuf>,

    /// Worker ID column for AKM analysis (Abowd-Kramarz-Margolis model)
    #[arg(long)]
    worker_id: Option<String>,

    /// Firm ID column for AKM analysis (Abowd-Kramarz-Margolis model)
    #[arg(long)]
    firm_id: Option<String>,

    /// Number of neighbors for matching
    #[arg(long, default_value_t = 1)]
    k_neighbors: usize,

    /// Matching method [choices: euclidean, mahalanobis, psm]
    #[arg(long, default_value = "euclidean")]
    matching_method: String,
}

#[derive(Parser, Debug)]
struct ReportArgs {
    /// Path to the input CSV data file
    #[arg(short, long)]
    data: PathBuf,

    /// The name of the column that contains the outcome variable
    #[arg(long)]
    outcome: String,

    /// The name of the column that divides the data into two distinct groups
    #[arg(long)]
    group: String,

    /// The specific value within the group column that identifies the reference group
    #[arg(long)]
    reference: String,

    /// A comma-separated string of column names to be used as numerical predictor variables
    #[arg(long, value_delimiter = ',')]
    predictors: Vec<String>,

    /// A comma-separated string of column names to be treated as categorical variables
    #[arg(long, value_delimiter = ',')]
    categorical: Option<Vec<String>>,

    /// Path to save the output HTML file
    #[arg(short, long)]
    output: PathBuf,
}

fn run_analysis(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let df = LazyCsvReader::new(&args.data)
        .with_has_header(true)
        .finish()?
        .collect()?;

    match args.analysis_type {
        AnalysisType::Mean => run_mean_analysis(&args, df),
        AnalysisType::Quantile => run_quantile_analysis(&args, df),
        AnalysisType::Akm => run_akm_analysis(&args, df),
        AnalysisType::Match => run_matching_analysis(&args, df),
    }
}

fn run_mean_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let reference_coeffs = match args.ref_coeffs {
        ReferenceType::GroupA => ReferenceCoefficients::GroupA,
        ReferenceType::GroupB => ReferenceCoefficients::GroupB,
        ReferenceType::Pooled => ReferenceCoefficients::Pooled,
        ReferenceType::Weighted => ReferenceCoefficients::Weighted,
    };

    let mut builder = if let Some(formula) = &args.formula {
        OaxacaBuilder::from_formula(df, formula, &args.group, &args.reference)?
    } else {
        let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
        let categorical_predictors: Vec<&str> = args
            .categorical
            .as_ref()
            .map(|v| v.iter().map(AsRef::as_ref).collect())
            .unwrap_or_default();
        let mut b = OaxacaBuilder::new(df, &args.outcome, &args.group, &args.reference);
        b.predictors(predictors.iter().copied())
            .categorical_predictors(categorical_predictors.iter().copied());
        b
    };

    builder
        .bootstrap_reps(args.bootstrap_reps)
        .reference_coefficients(reference_coeffs);

    if let Some(weights) = &args.weights {
        builder.weights(weights);
    }

    if let Some(sel_outcome) = &args.selection_outcome {
        if let Some(sel_predictors) = &args.selection_predictors {
            let sel_preds_refs: Vec<&str> = sel_predictors.iter().map(AsRef::as_ref).collect();
            builder.heckman_selection(sel_outcome, sel_preds_refs.iter().copied());
        } else {
            return Err(
                "Selection predictors must be provided if selection outcome is specified".into(),
            );
        }
    }

    let results = builder.run()?;
    results.summary();

    if let Some(path) = &args.output_json {
        let json = results
            .to_json()
            .map_err(|e| format!("Failed to serialize to JSON: {}", e))?;
        std::fs::write(path, json)?;
    }

    if let Some(path) = &args.output_markdown {
        let md = results.to_markdown();
        std::fs::write(path, md)?;
    }
    Ok(())
}

fn run_quantile_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn Error>> {
    // RIF-regression quantile decomposition (0014-MERIDIAN ruling a-1): the CLI uses the SAME
    // OaxacaBuilder::decompose_quantile path as the WASM/MCP surface — one coherent method
    // everywhere, no CLI-vs-browser divergence (AC-13). QuantileDecompositionBuilder (MM
    // simulation) stays exported for direct-API consumers but is no longer the CLI default;
    // `--simulations` is therefore inert on this path (kept for backward CLI compatibility).
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
    let quantiles = args
        .quantiles
        .clone()
        .unwrap_or_else(|| vec![0.1, 0.25, 0.5, 0.75, 0.9]);
    let categorical_predictors: Vec<&str> = args
        .categorical
        .as_ref()
        .map(|v| v.iter().map(AsRef::as_ref).collect())
        .unwrap_or_default();
    let reference_coeffs = match args.ref_coeffs {
        ReferenceType::GroupA => ReferenceCoefficients::GroupA,
        ReferenceType::GroupB => ReferenceCoefficients::GroupB,
        ReferenceType::Pooled => ReferenceCoefficients::Pooled,
        ReferenceType::Weighted => ReferenceCoefficients::Weighted,
    };

    for &q in &quantiles {
        // Arc/COW-cheap clone (stage-2 Finding 1); each τ gets a fresh builder.
        let mut builder =
            OaxacaBuilder::new(df.clone(), &args.outcome, &args.group, &args.reference);
        builder
            .predictors(predictors.iter().copied())
            .categorical_predictors(categorical_predictors.iter().copied())
            .bootstrap_reps(args.bootstrap_reps)
            .reference_coefficients(reference_coeffs);
        if let Some(weights) = &args.weights {
            builder.weights(weights);
        }

        let results = builder.decompose_quantile(q)?;
        println!("\n=== Quantile τ = {:.2} (RIF-regression) ===", q);
        results.summary();

        // AC-13 CLI<->library/WASM parity (0014-MERIDIAN): `OaxacaResults::to_json()` already
        // exists and is wired for the mean path (see `run_mean_analysis` above); wiring the same
        // call here gives the quantile path a machine-parseable output so its numeric parity with
        // the library/WASM path (which returns this same `OaxacaResults`) can be asserted in a
        // test instead of only by code-path inspection. Multiple `--quantiles` values would
        // otherwise collide on one output file, so each additional tau gets a `.qX.XX` suffix
        // inserted before the extension; the common single-quantile case writes exactly to the
        // path given.
        if let Some(path) = &args.output_json {
            let out_path = if quantiles.len() > 1 {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
                path.with_file_name(format!("{}.q{:.2}.{}", stem, q, ext))
            } else {
                path.clone()
            };
            let json = results
                .to_json()
                .map_err(|e| format!("Failed to serialize to JSON: {}", e))?;
            std::fs::write(&out_path, json)?;
        }
    }
    Ok(())
}

fn run_akm_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn Error>> {
    use oaxaca_blinder::AkmBuilder;
    let worker_col = args
        .worker_id
        .as_ref()
        .ok_or("Worker ID is required for AKM analysis")?;
    let firm_col = args
        .firm_id
        .as_ref()
        .ok_or("Firm ID is required for AKM analysis")?;
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();

    let mut builder = AkmBuilder::new(df, &args.outcome, worker_col, firm_col);
    builder.controls(predictors.iter().copied());
    let results = builder
        .run()
        .map_err(|e| format!("AKM estimation failed: {:?}", e))?;

    println!("AKM Estimation Results");
    println!("Method: Alternating Projections (MAP) on Largest Connected Set");
    println!("----------------------");
    println!("R-squared: {:.4}", results.r2);
    println!("Beta Coefficients:");
    for (i, name) in args.predictors.iter().enumerate() {
        if i < results.beta.len() {
            println!("  {}: {:.4}", name, results.beta[i]);
        }
    }
    Ok(())
}

fn run_matching_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn Error>> {
    use oaxaca_blinder::MatchingEngine;
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
    let engine = MatchingEngine::new(df, &args.group, &args.outcome, predictors.iter().copied());

    let weights = if args.matching_method == "psm" {
        engine
            .match_psm(args.k_neighbors)
            .map_err(|e| format!("Matching failed: {:?}", e))?
    } else {
        let use_mahalanobis = args.matching_method == "mahalanobis";
        engine
            .run_matching(args.k_neighbors, use_mahalanobis)
            .map_err(|e| format!("Matching failed: {:?}", e))?
    };

    if let Some(path) = &args.output_json {
        let json = serde_json::to_string(&weights)?;
        std::fs::write(path, json)?;
    } else {
        println!("Matching completed. Generated {} weights.", weights.len());
        println!(
            "First 10 weights: {:?}",
            weights.iter().take(10).collect::<Vec<_>>()
        );
    }
    Ok(())
}

use askama::Template;
use oaxaca_blinder::ComponentResult;

#[derive(Template)]
#[template(path = "report.html")]
struct ReportTemplate {
    n_a: usize,
    n_b: usize,
    total_gap: f64,
    two_fold: Vec<ComponentResult>,
    explained: Vec<ComponentResult>,
    unexplained: Vec<ComponentResult>,
}

fn run_report(args: ReportArgs) -> Result<(), Box<dyn Error>> {
    let df = LazyCsvReader::new(&args.data)
        .with_has_header(true)
        .finish()?
        .collect()?;
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
    let categorical_predictors: Vec<&str> = args
        .categorical
        .as_ref()
        .map(|v| v.iter().map(AsRef::as_ref).collect())
        .unwrap_or_default();
    let results = OaxacaBuilder::new(df, &args.outcome, &args.group, &args.reference)
        .predictors(predictors.iter().copied())
        .categorical_predictors(categorical_predictors.iter().copied())
        .run()?;

    let two_fold = results.two_fold().aggregate().clone();
    let explained = results.two_fold().detailed_explained().clone();
    let unexplained = results.two_fold().detailed_unexplained().clone();

    let template = ReportTemplate {
        n_a: *results.n_a(),
        n_b: *results.n_b(),
        total_gap: *results.total_gap(),
        two_fold,
        explained,
        unexplained,
    };

    let html = template.render()?;
    std::fs::write(&args.output, html)?;
    println!(
        "Report successfully generated at: {}",
        args.output.display()
    );
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Commands::Run(args)) => run_analysis(*args),
        Some(Commands::Report(args)) => run_report(args),
        None => run_analysis(cli.run_args),
    };
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        let mut cmd = Cli::command();
        let _ = cmd.print_help();
        std::process::exit(1);
    }
}
