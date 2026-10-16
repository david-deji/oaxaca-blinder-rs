#!/usr/bin/env python3
"""Archive the two release binaries with both licences and write a SHA-256 file beside the archive.

    python3 scripts/package-release.py --name linux-x86_64 --out dist
    python3 scripts/package-release.py --name windows-x86_64 --out dist --bin-dir target/release

Reads `oaxaca-cli` and `meridian-mcp` (`.exe` on Windows) from `--bin-dir` (default `target/release`) and the version
from `oaxaca_blinder/Cargo.toml`. Writes `oaxaca-blinder-<version>-<name>.tar.gz` (`.zip` when the name starts with
`windows`) holding one directory of the same name, and `<archive>.sha256` in `sha256sum` format.
Exit 0 and the archive path on stdout; exit 1 when a binary or a licence file is missing.
"""
import hashlib
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARIES = ("oaxaca-cli", "meridian-mcp")
EXTRA_FILES = ("LICENSE-MIT", "LICENSE-APACHE", "README.md")


def package(name, out_dir, bin_dir, root=ROOT):
    version = tomllib.loads((root / "oaxaca_blinder" / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    windows = name.startswith("windows")
    suffix = ".exe" if windows else ""
    stem = f"oaxaca-blinder-{version}-{name}"
    members = []
    for b in BINARIES:
        p = bin_dir / f"{b}{suffix}"
        if not p.is_file():
            raise FileNotFoundError(f"binary not found: {p}")
        members.append((p, f"{stem}/{b}{suffix}", 0o755))
    for f in EXTRA_FILES:
        p = root / f
        if not p.is_file():
            raise FileNotFoundError(f"file not found: {p}")
        members.append((p, f"{stem}/{f}", 0o644))
    out_dir.mkdir(parents=True, exist_ok=True)
    archive = out_dir / (stem + (".zip" if windows else ".tar.gz"))
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
            for src, arcname, _ in members:
                z.write(src, arcname)
    else:
        with tarfile.open(archive, "w:gz") as t:
            for src, arcname, mode in members:
                info = t.gettarinfo(str(src), arcname)
                info.mode, info.uid, info.gid, info.uname, info.gname = mode, 0, 0, "", ""
                with open(src, "rb") as fh:
                    t.addfile(info, fh)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (out_dir / (archive.name + ".sha256")).write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    return archive


def main(argv):
    def opt(flag, default=None):
        if flag in argv:
            return argv[argv.index(flag) + 1]
        if default is None:
            print(__doc__, file=sys.stderr)
            sys.exit(2)
        return default

    name = opt("--name")
    out_dir = Path(opt("--out", "dist"))
    bin_dir = Path(opt("--bin-dir", "target/release"))
    try:
        print(package(name, out_dir, bin_dir))
    except FileNotFoundError as e:
        print(f"package-release: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
