//! ×5 memory-profile fixture generator (0014-MERIDIAN, In-Scope 13 / D7).
//!
//! Reads the 10 000-row `Employers_data.csv` and emits a 50 000-row `.csv`
//! (`tests/fixtures/mem_profile_50k.csv` by default) as five replicates
//! `k ∈ {0,1,2,3,4}` written in k-order. Replicate `k=0` is the byte-verbatim
//! original (so the 10k profile point is exact real data); `k>0` applies the
//! D7 perturbation. Written in k-order so the harness's row-count subsets are
//! plain prefixes: 10k = first 10 000 (k=0), 25k = first 25 000 (k∈{0,1} +
//! first 5 000 of k=2), 50k = all.
//!
//! Deterministic: a fixed master seed drives a per-replicate `ChaCha8Rng`
//! (same family the deterministic-rng module selects), drawn sequentially, so
//! the output is byte-reproducible and schedule-independent (AC-M13). The
//! group variable (`Gender`) and every categorical predictor
//! (`Department, Job_Title, Education_Level, Location`) are copied verbatim
//! (AC-M14) — perturbing the group would corrupt the decomposition target,
//! and perturbing the categoricals would distort the dictionary/one-hot width
//! the profile is trying to measure realistically.
//!
//! The output is a gitignored, regenerable derived artifact (AC-M15) — never
//! committed (`.claude/rules/repo-management.md` output-pruning posture).
//!
//! Run: `cargo run -p oaxaca_blinder --example gen_mem_fixture [SRC_CSV] [OUT_CSV]`
//! Defaults: SRC = `$HOME/Downloads/Employers_data.csv`, OUT =
//! `<CARGO_MANIFEST_DIR>/tests/fixtures/mem_profile_50k.csv`.

use std::io::{BufWriter, Write};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Fixed master seed for the ×5 fixture. Documented literal (AC-M13): changing
/// it changes every `k>0` byte, so it is pinned. Distinct high bytes
/// (`0x0014…`) tie it visually to issue 0014-MERIDIAN.
const MEM_FIXTURE_SEED: u64 = 0x0014_50CE_5EED_A115;

/// Golden-ratio odd constant (same one deterministic-rng uses for rep-master
/// separation) — mixes the replicate index into a well-separated stream seed
/// so replicate RNGs don't correlate.
const PHI: u64 = 0x9E37_79B9_7F4A_7C15;

const REPLICATES: u64 = 5;
const ROWS_PER_REPLICATE: usize = 10_000;

fn main() {
    let mut args = std::env::args().skip(1);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let src = args
        .next()
        .unwrap_or_else(|| format!("{home}/Downloads/Employers_data.csv"));
    let out = args.next().unwrap_or_else(|| {
        format!(
            "{}/tests/fixtures/mem_profile_50k.csv",
            env!("CARGO_MANIFEST_DIR")
        )
    });

    let raw = std::fs::read_to_string(&src)
        .unwrap_or_else(|e| panic!("cannot read source CSV {src}: {e}"));
    let mut lines = raw.lines();
    let header = lines.next().expect("source CSV has a header row");

    // Materialize the 10 000 data rows verbatim (k=0 needs exact bytes; k>0
    // parses fields from the same source of truth).
    let rows: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();
    assert_eq!(
        rows.len(),
        ROWS_PER_REPLICATE,
        "expected {ROWS_PER_REPLICATE} source rows, found {}",
        rows.len()
    );

    let file = std::fs::File::create(&out).unwrap_or_else(|e| panic!("cannot create {out}: {e}"));
    let mut w = BufWriter::new(file);
    writeln!(w, "{header}").unwrap();

    let mut total = 0usize;
    for k in 0..REPLICATES {
        // One deterministic stream per replicate, drawn sequentially per row.
        // k=0 draws nothing (identity perturbation) so its RNG stream is unused;
        // keeping the per-k structure uniform makes the k>0 streams independent
        // of whether k=0 consumed draws.
        let mut rng = ChaCha8Rng::seed_from_u64(MEM_FIXTURE_SEED ^ k.wrapping_mul(PHI));
        for line in &rows {
            let f: Vec<&str> = line.split(',').collect();
            assert_eq!(f.len(), 10, "malformed source row (embedded comma?): {line}");
            let id: i64 = f[0].parse().expect("Employee_ID int");
            let name = f[1];
            let age: i64 = f[2].parse().expect("Age int");
            let gender = f[3]; // group var — unchanged (all k)
            let department = f[4]; // categorical — unchanged (all k)
            let job_title = f[5]; // categorical — unchanged (all k)
            let experience: i64 = f[6].parse().expect("Experience_Years int");
            let education = f[7]; // categorical — unchanged (all k)
            let location = f[8]; // categorical — unchanged (all k)
            let salary: f64 = f[9].parse().expect("Salary numeric");

            // k=0 is the untouched original: no id offset, no name suffix, zero
            // jitter — value-exact real data for the 10k profile point. Salary is
            // formatted with 2 decimals for ALL k so the column is uniformly
            // Float64 (Salary is Decimal(18,2) per INV-08); this also keeps CSV
            // schema inference from guessing i64 off the all-integer k=0 prefix.
            let (new_id, new_name, new_age, new_exp, new_salary) = if k == 0 {
                (id, name.to_string(), age, experience, salary)
            } else {
                // Draw order is fixed (age, exp, salary) so the stream is stable.
                let age_j: i64 = rng.gen_range(-2..=2);
                let exp_j: i64 = rng.gen_range(-2..=2);
                let eps: f64 = rng.gen_range(-0.03f64..0.03);
                let na = (age + age_j).clamp(18, 70);
                let ne = (experience + exp_j).clamp(0, (na - 16).max(0));
                let ns = ((salary * (1.0 + eps)) * 100.0).round() / 100.0;
                (
                    id + (k as i64) * ROWS_PER_REPLICATE as i64,
                    format!("{name}_r{k}"),
                    na,
                    ne,
                    ns,
                )
            };

            writeln!(
                w,
                "{new_id},{new_name},{new_age},{gender},{department},{job_title},{new_exp},{education},{location},{new_salary:.2}"
            )
            .unwrap();
            total += 1;
        }
    }
    w.flush().unwrap();
    eprintln!(
        "wrote {total} rows ({REPLICATES} replicates × {ROWS_PER_REPLICATE}) to {out} \
         (seed {MEM_FIXTURE_SEED:#018x})"
    );
}
