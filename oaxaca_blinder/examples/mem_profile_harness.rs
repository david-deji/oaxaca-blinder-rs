//! Native memory-profile harness (0014-MERIDIAN, In-Scope 11 / D1).
//!
//! Drives the `mem-profile` tracking allocator through the `decompose` path at
//! n ∈ {10k, 25k, 50k} on the ×5 fixture and reports the four D1 quantities
//! (`H_res`, `H_peak`, `Sc`, `St_obs`) plus the AC-M10 resample reduction proof
//! (`Sc_before` vs `Sc_after` at 50k, resampling isolated). Single-threaded
//! (RAYON_NUM_THREADS pinned to 1) so it measures the CURRENT single-threaded
//! memory curve the threading stage sizes its shared-memory budget against.
//!
//! Requires `--features mem-profile` (enforced by `required-features` in
//! Cargo.toml, so a default `cargo build --examples` never compiles it — INV-01).
//!
//! Run:
//!   cargo run --release -p oaxaca_blinder --example mem_profile_harness \
//!     --features mem-profile
//!
//! Reads `tests/fixtures/mem_profile_50k.csv` (generate it first with
//! `cargo run -p oaxaca_blinder --example gen_mem_fixture`).

use oaxaca_blinder::mem_profile::{
    current_bytes, last_checkpoint_a, last_checkpoint_b, last_checkpoint_b_peak, peak_bytes,
    reset_peak, set_reset_peak_at_checkpoint_b,
};
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const OUTCOME: &str = "Salary";
const GROUP: &str = "Gender";
const REFERENCE: &str = "Female";
const NUMERIC: [&str; 2] = ["Age", "Experience_Years"];
const CATEGORICAL: [&str; 4] = ["Education_Level", "Department", "Job_Title", "Location"];
/// Fixed profiling seed — the harness measures memory, not statistics, but a
/// pinned seed keeps the allocation trace reproducible (AC-M2).
const PROFILE_SEED: u64 = 0x0014_0050_0F11_E000;

fn fixture_path() -> String {
    format!(
        "{}/tests/fixtures/mem_profile_50k.csv",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Load the first `n` rows of the fixture, casting numeric columns to Float64
/// so the schema is identical across n (a pure-k=0 10k read would otherwise
/// infer Int64 for the all-integer Salary column and drift from 25k/50k).
fn load_fixture(n: usize) -> DataFrame {
    LazyCsvReader::new(fixture_path())
        .with_has_header(true)
        .with_n_rows(Some(n))
        .finish()
        .expect("fixture readable — run `gen_mem_fixture` first")
        .with_columns([
            col("Salary").cast(DataType::Float64),
            col("Age").cast(DataType::Float64),
            col("Experience_Years").cast(DataType::Float64),
        ])
        .collect()
        .expect("fixture parses")
}

fn build(df: DataFrame, reps: usize) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(df, OUTCOME, GROUP, REFERENCE);
    b.predictors(NUMERIC.to_vec())
        .categorical_predictors(CATEGORICAL.to_vec())
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(reps)
        .seed(PROFILE_SEED);
    b
}

/// Mirror of `rng::resample_indices` (pub(crate), not reachable from an
/// example) for the AC-M10 isolated new-path resample.
fn resample_idx(rng: &mut ChaCha8Rng, n: usize) -> IdxCa {
    let n_idx = n as IdxSize;
    let idx: Vec<IdxSize> = (0..n).map(|_| rng.gen_range(0..n_idx)).collect();
    IdxCa::from_vec("idx".into(), idx)
}

fn filter_group(df: &DataFrame, value: &str) -> DataFrame {
    let mask = df.column(GROUP).unwrap().str().unwrap().equal(value);
    df.filter(&mask).unwrap()
}

struct Row {
    n: usize,
    logical: usize, // df.estimated_size() cross-check
    h_res: usize,
    h_peak: usize,
    sc: usize,
}

fn mib(b: usize) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

fn profile_n(n: usize) -> Row {
    // --- H_res + H_peak: one 100-rep single-threaded run, peak left running. ---
    set_reset_peak_at_checkpoint_b(false);
    let (h_res, h_peak, logical) = {
        let df = load_fixture(n);
        let logical = df.estimated_size();
        reset_peak(); // anchor peak just before the whole run()
        let b = build(df, 100);
        let _ = b.run().expect("100-rep decompose");
        let h_res = last_checkpoint_b();
        let h_peak = peak_bytes();
        // sanity: checkpoint A (post-hstack) must be <= checkpoint B (pre-loop)
        debug_assert!(last_checkpoint_a() <= h_res);
        (h_res, h_peak, logical)
    }; // builder + result drop here → CURRENT falls back toward floor

    // --- Sc: reps=1, checkpoint B re-anchors peak → marginal of one in-flight rep. ---
    set_reset_peak_at_checkpoint_b(true);
    let sc = {
        let df = load_fixture(n);
        let b = build(df, 1);
        let _ = b.run().expect("1-rep decompose");
        peak_bytes().saturating_sub(last_checkpoint_b())
    };

    Row {
        n,
        logical,
        h_res,
        h_peak,
        sc,
    }
}

/// AC-M9/M10: isolate the per-rep resampling allocation for the old clone-pair
/// path vs the new shared-base `take` path at 50k. Everything downstream
/// (`run_single_pass`) is identical between the two, so the bracketed delta is
/// purely the resampling lever.
fn resample_before_after(n: usize) -> (usize, usize) {
    let df = load_fixture(n);
    let df_a = filter_group(&df, "Female");
    let df_b = filter_group(&df, "Male");
    let mut rng = ChaCha8Rng::seed_from_u64(0x0014_A110_0000_0011);

    // NEW: take on the shared read-only base — no per-rep clone.
    set_reset_peak_at_checkpoint_b(false);
    reset_peak();
    let base = current_bytes();
    {
        let ia = resample_idx(&mut rng, df_a.height());
        let ib = resample_idx(&mut rng, df_b.height());
        let sa = df_a.take(&ia).unwrap();
        let sb = df_b.take(&ib).unwrap();
        let s = sa.vstack(&sb).unwrap();
        std::hint::black_box(&s);
    }
    let sc_after = peak_bytes().saturating_sub(base);

    // OLD: clone both group frames, then sample_n_literal, then vstack.
    reset_peak();
    let base = current_bytes();
    {
        let da = df_a.clone();
        let db = df_b.clone();
        let sa = da
            .sample_n_literal(da.height(), true, false, None)
            .unwrap();
        let sb = db
            .sample_n_literal(db.height(), true, false, None)
            .unwrap();
        let s = sa.vstack(&sb).unwrap();
        std::hint::black_box(&s);
    }
    let sc_before = peak_bytes().saturating_sub(base);

    (sc_before, sc_after)
}

fn main() {
    // Rayon pool size is taken from the RAYON_NUM_THREADS env var set OUTSIDE
    // the process (setting it here via set_var is too late — the global pool is
    // already sized on first use). Print the actual pool size so the peak can be
    // read as H_res + N_concurrent × Sc.
    eprintln!(
        "[rayon] current_num_threads = {}",
        rayon::current_num_threads()
    );

    // Peak-attribution diagnostic @ 50k: split pre-loop peak (point estimate +
    // hstack, before checkpoint B) from final peak (adds the bootstrap loop),
    // swept over rep count. If final peak ≈ pre-loop peak and is flat in reps,
    // the peak is the serial point-estimate transient (rep- and thread-
    // independent); if it grows with reps, the bootstrap loop drives it.
    println!("### Peak attribution @ 50k (pre-loop vs final, by rep count)\n");
    println!("| reps | H_res (ckpt B) | pre-loop peak (ckpt B) | final peak | Δ bootstrap loop |");
    println!("|---|---|---|---|---|");
    set_reset_peak_at_checkpoint_b(false);
    for reps in [1usize, 10, 50, 100] {
        let df = load_fixture(50_000);
        reset_peak();
        let b = build(df, reps);
        let _ = b.run().expect("diagnostic run");
        let h_res = last_checkpoint_b();
        let pre = last_checkpoint_b_peak();
        let fin = peak_bytes();
        println!(
            "| {} | {:.1} MiB | {:.1} MiB | {:.1} MiB | {:.1} MiB |",
            reps,
            mib(h_res),
            mib(pre),
            mib(fin),
            mib(fin.saturating_sub(pre))
        );
    }
    println!();

    let ns = [10_000usize, 25_000, 50_000];
    let rows: Vec<Row> = ns.iter().map(|&n| profile_n(n)).collect();
    let (sc_before, sc_after) = resample_before_after(50_000);

    println!("## Native memory profile (single-threaded, tracking GlobalAlloc)\n");
    println!("| n | logical (estimated_size) | H_res | H_peak | Sc (per-rep marginal) |");
    println!("|---|---|---|---|---|");
    for r in &rows {
        println!(
            "| {} | {:.2} MiB | {:.2} MiB | {:.2} MiB | {:.3} MiB |",
            r.n,
            mib(r.logical),
            mib(r.h_res),
            mib(r.h_peak),
            mib(r.sc)
        );
    }
    println!("\n### Raw bytes (for constant computation)\n");
    for r in &rows {
        println!(
            "n={} logical={} H_res={} H_peak={} Sc={}",
            r.n, r.logical, r.h_res, r.h_peak, r.sc
        );
    }
    println!("\n### AC-M10 resample reduction @ 50k (resampling isolated)\n");
    println!("Sc_before (clone-pair + sample_n_literal): {} bytes ({:.3} MiB)", sc_before, mib(sc_before));
    println!("Sc_after  (shared-base take):              {} bytes ({:.3} MiB)", sc_after, mib(sc_after));
    if sc_after < sc_before {
        println!(
            "REDUCTION CONFIRMED: Sc_after < Sc_before by {} bytes ({:.1}%)",
            sc_before - sc_after,
            100.0 * (sc_before - sc_after) as f64 / sc_before as f64
        );
    } else {
        println!("WARNING: no reduction measured (sc_after >= sc_before)");
    }
}
