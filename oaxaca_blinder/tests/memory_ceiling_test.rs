//! Memory-ceiling @ 50k rows (0014-MERIDIAN, verification-benchmark D-3, AC-4; INV-05).
//!
//! Threading trades memory for speed; this test proves the trade stays inside the WASM
//! shared-memory budget. It runs a ~50k-row decomposition at the worst-case in-band pool
//! size (N_max_const=8) under the stage-2 tracking allocator and asserts the peak live-byte
//! count stays below the `--max-memory` link budget. Consumes the memory-budget domain's
//! `mem_profile::peak_bytes()` metric — it does not re-measure it.
//!
//! Sited in oaxaca_blinder (not pay-equity-engine as the spec names) because the tracking
//! allocator + peak metric live here (feature `mem-profile`, native-only, off by default).
//! Meaningful ONLY under that feature:
//!   cargo test -p oaxaca_blinder --features mem-profile --test memory_ceiling_test
//! Without the feature the allocator is absent, so the body is cfg'd out and the test skips
//! loudly rather than passing vacuously on a fabricated 0-byte peak (failure mode #6 guard).
//!
//! The 50k frame is 5x the committed 10k `employers_trust_fixture.csv` (no jitter — peak
//! memory is a function of row count + structure, not statistical realism), avoiding a
//! committed 50k binary fixture (repo-management / stage-2 gitignore posture).

#[test]
fn ac4_memory_ceiling_50k_under_budget() {
    #[cfg(not(feature = "mem-profile"))]
    {
        eprintln!(
            "SKIP ac4_memory_ceiling_50k: requires `--features mem-profile` (tracking allocator). \
             Not run under default features."
        );
    }

    #[cfg(feature = "mem-profile")]
    {
        use oaxaca_blinder::mem_profile::{peak_bytes, reset_peak};
        use oaxaca_blinder::OaxacaBuilder;
        use polars::prelude::*;
        use rayon::ThreadPoolBuilder;

        // --max-memory link budget (0014-MERIDIAN memory report M_max = 342_228_992 B = 326 MiB).
        const DECLARED_MAX: usize = 342_228_992;
        const N_MAX_CONST: usize = 8; // worst-case in-band pool size (stage-2 peak 248 MiB @ N=8)

        let base = LazyCsvReader::new("tests/fixtures/employers_trust_fixture.csv")
            .with_has_header(true)
            .finish()
            .expect("fixture readable")
            .collect()
            .expect("fixture parses");
        let mut df = base.clone();
        for _ in 0..4 {
            df = df.vstack(&base).expect("vstack");
        }
        assert!(
            df.height() >= 50_000,
            "50k frame build: got {}",
            df.height()
        );

        let pool = ThreadPoolBuilder::new()
            .num_threads(N_MAX_CONST)
            .build()
            .expect("rayon pool");

        reset_peak();
        let peak = pool.install(|| {
            let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
            b.predictors(vec!["Age", "Experience_Years"])
                .categorical_predictors(vec!["Education_Level", "Department", "Location"])
                .bootstrap_reps(20); // peak = H_res + N*Sc, flat in rep count
            b.run().expect("50k decompose");
            peak_bytes()
        });

        let mib = |b: usize| b / 1_048_576;
        eprintln!(
            "AC-4 memory-ceiling @ ~50k, N={}: peak = {} MiB, budget = {} MiB, headroom = {} MiB",
            N_MAX_CONST,
            mib(peak),
            mib(DECLARED_MAX),
            mib(DECLARED_MAX.saturating_sub(peak))
        );
        assert!(
            peak < DECLARED_MAX,
            "50k peak {} B ({} MiB) >= --max-memory budget {} B ({} MiB) — would OOM the SharedArrayBuffer",
            peak,
            mib(peak),
            DECLARED_MAX,
            mib(DECLARED_MAX)
        );
    }
}
