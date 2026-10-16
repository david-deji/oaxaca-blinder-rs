use clap::{Parser, Subcommand};
use oaxaca_blinder::{NormalizationConvention, OaxacaBuilder, ReferenceCoefficients, WeightsKind};
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
    #[clap(name = "run", hide = true)]
    Run(Box<RunArgs>),
    /// Generate a static HTML report from an analysis
    Report(ReportArgs),
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum AnalysisType {
    Mean,
    Quantile,
    /// Experimental: Abowd-Kramarz-Margolis worker and firm effects. Needs --worker-id and --firm-id.
    /// Hidden, and refused with a hint, in a build without the `experimental` feature.
    #[cfg_attr(not(feature = "experimental"), value(hide = true))]
    Akm,
    /// Experimental: nearest-neighbour or propensity-score matching weights.
    /// Hidden, and refused with a hint, in a build without the `experimental` feature.
    #[cfg_attr(not(feature = "experimental"), value(hide = true))]
    Match,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum ReferenceType {
    /// beta* = the compared (non-reference) group's coefficients
    GroupA,
    /// beta* = the reference group's coefficients
    GroupB,
    /// pooled regression WITH a group indicator (Jann 2008 `pooled`)
    Pooled,
    /// pooled regression WITHOUT a group indicator (Neumark 1988; Stata `omega`)
    PooledNoIndicator,
    /// sample-share-weighted average of the two groups' coefficients (Cotton 1988)
    Weighted,
}

impl ReferenceType {
    fn to_library(&self) -> ReferenceCoefficients {
        match self {
            ReferenceType::GroupA => ReferenceCoefficients::GroupA,
            ReferenceType::GroupB => ReferenceCoefficients::GroupB,
            ReferenceType::Pooled => ReferenceCoefficients::Pooled,
            ReferenceType::PooledNoIndicator => ReferenceCoefficients::PooledNoIndicator,
            ReferenceType::Weighted => ReferenceCoefficients::Weighted,
        }
    }
}

/// How categorical predictors' per-level contributions are expressed (mean and quantile runs).
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum NormalizationArg {
    /// Each level is a deviation from the pooled-sample share-weighted average of all levels,
    /// the dropped level included. The default.
    PopulationShare,
    /// Each level is a deviation from the simple average of all levels (Stata `categorical()`,
    /// R `oaxaca`, `ddecompose` `normalize_factors = TRUE`).
    EqualShare,
    /// Raw treatment coding: every level against the alphabetically first one. Per-level
    /// numbers then depend on which level that is; aggregates are the same in all three.
    None,
}

impl NormalizationArg {
    /// Applies the choice to a builder whose categorical predictors are already set.
    fn apply(self, builder: &mut OaxacaBuilder) {
        match self {
            NormalizationArg::None => {}
            NormalizationArg::PopulationShare => {
                builder
                    .normalization_convention(NormalizationConvention::PopulationShare)
                    .normalize_all_categoricals();
            }
            NormalizationArg::EqualShare => {
                builder
                    .normalization_convention(NormalizationConvention::EqualShare)
                    .normalize_all_categoricals();
            }
        }
    }
}

/// What a weights column means.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum WeightsKindArg {
    /// Whole-number replication counts (a headcount). `2` is the row twice, in the point estimates
    /// and in the bootstrap (a replicate draws sum(w) employees, so standard errors and p-values
    /// match the repeated rows); a fractional value is refused, naming the row.
    Frequency,
    /// Relative importance (FTE, design weights), rescaled to sum to the row count. The weighted
    /// quantile is `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`. Uniform weights change
    /// nothing.
    Relative,
}

impl WeightsKindArg {
    fn to_library(self) -> WeightsKind {
        match self {
            WeightsKindArg::Frequency => WeightsKind::Frequency,
            WeightsKindArg::Relative => WeightsKind::Relative,
        }
    }
}

/// Applies `--weights` and `--weights-kind` to a builder. clap guarantees they come together.
fn apply_weights(builder: &mut OaxacaBuilder, args: &RunArgs) {
    if let Some(weights) = &args.weights {
        builder.weights(weights);
        if let Some(kind) = args.weights_kind {
            builder.weights_kind(kind.to_library());
        }
    }
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Path to the input CSV data file
    #[arg(short, long)]
    data: PathBuf,

    /// The name of the column that contains the outcome variable. Not needed with --formula, which names it
    #[arg(long, required_unless_present = "formula")]
    outcome: Option<String>,

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

    /// How categorical predictors' per-level contributions are expressed
    #[arg(long, default_value = "population-share", value_enum)]
    normalization: NormalizationArg,

    /// A comma-separated string of quantiles to analyze (for quantile analysis)
    #[arg(long, value_delimiter = ',')]
    quantiles: Option<Vec<f64>>,

    /// The number of bootstrap replications for calculating standard errors. At least 41 are
    /// needed for a percentile confidence interval. The library default is the same.
    #[arg(long, default_value_t = 100)]
    bootstrap_reps: usize,

    /// Ignored. The quantile analysis is RIF regression; the option is kept so that older command lines still run
    #[arg(long, default_value_t = 200)]
    simulations: usize,

    /// R-style formula for the model (e.g., "wage ~ education + experience + C(sector)")
    #[arg(long)]
    formula: Option<String>,

    /// Column name for sample weights (for WLS). Needs --weights-kind.
    #[arg(long, requires = "weights_kind")]
    weights: Option<String>,

    /// What the weights column means: `frequency` (whole-number counts) or `relative`
    #[arg(long, value_enum, requires = "weights")]
    weights_kind: Option<WeightsKindArg>,

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

    /// Worker ID column for AKM analysis (Abowd-Kramarz-Margolis model). Experimental.
    #[cfg(feature = "experimental")]
    #[arg(long)]
    worker_id: Option<String>,

    /// Firm ID column for AKM analysis (Abowd-Kramarz-Margolis model). Experimental.
    #[cfg(feature = "experimental")]
    #[arg(long)]
    firm_id: Option<String>,

    /// Number of neighbors for matching. Experimental.
    #[cfg(feature = "experimental")]
    #[arg(long, default_value_t = 1)]
    k_neighbors: usize,

    /// Matching method [choices: euclidean, mahalanobis, psm]. Experimental.
    #[cfg(feature = "experimental")]
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

/// The smallest number of bootstrap replicates that gives a percentile confidence interval.
const MIN_PERCENTILE_CI_REPS: usize = 41;

/// Reads a CSV file. The columns named in `float_columns` (the outcome and the weights) are converted from whole
/// numbers to decimals, so a salary written `45600` is accepted. Every other column is left as read.
fn read_csv(path: &std::path::Path, float_columns: &[&str]) -> Result<DataFrame, Box<dyn Error>> {
    let mut df = LazyCsvReader::new(path)
        .with_has_header(true)
        .finish()?
        .collect()?;
    for name in float_columns {
        let converted = match df.column(name) {
            Ok(column) if column.dtype().is_integer() => column.cast(&DataType::Float64)?,
            _ => continue,
        };
        df.with_column(converted)?;
    }
    Ok(df)
}

/// The outcome column named in an R-style formula: the text left of `~`.
fn formula_outcome(formula: &str) -> Option<&str> {
    formula
        .split('~')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

/// Warns on standard error when the bootstrap is too small for a percentile interval.
fn warn_if_bootstrap_too_small(reps: usize) {
    if reps < MIN_PERCENTILE_CI_REPS {
        eprintln!(
            "Warning: {reps} bootstrap replicates were requested. A 95% interval needs at least \
             {MIN_PERCENTILE_CI_REPS}, so the intervals below print as NaN. Use --bootstrap-reps \
             {MIN_PERCENTILE_CI_REPS} or more."
        );
    }
}

impl RunArgs {
    /// The outcome column: `--outcome`, or the left side of `--formula`.
    fn outcome_name(&self) -> Result<&str, Box<dyn Error>> {
        self.outcome
            .as_deref()
            .or_else(|| self.formula.as_deref().and_then(formula_outcome))
            .ok_or_else(|| "Give the outcome column with --outcome".into())
    }
}

fn run_analysis(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut float_columns: Vec<&str> = vec![args.outcome_name()?];
    if let Some(weights) = &args.weights {
        float_columns.push(weights);
    }
    let df = read_csv(&args.data, &float_columns)?;
    warn_if_bootstrap_too_small(args.bootstrap_reps);

    match args.analysis_type {
        AnalysisType::Mean => run_mean_analysis(&args, df),
        AnalysisType::Quantile => run_quantile_analysis(&args, df),
        #[cfg(feature = "experimental")]
        AnalysisType::Akm => run_akm_analysis(&args, df),
        #[cfg(feature = "experimental")]
        AnalysisType::Match => run_matching_analysis(&args, df),
        #[cfg(not(feature = "experimental"))]
        AnalysisType::Akm | AnalysisType::Match => Err(
            "akm and match are experimental and are not in this build. Install the command line with \
             `cargo install oaxaca_blinder --features experimental`"
                .into(),
        ),
    }
}

fn run_mean_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let reference_coeffs = args.ref_coeffs.to_library();

    let mut builder = if let Some(formula) = &args.formula {
        OaxacaBuilder::from_formula(df, formula, &args.group, &args.reference)?
    } else {
        let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
        let categorical_predictors: Vec<&str> = args
            .categorical
            .as_ref()
            .map(|v| v.iter().map(AsRef::as_ref).collect())
            .unwrap_or_default();
        let mut b = OaxacaBuilder::new(df, args.outcome_name()?, &args.group, &args.reference);
        b.predictors(predictors.iter().copied())
            .categorical_predictors(categorical_predictors.iter().copied());
        b
    };

    builder
        .bootstrap_reps(args.bootstrap_reps)
        .reference_coefficients(reference_coeffs);
    args.normalization.apply(&mut builder);

    apply_weights(&mut builder, args);

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
    let reference_coeffs = args.ref_coeffs.to_library();
    let outcome = args.outcome_name()?;

    for &q in &quantiles {
        // Arc/COW-cheap clone (stage-2 Finding 1); each τ gets a fresh builder.
        let mut builder = OaxacaBuilder::new(df.clone(), outcome, &args.group, &args.reference);
        builder
            .predictors(predictors.iter().copied())
            .categorical_predictors(categorical_predictors.iter().copied())
            .bootstrap_reps(args.bootstrap_reps)
            .reference_coefficients(reference_coeffs);
        args.normalization.apply(&mut builder);
        apply_weights(&mut builder, args);

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

#[cfg(feature = "experimental")]
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

    let mut builder = AkmBuilder::new(df, args.outcome_name()?, worker_col, firm_col);
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

#[cfg(feature = "experimental")]
fn run_matching_analysis(args: &RunArgs, df: DataFrame) -> Result<(), Box<dyn Error>> {
    use oaxaca_blinder::MatchingEngine;
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
    let engine = MatchingEngine::new(
        df,
        &args.group,
        args.outcome_name()?,
        predictors.iter().copied(),
    );

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
    let df = read_csv(&args.data, &[args.outcome.as_str()])?;
    let predictors: Vec<&str> = args.predictors.iter().map(AsRef::as_ref).collect();
    let categorical_predictors: Vec<&str> = args
        .categorical
        .as_ref()
        .map(|v| v.iter().map(AsRef::as_ref).collect())
        .unwrap_or_default();
    // Same rule as `run`: categorical predictors are normalised (population-share), so the
    // report's per-level rows do not depend on which level sorts first.
    let results = OaxacaBuilder::new(df, &args.outcome, &args.group, &args.reference)
        .predictors(predictors.iter().copied())
        .categorical_predictors(categorical_predictors.iter().copied())
        .normalize_all_categoricals()
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
        std::process::exit(1);
    }
}
