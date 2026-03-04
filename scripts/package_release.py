#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import sys
import tarfile
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Package a compiled wrk binary")
    parser.add_argument("--target", required=True, help="Rust target triple")
    parser.add_argument("--version", required=True, help="Package version")
    parser.add_argument("--name", default="wrk", help="Binary/package name")
    parser.add_argument("--dist-dir", default="dist", help="Output directory")
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    dist_dir = repo_root / args.dist_dir
    target_dir = repo_root / "target" / args.target / "release"
    binary_name = f"{args.name}.exe" if "windows" in args.target else args.name
    binary_path = target_dir / binary_name

    if not binary_path.exists():
        raise SystemExit(f"missing built binary: {binary_path}")

    dist_dir.mkdir(parents=True, exist_ok=True)

    archive_base = f"{args.name}-{args.version}-{args.target}"
    if "windows" in args.target:
        archive_path = dist_dir / f"{archive_base}.zip"
        with ZipFile(archive_path, "w", compression=ZIP_DEFLATED) as archive:
            archive.write(binary_path, arcname=binary_name)
    else:
        archive_path = dist_dir / f"{archive_base}.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(binary_path, arcname=binary_name)

    checksum = sha256(archive_path)
    checksum_path = dist_dir / f"{archive_path.name}.sha256"
    checksum_path.write_text(f"{checksum}  {archive_path.name}\n", encoding="utf-8")

    print(archive_path)
    print(checksum_path)
    return 0


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


if __name__ == "__main__":
    sys.exit(main())
