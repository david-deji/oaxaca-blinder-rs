#!/usr/bin/env python3
"""Fail when a page of the built docs site links to a file that is not in the build (0121-MERIDIAN L3).

mdBook checks that every page in SUMMARY.md exists but never checks a link inside a page, so a renamed page or a
mistyped path would publish a dead link. This reads every built HTML page and resolves each relative `href` and
`src` against the build directory. External links, `mailto:` and bare `#fragment` links are skipped; a fragment on
a local link is not checked, only the file.

Usage: python3 scripts/check-book-links.py <build-dir>
Exit 0 = every local link resolves, 1 = dead links (listed), 2 = could not run.
"""
import re
import sys
from pathlib import Path

LINK = re.compile(r'(?:href|src)="([^"]+)"')
SKIP_PAGES = {"print.html", "404.html"}  # print.html repeats every page with links relative to the root


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(argv[1])
    pages = [p for p in root.rglob("*.html") if p.name not in SKIP_PAGES]
    if not pages:
        print(f"no html pages under {root}", file=sys.stderr)
        return 2
    dead = []
    for page in pages:
        for target in LINK.findall(page.read_text(encoding="utf-8")):
            if target.startswith(("http://", "https://", "//", "mailto:", "#", "data:", "javascript:")):
                continue
            path = target.split("#", 1)[0].split("?", 1)[0]
            if not path:
                continue
            resolved = (page.parent / path).resolve()
            if not resolved.exists():
                dead.append(f"{page.relative_to(root)}: {target}")
    if dead:
        print("dead local links:")
        for line in sorted(set(dead)):
            print(f"  {line}")
        return 1
    print(f"{len(pages)} pages checked, every local link resolves")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
