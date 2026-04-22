from __future__ import annotations

import argparse
import os
import json
import subprocess
import sys
from pathlib import Path

from shapely import wkt


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PYIEM_SRC = REPO_ROOT.parent / "pyIEM" / "src"
DEFAULT_FIXTURES = [
    REPO_ROOT / "tests" / "fixtures" / "wmo_bulletin.txt",
    REPO_ROOT / "tests" / "fixtures" / "wmo_tornado_warning.txt",
    REPO_ROOT / "tests" / "fixtures" / "wmo_segmented_svs.txt",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare nwws-rs raw bulletin parsing against pyIEM."
    )
    parser.add_argument(
        "fixtures",
        nargs="*",
        type=Path,
        default=DEFAULT_FIXTURES,
        help="Raw WMO bulletin fixture paths to compare.",
    )
    parser.add_argument(
        "--pyiem-src",
        type=Path,
        default=Path(os.environ.get("PYIEM_SRC", DEFAULT_PYIEM_SRC)),
        help="Path to the pyIEM source tree's src directory.",
    )
    return parser.parse_args()


def load_pyiem_text_product(pyiem_src: Path):
    if not pyiem_src.exists():
        raise SystemExit(
            f"pyIEM source tree not found at {pyiem_src}. "
            "Clone https://github.com/akrherz/pyIEM or set PYIEM_SRC."
        )

    sys.path.insert(0, str(pyiem_src))
    try:
        from pyiem.nws.product import TextProduct
    except Exception as err:  # pragma: no cover - environment dependent
        raise SystemExit(
            "Failed to import pyIEM TextProduct. "
            "Ensure minimal parser dependencies are installed. "
            f"Import error: {err}"
        ) from err

    return TextProduct


def ensure_summary_executable() -> Path:
    subprocess.run(
        ["cargo", "build", "--quiet", "--example", "summary"],
        cwd=REPO_ROOT,
        check=True,
    )
    candidates = [
        REPO_ROOT / "target" / "debug" / "examples" / "summary.exe",
        REPO_ROOT / "target" / "debug" / "examples" / "summary",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise SystemExit("failed to locate built summary example executable")


def rust_summary(path: Path, summary_executable: Path | None = None) -> dict:
    command = (
        [str(summary_executable), str(path)]
        if summary_executable is not None
        else ["cargo", "run", "--quiet", "--example", "summary", "--", str(path)]
    )
    result = subprocess.run(
        command,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"nwws-rs summary failed for {path}:\n{result.stderr.strip()}"
        )
    return json.loads(result.stdout)


def pyiem_summary(path: Path, text_product) -> dict:
    text = path.read_text(encoding="utf-8")
    product = text_product(text, ugc_provider={}, nwsli_provider={})

    segments = []
    for segment in product.segments:
        summary = summarize_pyiem_segment(segment)
        if summary is not None:
            segments.append(summary)

    return {
        "ttaaii": product.wmo,
        "cccc": product.source,
        "bbb": product.bbb,
        "awips_id": product.afos,
        "structured_segment_count": len(segments),
        "segments": segments,
    }


def summarize_pyiem_segment(segment) -> dict | None:
    if not any(
        [
            segment.ugcs,
            segment.vtec,
            segment.hvtec,
            segment.giswkt,
            segment.tml_giswkt,
            segment.tornadotag,
            segment.damagetag,
            segment.hailtag,
            segment.windtag,
        ]
    ):
        return None

    return {
        "ugcs": [str(ugc) for ugc in segment.ugcs],
        "pvtec": [str(value) for value in segment.vtec],
        "hvtec": [str(value) for value in segment.hvtec],
        "tornado_tag": segment.tornadotag,
        "flash_flood_observed": segment.flood_tags.get("FLASH FLOOD") == "OBSERVED",
        "flash_flood_emergency": bool(segment.is_emergency)
        and bool(segment.flood_tags),
        "hail_inches": round2(float(segment.hailtag)) if segment.hailtag else None,
        "wind_mph": int(segment.windtag)
        if segment.windtag and segment.windtagunits == "MPH"
        else None,
        "damage_threat": segment.damagetag,
        "lat_lon": canonical_ring(polygon_points(segment.sbw)),
        "time_mot_loc": time_mot_loc(segment),
    }


def polygon_points(polygon) -> list[dict] | None:
    if polygon is None:
        return None

    coords = list(polygon.exterior.coords)
    if len(coords) > 1 and coords[0] == coords[-1]:
        coords = coords[:-1]

    return [{"lat": round2(lat), "lon": round2(lon)} for lon, lat in coords]


def time_mot_loc(segment) -> dict | None:
    if segment.tml_giswkt is None:
        return None

    geometry = wkt.loads(segment.tml_giswkt.replace("SRID=4326;", ""))
    locations = [{"lat": round2(lat), "lon": round2(lon)} for lon, lat in geometry.coords]

    return {
        "time": segment.tml_valid.strftime("%H%MZ") if segment.tml_valid else None,
        "direction_degrees": segment.tml_dir,
        "speed_knots": segment.tml_sknt,
        "locations": locations,
    }


def comparable_rust_summary(summary: dict) -> dict:
    return {
        "ttaaii": summary["ttaaii"],
        "cccc": summary["cccc"],
        "bbb": summary["bbb"],
        "awips_id": summary["awips_id"],
        "structured_segment_count": summary["structured_segment_count"],
        "segments": [
            {
                "ugcs": segment["ugcs"],
                "pvtec": segment["pvtec"],
                "hvtec": segment["hvtec"],
                "tornado_tag": segment["tornado_tag"],
                "flash_flood_observed": segment["flash_flood_observed"],
                "flash_flood_emergency": segment["flash_flood_emergency"],
                "hail_inches": segment["hail_inches"],
                "wind_mph": segment["wind_mph"],
                "damage_threat": segment["damage_threat"],
                "lat_lon": canonical_ring(segment["lat_lon"]),
                "time_mot_loc": segment["time_mot_loc"],
            }
            for segment in summary["segments"]
        ],
    }


def compare(actual, expected, path: str = "root") -> list[str]:
    diffs: list[str] = []

    if type(actual) is not type(expected):
        return [f"{path}: type mismatch {type(actual).__name__} != {type(expected).__name__}"]

    if isinstance(actual, dict):
        keys = sorted(set(actual) | set(expected))
        for key in keys:
            if key not in actual:
                diffs.append(f"{path}.{key}: missing from rust summary")
                continue
            if key not in expected:
                diffs.append(f"{path}.{key}: extra in rust summary")
                continue
            diffs.extend(compare(actual[key], expected[key], f"{path}.{key}"))
        return diffs

    if isinstance(actual, list):
        if len(actual) != len(expected):
            diffs.append(f"{path}: length mismatch {len(actual)} != {len(expected)}")
            return diffs
        for index, (lhs, rhs) in enumerate(zip(actual, expected, strict=True)):
            diffs.extend(compare(lhs, rhs, f"{path}[{index}]"))
        return diffs

    if actual != expected:
        diffs.append(f"{path}: {actual!r} != {expected!r}")

    return diffs


def round2(value: float) -> float:
    return round(value, 2)


def canonical_ring(points: list[dict] | None) -> list[dict] | None:
    if not points:
        return points

    tuples = [(point["lat"], point["lon"]) for point in points]
    rotations = []
    for sequence in (tuples, list(reversed(tuples))):
        for index in range(len(sequence)):
            rotations.append(sequence[index:] + sequence[:index])

    best = min(rotations)
    return [{"lat": lat, "lon": lon} for lat, lon in best]


def main() -> int:
    args = parse_args()
    text_product = load_pyiem_text_product(args.pyiem_src)
    summary_executable = ensure_summary_executable()

    failures = 0
    for fixture in args.fixtures:
        rust = comparable_rust_summary(rust_summary(fixture, summary_executable))
        pyiem = pyiem_summary(fixture, text_product)
        diffs = compare(rust, pyiem)

        if diffs:
            failures += 1
            print(f"[FAIL] {fixture}")
            for diff in diffs:
                print(f"  - {diff}")
        else:
            print(f"[PASS] {fixture}")

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
