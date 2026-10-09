#!/usr/bin/env bash
# regen_norm_goldens.sh -- regenerate the normalisation fixtures and goldens (0120-MERIDIAN).
#
#   R_LIBS_USER=/path/to/R/library bash verification/regen_norm_goldens.sh
#
# Needs R with oaxaca 0.1.5, ddecompose 1.0.0, jsonlite, digest, and cargo. Order matters:
#   1. R writes the two synthetic fixtures (seeded; byte-identical on a rerun).
#   2. The engine writes its own per-group RIF columns for the skewed fixture (the one
#      engine-derived INPUT; the R oracles treat it as an ordinary outcome vector).
#   3. R writes norm_goldens_r.json, recording the sha256 of this chain's inputs. The Rust tests
#      recompute those hashes and refuse a stale golden.
# Commit gen_norm_goldens.R, the fixtures and the golden together: editing the R script alone
# turns the Rust tests red until the golden is regenerated.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
Rscript verification/gen_norm_goldens.R fixtures
CARGO_PROFILE_DEV_DEBUG=0 cargo run --locked -q -p oaxaca_blinder --example emit_rif_fixture
Rscript verification/gen_norm_goldens.R goldens
