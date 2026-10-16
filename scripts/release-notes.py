#!/usr/bin/env python3
"""Print the CHANGELOG section of one release: the text under `## [X.Y.Z] - date`, up to the next `## [` heading.

    python3 scripts/release-notes.py v0.3.0          # the tag or the bare version
    python3 scripts/release-notes.py 0.3.0 --changelog path/to/CHANGELOG.md

The release workflow uses the output as the body of the GitHub release, so the release text is the changelog text
and nothing else. The heading line itself is not printed (the release already carries the tag as its title).

Exit 0 and the section on stdout; exit 1 and a reason on stderr when the section is missing, repeated or empty.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def section(text, version):
    lines = text.splitlines()
    heading = re.compile(r"^## \[(?P<name>[^\]]+)\]")
    starts = [i for i, line in enumerate(lines) if (m := heading.match(line)) and m.group("name") == version]
    if not starts:
        raise ValueError(f"CHANGELOG.md has no `## [{version}]` heading")
    if len(starts) > 1:
        raise ValueError(f"CHANGELOG.md has {len(starts)} `## [{version}]` headings")
    body = []
    for line in lines[starts[0] + 1 :]:
        if line.startswith("## ["):
            break
        body.append(line)
    out = "\n".join(body).strip("\n")
    if not out.strip():
        raise ValueError(f"the `## [{version}]` section of CHANGELOG.md is empty")
    return out + "\n"


def main(argv):
    path = ROOT / "CHANGELOG.md"
    args = list(argv)
    if "--changelog" in args:
        i = args.index("--changelog")
        path = Path(args[i + 1])
        del args[i : i + 2]
    if len(args) != 1 or args[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    version = args[0][1:] if args[0].startswith("v") else args[0]
    try:
        sys.stdout.write(section(path.read_text(encoding="utf-8"), version))
    except (ValueError, OSError) as e:
        print(f"release-notes: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
