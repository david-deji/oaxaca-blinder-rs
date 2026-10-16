#!/usr/bin/env python3
"""Tests for the release scripts: release-notes.py, check-release-tag.py, package-release.py, deploy-book.sh.

The release workflow runs these scripts for the first time on a real tag, so each one is exercised here against
good and bad input, and the CHANGELOG and the manifests are held to what the workflow will read.

    python3 scripts/test-release-scripts.py

Scratch files go under target/ (never /tmp) and are removed afterwards.
"""
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import types
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
FAILS = []


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)


def run(*args, cwd=None, env=None):
    p = subprocess.run([str(a) for a in args], capture_output=True, text=True, cwd=cwd, env=env)
    return p.returncode, p.stdout, p.stderr


def load(name):
    """Import a script by compiling its current text (never a cached .pyc, which can outlive an edit)."""
    path = SCRIPTS / f"{name}.py"
    mod = types.ModuleType(name.replace("-", "_"))
    mod.__file__ = str(path)
    exec(compile(path.read_text(encoding="utf-8"), str(path), "exec"), mod.__dict__)
    return mod


def crate_version():
    return tomllib.loads((ROOT / "oaxaca_blinder" / "Cargo.toml").read_text())["package"]["version"]


# ---------------------------------------------------------------- release-notes.py

CHANGELOG = """# Changelog

## [Unreleased]

## [1.2.0] - 2030-01-02

Summary line.

### Added
- thing one

## [1.2.0-rc.1] - 2030-01-01

### Added
- rc thing

## [1.1.0] - 2029-12-01
- older

## [1.0.0]

## [0.9.0]
- first
"""


def test_release_notes(scratch):
    print("release-notes.py")
    notes = SCRIPTS / "release-notes.py"
    cl = scratch / "CHANGELOG.md"
    cl.write_text(CHANGELOG)

    rc, out, _ = run(sys.executable, notes, "v1.2.0", "--changelog", cl)
    check(rc == 0 and out.startswith("Summary line.") and "thing one" in out, "a tag prints its section")
    check("rc thing" not in out and "older" not in out and "## [" not in out, "the section stops at the next heading")
    rc2, out2, _ = run(sys.executable, notes, "1.2.0", "--changelog", cl)
    check(rc2 == 0 and out2 == out, "the bare version prints the same section as the tag")
    rc, out, _ = run(sys.executable, notes, "v1.2.0-rc.1", "--changelog", cl)
    check(rc == 0 and "rc thing" in out and "thing one" not in out, "a pre-release tag prints only its own section")
    rc, out, _ = run(sys.executable, notes, "v1.1.0", "--changelog", cl)
    check(rc == 0 and out.strip() == "- older", "a one-line section is printed")
    for label, version, want in [
        ("an absent version", "v9.9.9", "no `## [9.9.9]` heading"),
        ("an empty section", "v1.0.0", "is empty"),
        ("a heading that is only a prefix of the version", "v1.2", "no `## [1.2]` heading"),
    ]:
        rc, out, err = run(sys.executable, notes, version, "--changelog", cl)
        check(rc == 1 and not out and want in err, f"{label} exits 1 with a reason")
    dup = scratch / "dup.md"
    dup.write_text(CHANGELOG + "\n## [1.1.0]\n- again\n")
    rc, _, err = run(sys.executable, notes, "v1.1.0", "--changelog", dup)
    check(rc == 1 and "2 `## [1.1.0]` headings" in err, "a repeated heading exits 1")
    rc, _, _ = run(sys.executable, notes)
    check(rc == 2, "no argument exits 2")

    # The real file: the version the workflow will tag has a section, and it is only that section.
    v = crate_version()
    rc, out, err = run(sys.executable, notes, f"v{v}")
    check(rc == 0, f"CHANGELOG.md has a non-empty `## [{v}]` section (the crate's version)" + ("" if rc == 0 else f": {err.strip()}"))
    check("## [" not in out and "Never published to crates.io" not in out, f"the printed {v} section holds no other release")
    check("Migrating from 0.2" in out and "Statistical changes" in out, f"the {v} section carries the migration and the statistical changes")


# ---------------------------------------------------------------- check-release-tag.py

def run_obj(name, conclusion, completed="2030-01-01T10:00:00Z", status="completed"):
    return {"name": name, "status": status, "conclusion": conclusion, "completed_at": completed, "started_at": completed}


def test_check_release_tag(scratch):
    print("check-release-tag.py")
    mod = load("check-release-tag")
    ok = [run_obj("gate", "success")]
    cases = [
        ("tag equals the version and gate passed", "v0.3.0", "0.3.0", ok, 0),
        ("a pre-release tag equal to a pre-release version", "v0.3.0-rc.1", "0.3.0-rc.1", ok, 0),
        ("a tag that differs from the version", "v0.3.1", "0.3.0", ok, 1),
        ("a tag without the v", "0.3.0", "0.3.0", ok, 1),
        ("a tag that is not a version", "vnext", "0.3.0", ok, 1),
        ("no gate check run", "v0.3.0", "0.3.0", [run_obj("quality", "success")], 1),
        ("no check runs at all", "v0.3.0", "0.3.0", [], 1),
        ("gate failed", "v0.3.0", "0.3.0", [run_obj("gate", "failure")], 1),
        ("gate cancelled", "v0.3.0", "0.3.0", [run_obj("gate", "cancelled")], 1),
        ("gate skipped", "v0.3.0", "0.3.0", [run_obj("gate", "skipped")], 1),
        ("gate still running", "v0.3.0", "0.3.0", [run_obj("gate", None, status="in_progress", completed="")], 1),
        ("an older failure then a newer success", "v0.3.0", "0.3.0",
         [run_obj("gate", "failure", "2030-01-01T09:00:00Z"), run_obj("gate", "success", "2030-01-01T10:00:00Z")], 0),
        ("an older success then a newer failure", "v0.3.0", "0.3.0",
         [run_obj("gate", "success", "2030-01-01T09:00:00Z"), run_obj("gate", "failure", "2030-01-01T10:00:00Z")], 1),
        ("another check named like gate does not count", "v0.3.0", "0.3.0", [run_obj("gate-check", "success")], 1),
    ]
    for label, tag, version, runs, want in cases:
        got = 1 if mod.check(tag, version, runs) else 0
        check(got == want, f"{label}: {'refused' if got else 'accepted'}")

    # Through the command line, with the answer of `gh api --paginate` (objects back to back).
    manifest = scratch / "Cargo.toml"
    manifest.write_text('[package]\nname = "x"\nversion = "0.3.0"\n')
    runs = scratch / "runs.json"
    runs.write_text(json.dumps({"total_count": 1, "check_runs": [run_obj("quality", "success")]}) + "\n" +
                    json.dumps({"total_count": 1, "check_runs": [run_obj("gate", "success")]}))
    rc, out, _ = run(sys.executable, SCRIPTS / "check-release-tag.py", "--tag", "v0.3.0", "--manifest", manifest, "--check-runs", runs)
    check(rc == 0 and "gate passed" in out, "paginated answers are read as one (gate found on the second page)")
    rc, out, _ = run(sys.executable, SCRIPTS / "check-release-tag.py", "--tag", "v0.4.0", "--manifest", manifest, "--check-runs", runs)
    check(rc == 1 and "does not equal v0.3.0" in out, "the command line exits 1 on a tag that differs from the manifest")
    runs.write_text("not json")
    rc, _, _ = run(sys.executable, SCRIPTS / "check-release-tag.py", "--tag", "v0.3.0", "--manifest", manifest, "--check-runs", runs)
    check(rc == 2, "an unreadable answer exits 2")


# ---------------------------------------------------------------- package-release.py

def test_package_release(scratch):
    print("package-release.py")
    mod = load("package-release")
    version = crate_version()
    for name, windows in [("linux-x86_64", False), ("windows-x86_64", True)]:
        bins = scratch / f"bin-{name}"
        bins.mkdir()
        suffix = ".exe" if windows else ""
        for b in ("oaxaca-cli", "meridian-mcp"):
            (bins / f"{b}{suffix}").write_bytes(b"\x7fELF" + b.encode())
        out = scratch / f"dist-{name}"
        rc, stdout, err = run(sys.executable, SCRIPTS / "package-release.py", "--name", name, "--out", out, "--bin-dir", bins)
        archive = Path(stdout.strip()) if stdout.strip() else None
        want = f"oaxaca-blinder-{version}-{name}" + (".zip" if windows else ".tar.gz")
        check(rc == 0 and archive is not None and archive.name == want and archive.is_file(), f"{name}: writes {want}")
        if not (archive and archive.is_file()):
            continue
        if windows:
            names = set(zipfile.ZipFile(archive).namelist())
        else:
            with tarfile.open(archive) as t:
                names = set(t.getnames())
                modes = {m.name: m.mode for m in t.getmembers()}
        stem = f"oaxaca-blinder-{version}-{name}"
        expect = {f"{stem}/oaxaca-cli{suffix}", f"{stem}/meridian-mcp{suffix}", f"{stem}/LICENSE-MIT", f"{stem}/LICENSE-APACHE", f"{stem}/README.md"}
        check(names == expect, f"{name}: holds exactly both binaries, both licences and the README (got {sorted(names)})")
        if not windows:
            check(modes[f"{stem}/oaxaca-cli"] == 0o755 and modes[f"{stem}/LICENSE-MIT"] == 0o644, f"{name}: binaries are executable, licences are not")
        sha = (out / (archive.name + ".sha256")).read_text()
        import hashlib
        check(sha == f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n", f"{name}: the .sha256 file matches the archive in sha256sum format")

    empty = scratch / "bin-empty"
    empty.mkdir()
    rc, _, err = run(sys.executable, SCRIPTS / "package-release.py", "--name", "linux-x86_64", "--out", scratch / "dist-x", "--bin-dir", empty)
    check(rc == 1 and "binary not found" in err and not (scratch / "dist-x").exists(), "a missing binary exits 1 and writes nothing")
    # a missing licence file
    fake = scratch / "fakeroot"
    (fake / "oaxaca_blinder").mkdir(parents=True)
    shutil.copy(ROOT / "oaxaca_blinder" / "Cargo.toml", fake / "oaxaca_blinder" / "Cargo.toml")
    for f in ("LICENSE-MIT", "README.md"):
        shutil.copy(ROOT / f, fake / f)
    try:
        mod.package("linux-x86_64", scratch / "dist-y", scratch / "bin-linux-x86_64", root=fake)
        check(False, "a missing LICENSE-APACHE is refused")
    except FileNotFoundError as e:
        check("LICENSE-APACHE" in str(e), "a missing LICENSE-APACHE is refused")


# ---------------------------------------------------------------- deploy-book.sh

def git(*args, cwd=None):
    p = subprocess.run(["git", *[str(a) for a in args]], capture_output=True, text=True, cwd=cwd)
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def test_deploy_book(scratch):
    print("deploy-book.sh")
    origin = scratch / "origin.git"
    git("init", "--bare", "--quiet", "-b", "main", origin)
    clone = scratch / "clone"
    git("clone", "--quiet", origin, clone)
    git("-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "--allow-empty", "-q", "-m", "init", cwd=clone)
    git("push", "-q", "origin", "HEAD:refs/heads/main", cwd=clone)

    def book(version):
        d = scratch / f"book-{version}"
        d.mkdir(exist_ok=True)
        (d / "index.html").write_text(f"book {version}")
        (d / "intro.html").write_text(f"intro {version}")
        return d

    def deploy(tag, book_dir=None):
        v = tag[1:]
        return run("bash", SCRIPTS / "deploy-book.sh", book_dir or book(v), tag, cwd=clone)

    def on_branch(path):
        rc, out, _ = git("--git-dir", origin, "show", f"gh-pages:{path}")
        return out if rc == 0 else None

    def files():
        _, out, _ = git("--git-dir", origin, "ls-tree", "-r", "--name-only", "gh-pages")
        return set(out.splitlines())

    rc, out, err = deploy("v0.3.0")
    check(rc == 0, f"v0.3.0 on a repository with no gh-pages branch creates it (exit {rc}) {err[-200:]}")
    check({"v0.3/index.html", "v0.3/intro.html", "latest/index.html", "index.html", ".nojekyll", ".latest-version"} <= files(),
          f"the first deploy writes v0.3/, latest/, the root redirect and .nojekyll (got {sorted(files())})")
    check(on_branch("latest/index.html") == "book 0.3.0" and on_branch(".latest-version") == "0.3.0", "latest/ is the 0.3.0 book")
    check('url=latest/' in (on_branch("index.html") or ""), "the root index.html redirects to latest/")
    rc, branches, _ = git("--git-dir", origin, "for-each-ref", "--format=%(refname:short)")
    check(set(branches.split()) == {"main", "gh-pages"}, f"only gh-pages was added to the remote (branches: {branches.split()})")

    rc, _, err = deploy("v0.4.0")
    check(rc == 0, f"v0.4.0 deploys (exit {rc}) {err[-200:]}")
    check(on_branch("v0.3/index.html") == "book 0.3.0", "the older version's pages stay (v0.3/ still holds the 0.3.0 book)")
    check(on_branch("v0.4/index.html") == "book 0.4.0" and on_branch("latest/index.html") == "book 0.4.0", "v0.4/ and latest/ hold the 0.4.0 book")

    rc, _, _ = deploy("v0.3.1")
    check(rc == 0 and on_branch("v0.3/index.html") == "book 0.3.1", "a patch release replaces its own line's pages")
    check(on_branch("latest/index.html") == "book 0.4.0" and on_branch(".latest-version") == "0.4.0", "an older line does not take latest/ back")

    rc, _, _ = deploy("v0.5.0-rc.1")
    check(rc == 0 and on_branch("v0.5/index.html") == "book 0.5.0-rc.1", "a pre-release gets its own directory")
    check(on_branch("latest/index.html") == "book 0.4.0", "a pre-release does not take latest/")

    rc, out, _ = deploy("v0.3.1")
    check(rc == 0 and "nothing changed" in out, "deploying the same book again changes nothing")

    rc, _, err = deploy("v0.3")
    check(rc == 1 and "not a release tag" in err, "a tag that is not vMAJOR.MINOR.PATCH exits 1")
    rc, _, err = run("bash", SCRIPTS / "deploy-book.sh", scratch / "no-such-book", "v0.6.0", cwd=clone)
    check(rc == 1 and "does not exist" in err, "a book directory without index.html exits 1")


def main():
    base = ROOT / "target" / "0121-tests"
    base.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="release-scripts-", dir=base))
    try:
        test_release_notes(scratch)
        test_check_release_tag(scratch)
        test_package_release(scratch)
        test_deploy_book(scratch)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    print("\n" + ("FAILED: " + "; ".join(FAILS) if FAILS else "all release-script checks passed"))
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
