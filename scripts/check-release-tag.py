#!/usr/bin/env python3
"""Refuse a release tag that does not match the crate or whose commit did not pass `gate`.

    python3 scripts/check-release-tag.py --tag v0.3.0 --manifest oaxaca_blinder/Cargo.toml --check-runs check-runs.json

`--check-runs` is the answer of `gh api repos/OWNER/REPO/commits/SHA/check-runs?check_name=gate --paginate`
(one JSON object, or several back to back). Checked, in this order:
  * the tag is `v` plus a semantic version,
  * it equals `v` plus the `version` in the manifest of the published crate,
  * a check run named `gate` exists for the tagged commit, and the latest one finished with `success`.

The release workflow runs this first; nothing is built or published when it fails.
Exit 0 = the tag may be released, 1 = it may not (reason on stdout), 2 = could not run.
"""
import json
import re
import sys
import tomllib
from pathlib import Path

TAG_RE = re.compile(r"^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$")


def read_objects(text):
    """One JSON object, or several concatenated (what `gh api --paginate` prints)."""
    dec = json.JSONDecoder()
    i, out = 0, []
    while i < len(text):
        if text[i].isspace():
            i += 1
            continue
        obj, i = dec.raw_decode(text, i)
        out.append(obj)
    return out


def check(tag, crate_version, check_runs, gate_name="gate"):
    """Return a list of reasons the tag is refused (empty = fine)."""
    problems = []
    m = TAG_RE.match(tag)
    if not m:
        problems.append(f"tag {tag!r} is not `v` plus a semantic version")
    elif m.group(1) != crate_version:
        problems.append(f"tag {tag} does not equal v{crate_version}, the version in the manifest")
    gates = [r for r in check_runs if r.get("name") == gate_name]
    if not gates:
        problems.append(f"no check run named {gate_name!r} exists for the tagged commit")
    else:
        latest = max(gates, key=lambda r: (r.get("completed_at") or "", r.get("started_at") or ""))
        if latest.get("status") != "completed" or latest.get("conclusion") != "success":
            problems.append(
                f"the latest {gate_name!r} check run is status={latest.get('status')!r}, "
                f"conclusion={latest.get('conclusion')!r}, not completed/success"
            )
    return problems


def main(argv):
    def opt(name):
        if name not in argv:
            print(__doc__, file=sys.stderr)
            sys.exit(2)
        return argv[argv.index(name) + 1]

    tag, manifest, runs_path = opt("--tag"), Path(opt("--manifest")), Path(opt("--check-runs"))
    try:
        version = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["version"]
        runs = []
        for obj in read_objects(runs_path.read_text(encoding="utf-8")):
            runs.extend(obj.get("check_runs", []))
    except (OSError, KeyError, ValueError) as e:
        print(f"check-release-tag: cannot read inputs: {e}", file=sys.stderr)
        return 2
    problems = check(tag, version, runs)
    for p in problems:
        print(f"FAIL {p}")
    if problems:
        return 1
    print(f"release tag {tag}: equals the manifest version {version}, and gate passed on the tagged commit")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
