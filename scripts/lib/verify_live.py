#!/usr/bin/env python3
"""verify_live.py: the work behind scripts/verify-live.sh (0119-MERIDIAN S6).

"Live" for the engine means: the blobs the browser will load are the blobs this source produces.
Contract: internal-ops-bureau/knowledge/dev-loop.md (telos-machina) section "verify-live.sh contract".
Writes ground/receipts/<epic>-live.json; the receipt, not the exit code alone, is the definition of done.

Order, and why:
  0. Refuse a dirty tree: a receipt must describe a commit, and HEAD says so only on a clean tree.
  1. wasm_verify        build-wasm.sh --verify: raw blobs rebuilt from this source equal HEAD's baselines.
  2. published_vs_pkg   the app's published files equal engine/pkg byte for byte, their manifests agree,
                        and each manifest's raw sha256 is the raw blob step 1 just built. This is the
                        check that a hash written by the same run cannot fake: it compares files the
                        app will load with bytes built a moment ago. A failure here stops the run, because
                        every check below would be about a different blob than the one that ships.
  3. app_real_blob_specs  the app's specs that execute the SHIPPED blobs under node.
  4. app_verify_live      the app's own scripts/verify-live.sh <epic>. Its receipt is embedded only when
                        its ran_at is after this run started and its commit is the app's HEAD.
  5. app_tree_committed   the app tree that was tested is committed: the app receipt was not made from a dirty
                        tree, nothing under the published directories is uncommitted, and each published
                        engine-manifest.json is tracked by git (the sequential one sits in a directory whose
                        .gitignore is `*`, so a plain `git add` skips it).
Every nested exit code is checked: a non-zero exit fails its check whatever the receipt says.
Nothing here copies or publishes anything.

The receipt is committed to a public repository: it holds counts, flags and hashes, never an absolute path
of this machine, the app's file names, or its git status lines (0119 review OPS-2).
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GROUND = Path(os.environ.get("GROUND_DIR") or ROOT / "ground")
APP = Path(os.environ.get("MERIDIAN_APP") or ROOT.parent / "pay-equity-app")
FRONTEND_SRC = Path(os.environ.get("MERIDIAN_FRONTEND") or APP / "frontend" / "src")
ENV = dict(os.environ)
ENV.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")

SEQ_FILES = ["package.json", "pay_equity_engine.js", "pay_equity_engine.d.ts", "pay_equity_engine_bg.wasm",
             "pay_equity_engine_bg.wasm.d.ts", "engine-manifest.json"]
THR_FILES = list(SEQ_FILES)
ARTIFACTS = [
    ("sequential", ROOT / "engine" / "pkg", FRONTEND_SRC / "wasm", SEQ_FILES, False, "sequential"),
    ("threaded", ROOT / "engine" / "pkg-threaded", FRONTEND_SRC / "wasm-threaded", THR_FILES, True, "threaded"),
]


def now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha(path: Path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def run(cmd: list, timeout: int, cwd: Path = ROOT) -> tuple:
    """(rc, combined output). rc 127 when the binary is missing, 124 on timeout: both are failures, never skips."""
    if shutil.which(cmd[0], path=ENV.get("PATH")) is None:
        return 127, f"`{cmd[0]}` not found on PATH"
    try:
        p = subprocess.run(cmd, cwd=str(cwd), env=ENV, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return 124, f"timed out after {timeout}s"
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def git(args: list, cwd: Path = ROOT) -> str:
    rc, out = run(["git"] + args, 30, cwd)
    # rstrip("\n") only: `git status --porcelain` lines begin with a space, which strip() would eat
    return out.rstrip("\n") if rc == 0 else ""


def inside_app(path: Path, app: Path = None) -> bool:
    try:
        return path.resolve().is_relative_to((app or APP).resolve())
    except OSError:
        return False


def scrub(text: str) -> str:
    """Replace this machine's absolute paths in text that is about to be committed."""
    for real, label in ((str(FRONTEND_SRC), "<app>/frontend/src"), (str(APP), "<app>"), (str(ROOT), "<engine>"),
                        (str(ROOT.parent), "<workspace>"), (str(Path.home()), "~")):
        if real and real != "/":
            text = text.replace(real, label)
    return text


def app_receipt_summary(rec):
    """What of the app's own receipt may be committed here: results and counts, not its file names."""
    if rec is None:
        return None
    tt = rec.get("tested_tree") or {}
    return {
        "epic": rec.get("epic"), "ran_at": rec.get("ran_at"), "commit": rec.get("commit"), "result": rec.get("result"),
        "tested_tree": {"base_commit": tt.get("base_commit"), "dirty": tt.get("dirty"), "file_count": len(tt.get("files") or {})},
        "checks": [{"name": c.get("name"), "result": c.get("result")} for c in rec.get("checks") or []],
    }


def app_tree_problems(app: Path, frontend_src: Path, app_receipt, halted_at):
    """(problems, note) for check 5. `app` is the app checkout, `frontend_src` the directory published into."""
    problems = []
    note = None
    if not app.is_dir():
        problems.append("app checkout not found")
    elif not inside_app(frontend_src, app):
        problems.append("the published directory is outside the app checkout (MERIDIAN_FRONTEND), so its tracking in git cannot be checked")
    else:
        rel_src = frontend_src.resolve().relative_to(app.resolve())
        pub_rel = [str(rel_src / "wasm"), str(rel_src / "wasm-threaded")]
        uncommitted = [l for l in git(["status", "--porcelain", "--untracked-files=all", "--", *pub_rel, "scripts"], app).splitlines() if l]
        if uncommitted:
            problems.append(f"{len(uncommitted)} uncommitted entries under the published directories and the app's scripts/ (commit them, then rerun)")
        for rel in pub_rel:
            m = f"{rel}/engine-manifest.json"
            rc, _ = run(["git", "ls-files", "--error-unmatch", "--", m], 30, app)
            if rc != 0:
                ignored = run(["git", "check-ignore", "-q", "--", m], 30, app)[0] == 0
                problems.append(f"{m} is not tracked by git" + (" (matched by a .gitignore: `git add -f` it, or negate it in a tracked ignore rule)" if ignored else ""))
        note = "each published engine-manifest.json is tracked"
    if app_receipt is not None and (app_receipt.get("tested_tree") or {}).get("dirty") is not False:
        problems.append("the app receipt was made from a dirty app tree (tested_tree.dirty is not false)")
    elif app_receipt is None and halted_at is None:
        problems.append("no app receipt to read the tested tree from")
    return problems, note


def ci_checks_0120(add, verified, published):
    """0120-MERIDIAN: the CI the commit this receipt branches from, and the baselines CI recorded for it.

    `main_ci_green_on_base`  the CI run on the commit this branch was cut from (git merge-base with origin/main)
                             finished with every job success except the non-blocking benchmark, `gate` included.
    `ci_wasm_baselines`      the raw blobs that run built (its wasm-raw-seq and wasm-raw-threaded artifacts) equal
                             the baselines committed at that commit, the blobs built here, and the raw hash in
                             each manifest the app ships. Three independent builds, one hash.
    Needs the gh CLI and network; a missing gh is a failed check, never a skip.
    """
    base = git(["merge-base", "HEAD", "origin/main"])
    if not base:
        add("main_ci_green_on_base", False, "no merge base with origin/main (run `git fetch origin`)")
        add("ci_wasm_baselines", False, "not run: no merge base with origin/main")
        return
    rc, out = run(["gh", "run", "list", "--workflow", "CI", "--branch", "main", "--commit", base, "--json",
                   "databaseId,status,conclusion,headSha,event", "--limit", "10"], 60)
    runs = []
    try:
        runs = [r for r in json.loads(out) if r.get("headSha") == base and r.get("event") == "push"]
    except ValueError:
        pass
    if rc != 0 or not runs:
        add("main_ci_green_on_base", False, f"no CI push run found on main for {base[:8]} (gh exit {rc}): {out.strip()[:160]}")
        add("ci_wasm_baselines", False, "not run: no CI run to read the baselines from")
        return
    run_id = str(runs[0]["databaseId"])
    rc, out = run(["gh", "run", "view", run_id, "--json", "status,conclusion,jobs"], 60)
    try:
        view = json.loads(out)
    except ValueError:
        view = None
    if rc != 0 or not view:
        add("main_ci_green_on_base", False, f"could not read run {run_id} (gh exit {rc}): {out.strip()[:160]}")
        add("ci_wasm_baselines", False, "not run: the CI run could not be read")
        return
    jobs = {j["name"]: (j.get("conclusion") or j.get("status")) for j in view["jobs"]}
    gate = jobs.get("gate")
    bad = {n: c for n, c in jobs.items() if c != "success" and not n.startswith("Benchmark")}
    ok = view.get("status") == "completed" and view.get("conclusion") == "success" and gate == "success" and not bad
    add("main_ci_green_on_base", ok,
        f"CI run {run_id} on main at {base[:8]}: run {view.get('status')}/{view.get('conclusion')}, gate {gate}, {len(jobs)} jobs"
        + ("" if not bad else f"; not success: {bad}"),
        ci_run_id=int(run_id), ci_commit=base, ci_jobs=jobs)

    dest = ROOT / "target" / "ci-artifacts" / run_id
    shas, problems = {}, []
    for key, art, fname in (("sequential", "wasm-raw-seq", "raw-sha256.txt"), ("threaded", "wasm-raw-threaded", "raw-sha256.txt")):
        d = dest / f"{art}-{int(time.time())}"   # gh refuses to extract over an earlier download
        d.mkdir(parents=True, exist_ok=True)
        rc, out = run(["gh", "run", "download", run_id, "-n", art, "-D", str(d)], 120)
        f = d / fname
        if rc != 0 or not f.is_file():
            problems.append(f"{key}: artifact {art} not downloaded (gh exit {rc}): {out.strip()[:120]}")
            continue
        shas[key] = f.read_text().split()[0]
    baseline = {"sequential": git(["show", f"{base}:engine/pay_equity_engine.wasm.sha256"]).split()[:1],
                "threaded": git(["show", f"{base}:engine/pay_equity_engine.threaded.wasm.sha256"]).split()[:1]}
    rows = {}
    for key in ("sequential", "threaded"):
        want = (baseline[key] or [None])[0]
        ci = shas.get(key)
        local = ((verified or {}).get(key) or {}).get("built")
        shipped = (published.get(key) or {}).get("manifest_raw")
        rows[key] = {"ci_raw_sha256": ci, "baseline_at_base": want, "built_here": local, "shipped_manifest_raw": shipped}
        if not (ci and want and local and shipped and ci == want == local == shipped):
            problems.append(f"{key}: ci {str(ci)[:12]} baseline {str(want)[:12]} built-here {str(local)[:12]} shipped-manifest {str(shipped)[:12]} are not one hash")
    add("ci_wasm_baselines", not problems,
        "the raw blobs CI built, the baselines committed at that commit, the blobs built here and the manifests the app ships are one hash per artifact: "
        + "; ".join(f"{k} {v['ci_raw_sha256'][:12]}" for k, v in rows.items() if v["ci_raw_sha256"])
        if not problems else "; ".join(problems)[:1200],
        hashes=rows)


def probe_checks_0120(add, frontend_src):
    """0120-MERIDIAN: the shipped blobs, run in node on the engine's own 10 000-row employers file."""
    rc, out = run(["node", "scripts/lib/shipped_blob_probe.mjs", str(frontend_src)], 600)
    try:
        probe = json.loads(out.strip().splitlines()[-1])
    except (ValueError, IndexError):
        probe = None
    names = [("relabel", "shipped_blob_relabel_invariance"), ("carve_out", "shipped_blob_carve_out_invariance"),
             ("t8_zero_budget_identity", "shipped_blob_t8_zero_budget_identity")]
    for key, name in names:
        if probe is None or rc != 0:
            add(name, False, f"the probe wrote no result (exit {rc}): {out.strip()[-200:]}")
            continue
        parts, ok = [], True
        for art, r in probe["artifacts"].items():
            c = (r.get("checks") or {}).get(key)
            if c is None:
                ok = False
                parts.append(f"{art}: {r.get('error', 'no result')[:160]}")
                continue
            ok = ok and c.get("ok") is True
            teeth = c.get("comparator_sees_a_nudge", c.get("comparator_sees_a_shift"))
            ok = ok and teeth is True
            if key == "relabel":
                parts.append(f"{art}: {c['rows']} Department rows map one to one after Engineering becomes zz_Engineering, worst difference {c['worst_abs_difference']:.2e} (limit 1e-9), comparator catches a 0.01 nudge: {teeth}")
            elif key == "carve_out":
                parts.append(f"{art}: 20 Engineering rows moved to a new department Legal, {c['untouched_departments']} untouched departments move at most {c['worst_untouched_move']:.2e} (limit 1e-4), comparator catches a 0.01 nudge: {teeth}")
            else:
                p, r2 = c["pooled"], c["reference"]
                parts.append(f"{art}: Pooled optimiser starts at {p['optimize_pooled_original']:.9f}, decomposition {p['decompose_pooled']:.9f}, independent group-indicator fit {p['independent_group_indicator_coefficient']:.9f}; "
                             f"Reference optimiser {r2['optimize_reference_original']:.9f}, GroupB decomposition {r2['decompose_groupb']:.9f}, independent fit {r2['independent_mean_shortfall_against_reference_line']:.9f}; comparator catches a shift: {teeth}")
        add(name, ok, " | ".join(parts)[:1500])


def main() -> int:
    if len(sys.argv) < 2 or not sys.argv[1]:
        print("usage: bash scripts/verify-live.sh <epic-id>", file=sys.stderr)
        return 2
    epic = sys.argv[1]
    started_ts = time.time()
    started_at = now()

    # 0. dirty tree: tracked changes anywhere, or untracked files outside ground/ (this script's own output)
    porcelain = [l for l in git(["status", "--porcelain", "--untracked-files=all"]).splitlines() if l]
    dirty = [l for l in porcelain if not l[3:].startswith("ground/")]
    if dirty:
        print("verify-live: refusing to run on a dirty tree (a receipt must describe a commit). Commit or stash:", file=sys.stderr)
        for l in dirty[:15]:
            print("  " + l, file=sys.stderr)
        return 2

    commit_full = git(["rev-parse", "HEAD"])
    checks: list = []
    app_receipt = None
    app_receipt_note = "not run"
    halted_at = None

    def add(name, ok, detail, **extra):
        checks.append({"name": name, "result": "pass" if ok else "fail", "detail": detail, **extra})
        return ok

    # 1. wasm_verify
    verify_json = ROOT / "target" / "wasm-verify.json"
    if verify_json.exists():
        verify_json.unlink()  # a stale result must not be readable as this run's
    print("verify-live: build-wasm.sh --verify", file=sys.stderr, flush=True)
    rc, out = run(["bash", "scripts/build-wasm.sh", "--verify"], int(os.environ.get("VERIFY_LIVE_WASM_TIMEOUT", "2400")))
    verified = None
    if verify_json.exists():
        try:
            verified = json.loads(verify_json.read_text())
        except ValueError:
            verified = None
    ok = rc == 0 and bool(verified) and verified.get("match") is True and verified.get("commit") == commit_full
    if ok:
        detail = f"raw sequential {verified['sequential']['built'][:12]} and threaded {verified['threaded']['built'][:12]} equal HEAD's baselines (exit 0)"
    elif verified is None:
        detail = f"build-wasm.sh --verify wrote no result (exit {rc}): {' '.join(out.strip().splitlines()[-2:])[:240]}"
    else:
        detail = f"exit {rc}; match={verified.get('match')}; commit {str(verified.get('commit'))[:8]} vs HEAD {commit_full[:8]}"
    add("wasm_verify", ok, detail, exit_code=rc)

    # 2. published_vs_pkg (runs even when step 1 failed: it is cheap, and its facts help read that failure)
    problems: list = []
    facts: dict = {}
    for name, pkg, pub, files, has_snippets, key in ARTIFACTS:
        if not pkg.is_dir():
            problems.append(f"{name}: {pkg} does not exist, so there is no build output to compare (run scripts/build-wasm.sh)")
            continue
        if not pub.is_dir():
            problems.append(f"{name}: published directory {pub} does not exist")
            continue
        for f in files:
            a, b = sha(pkg / f), sha(pub / f)
            if a is None:
                problems.append(f"{name}: engine pkg lacks {f}")
            elif b is None:
                problems.append(f"{name}: published copy lacks {f}")
            elif a != b:
                problems.append(f"{name}: {f} differs between engine pkg ({a[:12]}) and the published copy ({b[:12]})")
        if has_snippets:
            ps = {str(p.relative_to(pkg / "snippets")): sha(p) for p in (pkg / "snippets").rglob("*") if p.is_file()} if (pkg / "snippets").is_dir() else {}
            us = {str(p.relative_to(pub / "snippets")): sha(p) for p in (pub / "snippets").rglob("*") if p.is_file()} if (pub / "snippets").is_dir() else {}
            if ps != us:
                problems.append(f"{name}: snippets/ differ between engine pkg and the published copy ({sorted(set(ps) ^ set(us)) or 'content'})")
        try:
            m = json.loads((pub / "engine-manifest.json").read_text())
        except (OSError, ValueError):
            problems.append(f"{name}: published engine-manifest.json missing or not JSON")
            continue
        blob_sha = sha(pub / "pay_equity_engine_bg.wasm")
        if blob_sha != m.get("bg_wasm_sha256"):
            problems.append(f"{name}: published blob {str(blob_sha)[:12]} is not the manifest's bg_wasm_sha256 {str(m.get('bg_wasm_sha256'))[:12]}")
        if m.get("engine_dirty") is not False:
            problems.append(f"{name}: manifest was written from a dirty engine tree")
        built = ((verified or {}).get(key) or {}).get("built")
        if built is None:
            problems.append(f"{name}: no freshly built raw sha256 to compare the manifest with (step wasm_verify produced none)")
        elif m.get("raw_sha256") != built:
            problems.append(f"{name}: manifest raw sha256 {str(m.get('raw_sha256'))[:12]} is not the raw blob just built from this source ({built[:12]})")
        facts[name] = {"published_bg_wasm": blob_sha, "manifest_raw": m.get("raw_sha256"), "manifest_commit": m.get("engine_commit")}
    pub_ok = add(
        "published_vs_pkg",
        not problems,
        "published blobs, glue and manifests equal engine pkg byte for byte, and each manifest's raw sha256 equals the raw blob built from this source"
        if not problems else "; ".join(problems)[:1500],
        published_dir="frontend/src of the app checkout" if inside_app(FRONTEND_SRC) else "an override outside the app checkout (MERIDIAN_FRONTEND)",
        artifacts=facts,
    )
    if not (checks[0]["result"] == "pass" and pub_ok):
        halted_at = "wasm_verify" if checks[0]["result"] != "pass" else "published_vs_pkg"

    # 3 + 4. the app's checks, only against blobs that passed 1 and 2
    app_head = git(["rev-parse", "--short", "HEAD"], APP) if APP.is_dir() else ""
    if halted_at is None:
        if not (APP / "frontend").is_dir():
            add("app_real_blob_specs", False, f"app checkout not found at {APP} (set MERIDIAN_APP)")
            add("app_verify_live", False, f"app checkout not found at {APP} (set MERIDIAN_APP)")
        else:
            report = ROOT / "target" / "app-realblob-report.json"
            report.parent.mkdir(exist_ok=True)
            if report.exists():
                report.unlink()
            specs = ["src/stores/__tests__/enginePayloadContract.spec.js", "src/stores/__tests__/engineRemedyRealBlob.spec.js"]
            print("verify-live: app real-blob specs", file=sys.stderr, flush=True)
            rc, out = run(["npx", "vitest", "run", "--config", "vitest.sqlite.config.mjs", "--maxWorkers=1", "--reporter=json",
                           f"--outputFile={report}", *specs], 900, APP / "frontend")
            counts = {}
            try:
                r = json.loads(report.read_text())
                counts = {"passed": r.get("numPassedTests"), "failed": r.get("numFailedTests"), "pending": r.get("numPendingTests")}
            except (OSError, ValueError):
                pass
            # zero passed, or any skipped, would be a green exit that tested nothing
            ok = rc == 0 and counts.get("passed", 0) > 0 and counts.get("failed") == 0 and counts.get("pending") == 0
            add("app_real_blob_specs", ok,
                f"exit {rc}; {counts or 'no vitest report was written: ' + ' '.join(out.strip().splitlines()[-2:])[:200]}", exit_code=rc, **({"counts": counts} if counts else {}))

            print("verify-live: app scripts/verify-live.sh (browser walk; slow)", file=sys.stderr, flush=True)
            app_json = APP / "ground" / "receipts" / f"{epic}-live.json"
            rc, out = run(["bash", "scripts/verify-live.sh", epic], int(os.environ.get("VERIFY_LIVE_APP_TIMEOUT", "3600")), APP)
            try:
                rec = json.loads(app_json.read_text())
            except (OSError, ValueError):
                rec = None
            reasons = []
            if rec is None:
                reasons.append("the app wrote no receipt")
            else:
                ran = rec.get("ran_at")
                try:
                    fresh = datetime.fromisoformat(ran.replace("Z", "+00:00")).timestamp() >= started_ts - 1
                except (AttributeError, ValueError):
                    fresh = False
                if not fresh:
                    reasons.append(f"its ran_at {ran} is not after this run started ({started_at})")
                if rec.get("commit") != app_head:
                    reasons.append(f"its commit {rec.get('commit')} is not the app HEAD {app_head}")
            if rec is not None and not reasons:
                app_receipt = rec
                app_receipt_note = "embedded: ran_at after this run's start and commit equals the app HEAD"
            else:
                app_receipt_note = "not embedded: " + "; ".join(reasons)
            ok = rc == 0 and app_receipt is not None and app_receipt.get("result") == "pass"
            add("app_verify_live", ok,
                f"exit {rc}; app receipt {app_receipt.get('result') if app_receipt else 'unusable'} ({app_receipt_note})"
                + ("" if rc == 0 else "; " + " ".join(out.strip().splitlines()[-2:])[:200]), exit_code=rc)
    else:
        for n in ("app_real_blob_specs", "app_verify_live"):
            checks.append({"name": n, "result": "not_run", "detail": f"not run: {halted_at} failed, so the app's checks would describe a different blob than the one that ships"})

    # 5. the tested app tree is committed (a receipt that says `pass` for a tree that exists in no commit proves nothing)
    problems, tracked_note = app_tree_problems(APP, FRONTEND_SRC, app_receipt, halted_at)
    add("app_tree_committed", not problems,
        "the app receipt's tree is clean, nothing under the published directories or scripts/ is uncommitted, " + (tracked_note or "")
        if not problems else "; ".join(problems)[:1200])

    # 6. 0120-MERIDIAN: CI on the base commit, the baselines it recorded, and the shipped blobs run in node
    if epic.startswith("0120"):
        print("verify-live: 0120 checks (CI on the base commit, shipped-blob probe)", file=sys.stderr, flush=True)
        ci_checks_0120(add, verified, facts)
        probe_checks_0120(add, FRONTEND_SRC)

    app_clean = None
    app_uncommitted = None
    if APP.is_dir():
        app_status = git(["status", "--porcelain"], APP).splitlines()
        app_uncommitted = len([l for l in app_status if l])
        app_clean = app_uncommitted == 0
    overall = all(c["result"] == "pass" for c in checks)
    receipt = {
        "epic": epic,
        "started_at": started_at,
        "ran_at": now(),
        "commit": git(["rev-parse", "--short", "HEAD"]),
        "commit_full": commit_full,
        "result": "pass" if overall else "fail",
        "halted_at": halted_at,
        "copied_or_published": False,
        "checks": checks,
        "app_receipt": app_receipt_summary(app_receipt),
        "app_receipt_note": app_receipt_note,
        "app_git_status": {"head": app_head or None, "clean": app_clean, "uncommitted_entries": app_uncommitted},
        "engine_git_status": porcelain,
    }
    rdir = GROUND / "receipts"
    rdir.mkdir(parents=True, exist_ok=True)
    path = rdir / f"{epic}-live.json"
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(scrub(json.dumps(receipt, indent=2)) + "\n")
    os.replace(tmp, path)
    print(json.dumps({k: receipt[k] for k in ("epic", "ran_at", "commit", "result", "halted_at")} | {"checks": [(c["name"], c["result"]) for c in checks]}, indent=2))
    print(f"receipt written: {path}", file=sys.stderr)
    return 0 if overall else 1


if __name__ == "__main__":
    sys.exit(main())
