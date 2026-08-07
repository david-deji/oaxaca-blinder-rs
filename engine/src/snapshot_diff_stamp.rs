//! The freshness stamp for the snapshot diff (0018-MERIDIAN phase 4, F2).
//!
//! ## The failure this exists to catch
//!
//! `cargo build --target wasm32-unknown-unknown` produces a raw `.wasm` and stops. It runs no
//! wasm-bindgen and touches nothing the browser loads. Only `scripts/build-wasm.sh` publishes.
//! And the app's suites MOCK the engine, so a blob that lags the Rust source keeps every test on
//! both sides green while the browser computes old numbers. A green `cargo test` is therefore
//! evidence about the SOURCE and evidence about nothing that ships.
//!
//! This module makes the shipped blob able to answer "which diff source was I compiled from".
//! `wasmDiffParity.spec.js` instantiates `frontend/src/wasm/pay_equity_engine_bg.wasm`, calls
//! `diff_source_sha256_js()`, and compares it against the digest recomputed in JS from the two
//! `.rs` files on disk. Editing `snapshot_diff.rs` without re-running `build-wasm.sh` turns that
//! assertion red — which is the only way "the Rust changed" and "the shipped blob changed" can
//! be told apart from inside a test.
//!
//! ## Why `include_str!` and not a `build.rs`
//!
//! No new build-dependency, no `rerun-if-changed` bookkeeping to get wrong, and the bytes that
//! are hashed are literally the bytes that were compiled in.
//!
//! Both files are hashed because hashing only the core would let a change to the ENCODER
//! (`snapshot_diff_wasm.rs`) ship stale — and the encoder is where a wire-format change lives.
//! `snapshot_diff_tests.rs` is deliberately NOT hashed: a test edit is not an engine change, and
//! including it would force a republish to keep the freshness assertion green.
//!
//! ## The digest, stated so JS can reproduce it byte for byte
//!
//!   sha256( u64le(len(core)) || core_bytes || u64le(len(wasm)) || wasm_bytes )
//!
//! Length-prefixed so the concatenation is unambiguous: moving a line from one file to the other
//! changes the digest, which a bare concatenation would not.

use sha2::{Digest, Sha256};

pub const DIFF_SOURCE_CORE: &str = include_str!("snapshot_diff.rs");
pub const DIFF_SOURCE_WASM: &str = include_str!("snapshot_diff_wasm.rs");

pub fn diff_source_sha256() -> String {
    let mut hasher = Sha256::new();
    hasher.update((DIFF_SOURCE_CORE.len() as u64).to_le_bytes());
    hasher.update(DIFF_SOURCE_CORE.as_bytes());
    hasher.update((DIFF_SOURCE_WASM.len() as u64).to_le_bytes());
    hasher.update(DIFF_SOURCE_WASM.as_bytes());
    let out = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in out.iter() {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

#[cfg(feature = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn diff_source_sha256_js() -> String {
    diff_source_sha256()
}
