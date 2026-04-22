from __future__ import annotations

import argparse
from pathlib import Path

import compare_pyiem as base


DEFAULT_PRODUCT_DIRS = [
    "vtec",
    "SVR",
    "TOROAX",
    "TORE",
    "FFW",
    "WCN",
    "WSW",
    "NPW",
    "SQW",
    "CFW",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a corpus-scale nwws-rs vs pyIEM comparison."
    )
    parser.add_argument(
        "paths",
        nargs="*",
        type=Path,
        help="Files or directories to compare. Defaults to a curated pyIEM warning corpus.",
    )
    parser.add_argument(
        "--pyiem-src",
        type=Path,
        default=Path(base.os.environ.get("PYIEM_SRC", base.DEFAULT_PYIEM_SRC)),
        help="Path to the pyIEM source tree's src directory.",
    )
    parser.add_argument(
        "--max-failures",
        type=int,
        default=20,
        help="Stop after this many failing files.",
    )
    return parser.parse_args()


def default_paths(pyiem_src: Path) -> list[Path]:
    examples_root = pyiem_src.parent / "data" / "product_examples"
    return [examples_root / name for name in DEFAULT_PRODUCT_DIRS]


def collect_files(paths: list[Path]) -> list[Path]:
    files: list[Path] = []
    for path in paths:
        if path.is_dir():
            files.extend(sorted(path.rglob("*.txt")))
        elif path.is_file():
            files.append(path)
    return files


def main() -> int:
    args = parse_args()
    text_product = base.load_pyiem_text_product(args.pyiem_src)
    summary_executable = base.ensure_summary_executable()

    requested = args.paths or default_paths(args.pyiem_src)
    files = collect_files(requested)
    if not files:
        raise SystemExit("no corpus files found")

    passes = 0
    failures = 0

    for path in files:
        try:
            rust = base.comparable_rust_summary(base.rust_summary(path, summary_executable))
            pyiem = base.pyiem_summary(path, text_product)
            diffs = base.compare(rust, pyiem)
        except BaseException as err:
            failures += 1
            print(f"[FAIL] {path}")
            print(f"  - harness error: {err}")
            if failures >= args.max_failures:
                break
            continue

        if diffs:
            failures += 1
            print(f"[FAIL] {path}")
            for diff in diffs:
                print(f"  - {diff}")
            if failures >= args.max_failures:
                break
        else:
            passes += 1

    total = passes + failures
    print(
        f"Compared {total} file(s): {passes} passed, {failures} failed."
    )
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
