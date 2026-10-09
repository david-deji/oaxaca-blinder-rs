#!/usr/bin/env python3
"""Tests for scripts/ground.sh and scripts/verify-live.sh (0119-MERIDIAN S6, verification V6).

    python3 scripts/test-loop-scripts.py                 # all
    python3 scripts/test-loop-scripts.py ground-nopath   # one of: ground-nopath ground-normal ground-validator verify-live-corrupt verify-live-dirty

Each test writes its output under target/ (never /tmp: disk is tight) and removes it afterwards.
  ground-nopath         ground.sh with `gh` and `cargo` removed from PATH exits 0, and errors[] names both.
  ground-normal         ground.sh on a normal run: every contract key present, suites total > 0.
  verify-live-corrupt   verify-live.sh against a COPY of the published blobs with one byte flipped: exits 1,
                        fails at published_vs_pkg, runs none of the app's checks, copies nothing.
verify-live-corrupt needs a clean engine tree (verify-live refuses a dirty one) and engine/pkg from a
build-wasm.sh run.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONTRACT_KEYS = ["ran_at", "commit", "suites", "gate_receipts", "verify_live", "register_drift", "open_issues",
                 "flake_recurrence", "app_specific", "errors"]
FAILS = []


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)


def scratch(name):
    base = ROOT / "target" / "0119-tests"
    base.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix=name + "-", dir=base))


def test_ground_nopath():
    print("ground-nopath: ground.sh with gh and cargo removed from PATH")
    tmp = scratch("nopath")
    try:
        bindir = tmp / "bin"
        bindir.mkdir()
        for tool in ["bash", "python3", "date", "mktemp", "tee", "tail", "tr", "cat", "rm", "mkdir", "dirname", "git", "node", "env", "sh", "cut", "sha256sum", "uname"]:
            real = shutil.which(tool)
            if real:
                (bindir / tool).symlink_to(real)
        check(not (bindir / "gh").exists() and not (bindir / "cargo").exists(), "the restricted PATH has neither gh nor cargo")
        env = dict(os.environ, PATH=str(bindir), GROUND_DIR=str(tmp / "ground"))
        p = subprocess.run(["bash", str(ROOT / "scripts" / "ground.sh")], cwd=ROOT, env=env, capture_output=True, text=True, timeout=900)
        check(p.returncode == 0, f"exit code is 0 (got {p.returncode})")
        d = json.loads(p.stdout)
        errs = " | ".join(d["errors"])
        check(all(k in d for k in CONTRACT_KEYS), "every contract key is present")
        check("`gh` not found" in errs, "errors[] names gh")
        check("`cargo` not found" in errs, "errors[] names cargo")
        check(d["suites"]["cargo"]["passed"] is None and "reason" in d["suites"]["cargo"], "the cargo suite is null with a reason, not omitted")
        check(d["open_issues"] is None and d["gate_receipts"]["ci_main"]["conclusion"] is None, "gh-backed keys are null with a reason, not omitted")
        check("GROUND REPORTED" in p.stderr, "the stderr banner is printed")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_ground_normal():
    print("ground-normal: ground.sh on a normal run")
    tmp = scratch("normal")
    try:
        env = dict(os.environ, GROUND_DIR=str(tmp / "ground"))
        p = subprocess.run(["bash", str(ROOT / "scripts" / "ground.sh")], cwd=ROOT, env=env, capture_output=True, text=True, timeout=5400)
        check(p.returncode == 0, f"exit code is 0 (got {p.returncode})")
        d = json.loads(p.stdout)
        check(all(k in d for k in CONTRACT_KEYS), "every contract key is present")
        cargo = d["suites"].get("cargo", {})
        total = sum((s.get("passed") or 0) + (s.get("failed") or 0) + (s.get("skipped") or 0) for s in d["suites"].values())
        check(total > 0, f"suites total is > 0 (got {total})")
        check(cargo.get("listed_total") == (cargo.get("passed") or 0) + (cargo.get("failed") or 0) + (cargo.get("skipped") or 0) + (cargo.get("measured") or 0),
              "the cargo total equals the `cargo test -- --list` total")
        check(not any(e.startswith("suites.") for e in d["errors"]), "no suite error")
        check(isinstance(d["app_specific"].get("wasm_verify", {}).get("match"), bool), "wasm_verify reports match true/false")
        check((tmp / "ground").is_dir() and any((tmp / "ground").glob("*-probes.json")), "the dated probes file was written")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_verify_live_corrupt():
    print("verify-live-corrupt: a corrupted copy of a published blob fails at published_vs_pkg")
    tmp = scratch("vlcorrupt")
    try:
        app_src = ROOT.parent / "pay-equity-app" / "frontend" / "src"
        fe = tmp / "src"
        for d in ("wasm", "wasm-threaded"):
            shutil.copytree(app_src / d, fe / d)
        blob = fe / "wasm" / "pay_equity_engine_bg.wasm"
        data = bytearray(blob.read_bytes())
        data[1000] ^= 0xFF
        blob.write_bytes(bytes(data))
        before = {str(p): p.stat().st_mtime_ns for p in (fe).rglob("*") if p.is_file()}
        env = dict(os.environ, MERIDIAN_FRONTEND=str(fe), GROUND_DIR=str(tmp / "ground"))
        p = subprocess.run(["bash", str(ROOT / "scripts" / "verify-live.sh"), "0119-corrupt"], cwd=ROOT, env=env, capture_output=True, text=True, timeout=2700)
        check(p.returncode == 1, f"exit code is 1 (got {p.returncode}; stderr tail: {p.stderr[-300:]!r})")
        rec = json.loads((tmp / "ground" / "receipts" / "0119-corrupt-live.json").read_text())
        names = {c["name"]: c for c in rec["checks"]}
        check(rec["result"] == "fail" and rec["halted_at"] == "published_vs_pkg", f"fails at published_vs_pkg (halted_at={rec['halted_at']})")
        check(names["wasm_verify"]["result"] == "pass", "wasm_verify itself passed (the source is fine; only the copy is bad)")
        check(names["published_vs_pkg"]["result"] == "fail" and "pay_equity_engine_bg.wasm" in names["published_vs_pkg"]["detail"], "the detail names the differing file")
        check(names["app_real_blob_specs"]["result"] == "not_run" and names["app_verify_live"]["result"] == "not_run", "none of the app's checks ran")
        check(rec["copied_or_published"] is False and rec["app_receipt"] is None, "nothing was copied and no app receipt was embedded")
        after = {str(p): p.stat().st_mtime_ns for p in (fe).rglob("*") if p.is_file()}
        check(before == after, "the corrupted copy was left exactly as it was (no repair, no overwrite)")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_ground_validator():
    print("ground-validator: the validator goes red on zero tests, a shrunk total, and a count that disagrees with --list")
    tmp = scratch("validator")
    try:
        bindir = tmp / "bin"
        bindir.mkdir()
        fake = bindir / "cargo"
        fake.write_text(
            "#!/bin/sh\n"
            'case "$*" in\n'
            '  *--list*) echo "$FAKE_LISTED tests, 0 benchmarks" ;;\n'
            '  test*) echo "test result: ok. $FAKE_PASSED passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s" ;;\n'
            "  *) echo '{}' ;;\n"
            "esac\n"
        )
        fake.chmod(0o755)
        fake_node = bindir / "node"   # reports an empty node suite, so cargo alone decides the total
        fake_node.write_text('#!/bin/sh\nprintf "# tests 0\\n# pass 0\\n# fail 0\\n# skipped 0\\n"\n')
        fake_node.chmod(0o755)

        def go(label, passed, listed, previous=None):
            gdir = tmp / label
            gdir.mkdir()
            if previous is not None:
                (gdir / "2026-01-01-probes.json").write_text(json.dumps({"suites": {"cargo": {"passed": previous, "failed": 0, "skipped": 0}}}))
            env = dict(os.environ, PATH=f"{bindir}:{os.environ['PATH']}", GROUND_DIR=str(gdir), GROUND_SKIP_WASM_VERIFY="1",
                       FAKE_PASSED=str(passed), FAKE_LISTED=str(listed))
            p = subprocess.run(["bash", str(ROOT / "scripts" / "ground.sh")], cwd=ROOT, env=env, capture_output=True, text=True, timeout=900)
            check(p.returncode == 0, f"{label}: ground.sh still exits 0")
            return json.loads(p.stdout)["errors"]

        errs = go("zero", 0, 0)
        check(any("validator: total tests is 0" in e for e in errs), "a run that executed zero tests is an error")
        errs = go("shrink", 3, 3, previous=9999)
        check(any("below the previous probes file (9999)" in e for e in errs), "a total below the previous probes file is an error")
        errs = go("disagree", 3, 5)
        check(any("names 5" in e for e in errs), "a summed count that disagrees with `cargo test -- --list` is an error")
        errs = go("agree", 3, 3)
        check(not any(e.startswith("suites.cargo") for e in errs) and not any("validator: total tests" in e for e in errs),
              "control: matching counts and no previous file give neither error (so the three above could have stayed quiet)")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_verify_live_dirty():
    print("verify-live-dirty: a dirty tree is refused")
    tmp = scratch("vldirty")
    probe = ROOT / "scripts" / ".dirty-probe"
    try:
        probe.write_text("untracked\n")
        env = dict(os.environ, GROUND_DIR=str(tmp / "ground"))
        p = subprocess.run(["bash", str(ROOT / "scripts" / "verify-live.sh"), "0119-dirty"], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
        check(p.returncode == 2, f"exit code is 2 (got {p.returncode})")
        check("refusing to run on a dirty tree" in p.stderr and ".dirty-probe" in p.stderr, "stderr names the dirty path")
        check(not (tmp / "ground" / "receipts").exists(), "no receipt was written")
    finally:
        probe.unlink(missing_ok=True)
        shutil.rmtree(tmp, ignore_errors=True)


TESTS = {"ground-nopath": test_ground_nopath, "ground-normal": test_ground_normal, "ground-validator": test_ground_validator, "verify-live-corrupt": test_verify_live_corrupt, "verify-live-dirty": test_verify_live_dirty}

if __name__ == "__main__":
    wanted = sys.argv[1:] or list(TESTS)
    for name in wanted:
        if name not in TESTS:
            print(f"unknown test {name}; choose from {', '.join(TESTS)}", file=sys.stderr)
            sys.exit(2)
        TESTS[name]()
    print("\n" + ("FAILED: " + "; ".join(FAILS) if FAILS else "all checks passed"))
    sys.exit(1 if FAILS else 0)
