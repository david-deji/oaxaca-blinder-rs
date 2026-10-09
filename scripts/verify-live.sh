#!/usr/bin/env bash
# verify-live.sh: defines "live" for the engine (0119-MERIDIAN S6).
#
# Contract: internal-ops-bureau/knowledge/dev-loop.md (telos-machina) section "verify-live.sh contract".
# Refuses a dirty tree; runs `build-wasm.sh --verify`; compares the app's published blobs, glue and
# manifests with engine/pkg and with the raw blob just built, BEFORE anything else of the app runs; then
# runs the app's real-blob specs and the app's own verify-live.sh as separate checks, embedding the app's
# receipt only when it is from this run and its commit is the app HEAD. Writes ground/receipts/<epic>-live.json.
# It copies and publishes nothing. Exit non-zero on any failed check; the receipt is the definition of done.
#
# Usage: bash scripts/verify-live.sh <epic-id>
# Env:   MERIDIAN_FRONTEND=<dir>  the app's frontend/src to compare (default ../pay-equity-app/frontend/src)
#        MERIDIAN_APP=<dir>       the app checkout (default ../pay-equity-app)
#        GROUND_DIR=<dir>         where receipts/ is written (default ./ground)
set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${1:-}" ]; then
  echo "usage: bash scripts/verify-live.sh <epic-id>" >&2
  exit 2
fi
cd "$REPO_ROOT" || exit 2
exec python3 "$REPO_ROOT/scripts/lib/verify_live.py" "$1"
