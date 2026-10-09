#!/usr/bin/env bash
# ground.sh: deterministic ground-truth probes for the dev loop (0119-MERIDIAN S6).
#
# Contract: internal-ops-bureau/knowledge/dev-loop.md (telos-machina) section "ground.sh contract". Pure
# probes, no LLM, no judgment. Exits 0 ALWAYS: a probe that could not run writes null plus a reason into
# the JSON and into errors[] (scripts/lib/ground_probes.py), and a total crash of that script is caught
# here and reported as a single error entry. The report goes to stdout and to ground/YYYY-MM-DD-probes.json;
# when errors[] is not empty a banner goes to stderr, so a crash and a clean run are not the same event.
#
# Usage: bash scripts/ground.sh
# Env:   GROUND_SKIP_WASM_VERIFY=1   skip the (slow) `build-wasm.sh --verify` probe; it is then reported as not run
#        GROUND_CARGO_TIMEOUT / GROUND_WASM_VERIFY_TIMEOUT / GROUND_NET_TIMEOUT   seconds
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GROUND_DIR="${GROUND_DIR:-$REPO_ROOT/ground}"
export GROUND_DIR
mkdir -p "$GROUND_DIR"
OUT_FILE="$GROUND_DIR/$(date -u +%Y-%m-%d)-probes.json"
TMP_OUT="$(mktemp "$GROUND_DIR/.ground-out.XXXXXX")"
TMP_ERR="$(mktemp "$GROUND_DIR/.ground-err.XXXXXX")"

# Progress lines from the probes pass straight through to stderr; python's stderr is also kept for the crash case.
python3 "$REPO_ROOT/scripts/lib/ground_probes.py" > "$TMP_OUT" 2> >(tee "$TMP_ERR" >&2)
PY_EXIT=$?
wait

if [ "$PY_EXIT" -eq 0 ] && [ -s "$OUT_FILE" ] && python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$OUT_FILE" 2>/dev/null; then
  : # the probes wrote $OUT_FILE themselves (atomically); TMP_OUT is the same document
else
  STDERR_TAIL="$(tail -c 1500 "$TMP_ERR" 2>/dev/null | tr '"\\' "'/" | tr '\n' ' ')"
  RAN_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '{"ran_at":"%s","commit":null,"suites":{},"gate_receipts":{},"verify_live":{},"register_drift":{},"open_issues":null,"flake_recurrence":null,"app_specific":{},"errors":["ground_probes.py failed (exit %s): %s"]}\n' \
    "$RAN_AT" "$PY_EXIT" "$STDERR_TAIL" > "$OUT_FILE"
fi
rm -f "$TMP_OUT" "$TMP_ERR"

cat "$OUT_FILE"

# The report is also read back here so the banner is built from the file, not from what the probes meant to say.
python3 - "$OUT_FILE" >&2 <<'PY' 2>/dev/null || true
import json, sys
d = json.load(open(sys.argv[1]))
errs = d.get("errors") or []
if errs:
    print("")
    print(f"=== GROUND REPORTED {len(errs)} ERROR(S): the probes did not all run or did not all agree ===")
    for e in errs:
        print("  " + str(e)[:400])
    suites = list((d.get("suites") or {}).keys())
    print("  suites reported: " + (", ".join(suites) if suites else "NONE"))
    print("  artifact: " + sys.argv[1])
    print("=" * 78)
PY

exit 0
