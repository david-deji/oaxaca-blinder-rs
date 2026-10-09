//! Deterministic, schedule-independent RNG for bootstrap resampling and the
//! Machado–Mata / RIF quantile simulation (0014-MERIDIAN, In-Scope 3 + 10).
//!
//! Every per-unit RNG is a closed-form function of `(master, purpose, unit)` — no
//! thread id, no clock, no atomic counter. This is the SC-03 property: the stream
//! a given rep draws is fully determined by its inputs, so bootstrap output is
//! bit-identical across thread counts and sequential/threaded modes (INV-02,
//! within-platform).
//!
//! The master seed defaults to the documented constant [`DEFAULT_SEED`]
//! (reproducible-by-default, for audit defensibility). `seed_from_entropy()` on the
//! builders is the native-only opt-in for run-to-run variety; it records the drawn
//! seed in [`RunMetadata`] so the run stays reproducible after the fact.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use polars::prelude::{IdxCa, IdxSize};

/// Fixed master seed for all bootstrap resampling. Reproducible-by-default.
pub const DEFAULT_SEED: u64 = 0x5EED_0A11_CA8A_0002;

/// The `rand_chacha` version pinned in `Cargo.lock`, recorded in [`RunMetadata`]
/// so a serialized run declares which ChaCha implementation produced it (RK2).
pub const RAND_CHACHA_VERSION: &str = "0.3.1";

/// Which RNG site a stream belongs to. Spaced into the high bits of the ChaCha
/// stream id (below) so the three sites can never collide on a stream even when
/// they share a `unit`.
#[derive(Clone, Copy)]
pub(crate) enum RngPurpose {
    Bootstrap = 0,
    QuantileTau = 1,
    QuantileResample = 2,
}

/// Per-unit ChaCha8 stream. `set_stream` gives statistically independent streams
/// for adjacent ids by ChaCha construction; `purpose` is spaced into high bits to
/// guarantee no cross-site stream collision. Bit-identical across thread counts
/// because `(master, purpose, unit)` fully determines the stream with zero
/// dependence on execution order.
///
/// `set_stream(u64)` verified present at `rand_chacha-0.3.1 chacha.rs:237` (L2).
pub(crate) fn unit_rng(master: u64, purpose: RngPurpose, unit: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(master);
    rng.set_stream(((purpose as u64) << 40) ^ unit);
    rng
}

/// With-replacement bootstrap indices in `0..n`, drawn from `rng`. Returns an
/// `IdxCa` suitable for `DataFrame::take` (gather semantics, bounds-checked,
/// duplicates allowed) — verified at `polars-core-0.44.2 frame/mod.rs:1841` (L1).
pub(crate) fn resample_indices(rng: &mut ChaCha8Rng, n: usize) -> IdxCa {
    let n_idx = n as IdxSize;
    let idx: Vec<IdxSize> = (0..n).map(|_| rng.gen_range(0..n_idx)).collect();
    IdxCa::from_vec("idx".into(), idx)
}

/// One bootstrap replicate of a FREQUENCY-weighted group (0120-MERIDIAN E-REV-1).
///
/// Under `WeightsKind::Frequency` a row with `w = 2` stands for two identical employees, so the
/// inferential sample is `sum(w)` units, not `rows` units. A replicate draws `N = sum(w)` units
/// with replacement, each unit being row `i` with probability `w_i / N` (multinomial), and
/// returns the rows that were drawn at least once (ascending) with their draw counts, which
/// become that replicate's weights. A replicate on `w = 2` is therefore distributed as a replicate
/// on the same rows written twice with `w = 1`, which is what Stata `bsample ..., weight(fw)` draws.
/// Weights must be whole numbers (validated before the bootstrap runs).
pub(crate) fn resample_frequency(rng: &mut ChaCha8Rng, weights: &[f64]) -> (IdxCa, Vec<f64>) {
    let mut cum: Vec<u64> = Vec::with_capacity(weights.len());
    let mut acc: u64 = 0;
    for &w in weights {
        acc += w.max(0.0) as u64;
        cum.push(acc);
    }
    let mut counts = vec![0u64; weights.len()];
    if acc > 0 {
        for _ in 0..acc {
            let r = rng.gen_range(0..acc);
            // first row whose cumulative weight exceeds r; zero-weight rows are never selected
            counts[cum.partition_point(|&c| c <= r)] += 1;
        }
    }
    let mut idx: Vec<IdxSize> = Vec::new();
    let mut drawn: Vec<f64> = Vec::new();
    for (i, &c) in counts.iter().enumerate() {
        if c > 0 {
            idx.push(i as IdxSize);
            drawn.push(c as f64);
        }
    }
    (IdxCa::from_vec("idx".into(), idx), drawn)
}

/// Outcome of a single bootstrap replicate. Replaces the silent `filter_map(...).ok()`
/// discard with an explicit, order-preserving marker so the discard count is a pure
/// function of `(master, rep, base frames)` and therefore thread-count-independent (D5).
pub(crate) enum RepOutcome<T> {
    Ok(T),
    Failed,
}

/// Draw one master seed from OS entropy. Native/CLI only: `oaxaca_blinder` declares
/// no direct `getrandom` dependency, so the entropy draw is scoped off the wasm path
/// (the wasm consumer passes an explicit seed or takes [`DEFAULT_SEED`]) — decision D1.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn draw_entropy_seed() -> u64 {
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut buf = [0u8; 8];
    OsRng.fill_bytes(&mut buf);
    u64::from_le_bytes(buf)
}

/// Serialize a `u64` as a decimal string (see `RunMetadata::seed`). Lossless across the JS
/// boundary where a bare number would exceed `Number.MAX_SAFE_INTEGER`.
fn serialize_u64_as_str<S: serde::Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

/// Provenance of a decomposition run: the effective master seed, the RNG algorithm
/// and pinned version, and the bootstrap-rep accounting. The `*_discarded` count is
/// the authoritative record of failed reps (the `eprintln!` log is non-authoritative).
/// Serialized into the engine `decompose` JSON the parity harness byte-compares (D7).
#[derive(serde::Serialize, Clone, Debug)]
pub struct RunMetadata {
    // Serialized as a STRING, not a JSON number. A 64-bit seed exceeds JS `Number.MAX_SAFE_INTEGER`
    // (2^53), so a numeric encoding is (a) lossy for any JS consumer of the native JSON and (b) an
    // outright throw under serde_wasm_bindgen at the wasm boundary. A decimal string is the correct
    // lossless provenance encoding in both serializers. Value + Rust type are unchanged; only the
    // JSON encoding differs, so mean-path/statistical goldens are untouched (0014-MERIDIAN stage-4).
    #[serde(serialize_with = "serialize_u64_as_str")]
    pub seed: u64,
    pub rng_algorithm: &'static str,
    pub rand_chacha_version: &'static str,
    pub bootstrap_reps_requested: usize,
    pub bootstrap_reps_succeeded: usize,
    pub bootstrap_reps_discarded: usize,
    /// RIF quantile bootstrap policy (0014-MERIDIAN ruling 4). `Some(false)` = the RIF
    /// transform is recomputed inside every bootstrap replicate (each resample re-estimates
    /// its own `q_τ`/`f_Y(q_τ)` → correct quantile CIs — the ratified default). `Some(true)`
    /// = legacy compute-once. `None` on non-quantile (mean/OLS) runs; omitted from
    /// serialization there so the mean-path bytes are unchanged (AC-9/AC-6 baselines hold).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_rif: Option<bool>,
    /// Categorical-coefficient normalisation actually requested for this run (0120-MERIDIAN
    /// S1): the convention, the share basis and the restriction weights of the POINT-estimate
    /// sample (bootstrap replicates recompute theirs from each replicate's pooled resample).
    /// `None` when the caller did not call `normalize()`, and omitted from serialization then,
    /// so the raw-library bytes (AC-9 / AC-6 baselines) are unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalization: Option<crate::math::normalization::NormalizationRecord>,
    /// The `reference_coefficients` scheme the headline was computed under. Set by the engine
    /// layer (the library leaves it `None`, so raw-library bytes are unchanged). 0120 T11.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_coefficients_used: Option<String>,
    /// Version of the `pay-equity-engine` crate that produced the result. Engine layer only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    /// Method marker set by the engine layer: `"oaxaca-blinder-mean"` or `"rif-quantile"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// `variable=level` entries for every categorical level that was absent from a bootstrap
    /// resample of a group and so made that replicate fail (the replicate is discarded and
    /// counted in `bootstrap_reps_discarded`). Sorted, de-duplicated; omitted when empty. A
    /// level listed here is estimated from fewer replicates than the others.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bootstrap_discard_levels: Vec<String>,
}

impl RunMetadata {
    /// Build the record from a resolved master seed and the rep accounting. `fixed_rif`
    /// defaults to `None` (mean/OLS run); the quantile path sets it via [`with_fixed_rif`].
    pub(crate) fn new(seed: u64, requested: usize, succeeded: usize, discarded: usize) -> Self {
        RunMetadata {
            seed,
            rng_algorithm: "ChaCha8",
            rand_chacha_version: RAND_CHACHA_VERSION,
            bootstrap_reps_requested: requested,
            bootstrap_reps_succeeded: succeeded,
            bootstrap_reps_discarded: discarded,
            fixed_rif: None,
            normalization: None,
            reference_coefficients_used: None,
            engine_version: None,
            method: None,
            bootstrap_discard_levels: Vec::new(),
        }
    }

    /// Record the RIF-recompute policy on a quantile run (ruling 4). `false` = per-replicate
    /// recompute (the ratified default); `true` = legacy compute-once on the full sample.
    pub(crate) fn with_fixed_rif(mut self, fixed_rif: bool) -> Self {
        self.fixed_rif = Some(fixed_rif);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn first_draws(mut rng: ChaCha8Rng, k: usize) -> Vec<u64> {
        (0..k).map(|_| rng.next_u64()).collect()
    }

    /// AC-3(a): `unit_rng(m,p,u)` is referentially transparent — same args produce
    /// the identical first 16 `u64` draws.
    #[test]
    fn ac3_referential_transparency() {
        let a = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 7);
        let b = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 7);
        assert_eq!(first_draws(a, 16), first_draws(b, 16));

        // Different unit -> different stream (sanity: not the same sequence).
        let c = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 8);
        assert_ne!(
            first_draws(unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 7), 16),
            first_draws(c, 16)
        );
    }

    /// AC-3(b): the three `RngPurpose` values yield pairwise-disjoint draw
    /// sequences for the same `(master, unit)`.
    #[test]
    fn ac3_purposes_disjoint() {
        let master = DEFAULT_SEED;
        let unit = 3;
        let boot = first_draws(unit_rng(master, RngPurpose::Bootstrap, unit), 16);
        let tau = first_draws(unit_rng(master, RngPurpose::QuantileTau, unit), 16);
        let resample = first_draws(unit_rng(master, RngPurpose::QuantileResample, unit), 16);
        assert_ne!(boot, tau, "Bootstrap vs QuantileTau collided");
        assert_ne!(boot, resample, "Bootstrap vs QuantileResample collided");
        assert_ne!(tau, resample, "QuantileTau vs QuantileResample collided");
    }

    /// AC-3(c): `resample_indices(rng, n)` is bit-identical across two calls with a
    /// cloned rng, and every index is `< n`.
    #[test]
    fn ac3_resample_indices_deterministic_and_bounded() {
        let n = 37usize;
        let base = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 42);
        let mut r1 = base.clone();
        let mut r2 = base;
        let idx1: Vec<IdxSize> = resample_indices(&mut r1, n).into_no_null_iter().collect();
        let idx2: Vec<IdxSize> = resample_indices(&mut r2, n).into_no_null_iter().collect();
        assert_eq!(
            idx1, idx2,
            "resample_indices not reproducible for cloned rng"
        );
        assert_eq!(
            idx1.len(),
            n,
            "resample_indices must return exactly n indices"
        );
        assert!(
            idx1.iter().all(|&i| (i as usize) < n),
            "every resampled index must be < n"
        );
    }
    /// E-REV-1: a frequency replicate draws `sum(w)` units, never selects a zero-weight row,
    /// returns ascending distinct rows, is reproducible, and gives row `i` a mean count of `w_i`.
    #[test]
    fn frequency_replicate_draws_the_expanded_sample() {
        let w = [2.0, 0.0, 5.0, 1.0, 0.0, 3.0];
        let total: f64 = w.iter().sum();
        let mut sum_counts = vec![0.0f64; w.len()];
        let reps = 4000u64;
        for rep in 0..reps {
            let mut rng = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, rep);
            let (idx, counts) = resample_frequency(&mut rng, &w);
            let idx: Vec<IdxSize> = idx.into_no_null_iter().collect();
            assert_eq!(counts.iter().sum::<f64>(), total, "sum(w) units are drawn");
            assert_eq!(idx.len(), counts.len());
            assert!(idx.windows(2).all(|p| p[0] < p[1]), "ascending, distinct");
            assert!(counts.iter().all(|&c| c >= 1.0));
            for (i, c) in idx.iter().zip(&counts) {
                assert!(w[*i as usize] > 0.0, "a zero-weight row was drawn");
                sum_counts[*i as usize] += c;
            }
        }
        for (i, wi) in w.iter().enumerate() {
            let mean = sum_counts[i] / reps as f64;
            assert!(
                (mean - wi).abs() < 0.08,
                "row {i}: mean count {mean} vs weight {wi}"
            );
        }
        let (a, ca) = resample_frequency(&mut unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 9), &w);
        let (b, cb) = resample_frequency(&mut unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, 9), &w);
        assert_eq!(
            a.into_no_null_iter().collect::<Vec<_>>(),
            b.into_no_null_iter().collect::<Vec<_>>()
        );
        assert_eq!(ca, cb);
    }
}
