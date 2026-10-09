#!/usr/bin/env python3
"""Tests for scripts/ground.sh and scripts/verify-live.sh (0119-MERIDIAN S6, verification V6).

    python3 scripts/test-loop-scripts.py                 # all
    python3 scripts/test-loop-scripts.py ground-nopath   # one of: ground-nopath ground-normal ground-validator verify-live-corrupt verify-live-dirty verify-live-app-tree verify-live-scrub

Each test writes its output under target/ (never /tmp: disk is tight) and removes it afterwards.
  ground-nopath         ground.sh with `gh` and `cargo` removed from PATH exits 0, and errors[] names both.
  ground-normal         ground.sh on a normal run: every contract key present, suites total > 0.
  verify-live-corrupt   verify-live.sh against a COPY of the published blobs with one byte flipped: exits 1,
                        fails at published_vs_pkg, runs none of the app's checks, copies nothing.
  verify-live-app-tree  the app_tree_committed check, against a scratch git repo standing in for the app: a clean
                        committed tree passes; an ignored manifest, an uncommitted blob and a dirty app receipt
                        each fail with their own message (0119 review F3, OPS-3). Needs git only.
  verify-live-scrub     the committed receipt holds no absolute path, no app file names and no git status lines
                        (0119 review OPS-2). Needs python only.
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


def load_verify_live(env):
    """Import scripts/lib/verify_live.py with MERIDIAN_APP / MERIDIAN_FRONTEND set to a scratch app."""
    import importlib.util
    for k, v in env.items():
        os.environ[k] = v
    spec = importlib.util.spec_from_file_location("verify_live_under_test", ROOT / "scripts" / "lib" / "verify_live.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def git_in(cwd, *args):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True,
                          env=dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.invalid", GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.invalid"))


def test_verify_live_app_tree():
    print("verify-live-app-tree: the tested app tree must be committed, manifests tracked")
    tmp = scratch("apptree")
    saved = {k: os.environ.get(k) for k in ("MERIDIAN_APP", "MERIDIAN_FRONTEND")}
    try:
        app = tmp / "app"
        fe = app / "frontend" / "src"
        for d in ("wasm", "wasm-threaded"):
            (fe / d).mkdir(parents=True)
            (fe / d / "pay_equity_engine_bg.wasm").write_bytes(b"blob-" + d.encode())
            (fe / d / "engine-manifest.json").write_text("{}")
        (fe / "wasm" / ".gitignore").write_text("*\n!.gitignore\n")   # like the app's: the manifest is ignored
        (app / "scripts").mkdir()
        (app / "scripts" / "gate.mjs").write_text("// gate\n")
        git_in(app, "init", "-q")
        git_in(app, "add", "-A")
        git_in(app, "add", "-f", "frontend/src/wasm/pay_equity_engine_bg.wasm")   # the app force-adds its blobs; the manifest is the one left out
        git_in(app, "commit", "-q", "-m", "init")
        vl = load_verify_live({"MERIDIAN_APP": str(app), "MERIDIAN_FRONTEND": str(fe)})
        clean_receipt = {"tested_tree": {"dirty": False}}

        problems, _ = vl.app_tree_problems(app, fe, clean_receipt, None)
        check(any("wasm/engine-manifest.json is not tracked" in p and ".gitignore" in p for p in problems),
              "a manifest matched by a .gitignore and never added is reported, naming the ignore")
        check(not any("wasm-threaded/engine-manifest.json" in p for p in problems), "the tracked threaded manifest is not reported")

        git_in(app, "add", "-f", "frontend/src/wasm/engine-manifest.json")
        git_in(app, "commit", "-q", "-m", "manifest")
        problems, note = vl.app_tree_problems(app, fe, clean_receipt, None)
        check(problems == [] and note, f"committed blobs and tracked manifests pass (problems: {problems})")

        (fe / "wasm" / "pay_equity_engine_bg.wasm").write_bytes(b"changed")
        problems, _ = vl.app_tree_problems(app, fe, clean_receipt, None)
        check(any("uncommitted entries" in p for p in problems), "an uncommitted blob is reported")
        git_in(app, "checkout", "--", "frontend/src/wasm/pay_equity_engine_bg.wasm")

        (app / "scripts" / "gate.mjs").write_text("// edited\n")
        problems, _ = vl.app_tree_problems(app, fe, clean_receipt, None)
        check(any("uncommitted entries" in p for p in problems), "an uncommitted edit in the app's scripts/ is reported")
        git_in(app, "checkout", "--", "scripts/gate.mjs")

        problems, _ = vl.app_tree_problems(app, fe, {"tested_tree": {"dirty": True}}, None)
        check(any("dirty app tree" in p for p in problems), "an app receipt made from a dirty tree is reported")
        problems, _ = vl.app_tree_problems(app, fe, {"tested_tree": {}}, None)
        check(any("dirty app tree" in p for p in problems), "an app receipt that does not say the tree was clean is reported")
        problems, _ = vl.app_tree_problems(app, fe, None, None)
        check(any("no app receipt" in p for p in problems), "no app receipt is reported when the run was not halted")
        problems, _ = vl.app_tree_problems(app, fe, None, "published_vs_pkg")
        check(not any("no app receipt" in p for p in problems), "no app receipt is not an extra complaint when an earlier check halted the run")
        problems, _ = vl.app_tree_problems(app, tmp / "elsewhere" / "src", clean_receipt, None)
        check(any("outside the app checkout" in p for p in problems), "a published directory outside the app checkout cannot pass")
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(tmp, ignore_errors=True)


def test_verify_live_scrub():
    print("verify-live-scrub: nothing machine-specific or app-private in the committed receipt")
    vl = load_verify_live({"MERIDIAN_APP": "/home/someone/ws/pay-equity-app", "MERIDIAN_FRONTEND": "/home/someone/ws/pay-equity-app/frontend/src"})
    text = vl.scrub(json.dumps({"a": "/home/someone/ws/pay-equity-app/frontend/src/wasm", "b": "/home/someone/ws/pay-equity-app", "c": str(ROOT / "engine"), "d": str(Path.home()) + "/x"}))
    check("/home/someone" not in text and str(ROOT) not in text and str(Path.home()) not in text, f"absolute paths are replaced ({text[:140]})")
    summary = vl.app_receipt_summary({
        "epic": "0119", "ran_at": "t", "commit": "abc", "result": "pass",
        "tested_tree": {"base_commit": "abc", "dirty": True, "files": {"frontend/src/secret-name.spec.js": "sha"}},
        "checks": [{"name": "render_sweep", "result": "pass", "detail": "private detail /home/x"}],
    })
    flat = json.dumps(summary)
    check("secret-name" not in flat and "private detail" not in flat, "the app receipt summary drops file names and details")
    check(summary["tested_tree"] == {"base_commit": "abc", "dirty": True, "file_count": 1} and summary["checks"] == [{"name": "render_sweep", "result": "pass"}],
          "the summary keeps results, the dirty flag and counts")
    check(vl.app_receipt_summary(None) is None, "no app receipt stays None")


TESTS = {"ground-nopath": test_ground_nopath, "ground-normal": test_ground_normal, "ground-validator": test_ground_validator, "verify-live-corrupt": test_verify_live_corrupt, "verify-live-dirty": test_verify_live_dirty, "verify-live-app-tree": test_verify_live_app_tree, "verify-live-scrub": test_verify_live_scrub}

if __name__ == "__main__":
    wanted = sys.argv[1:] or list(TESTS)
    for name in wanted:
        if name not in TESTS:
            print(f"unknown test {name}; choose from {', '.join(TESTS)}", file=sys.stderr)
            sys.exit(2)
        TESTS[name]()
    print("\n" + ("FAILED: " + "; ".join(FAILS) if FAILS else "all checks passed"))
    sys.exit(1 if FAILS else 0)
