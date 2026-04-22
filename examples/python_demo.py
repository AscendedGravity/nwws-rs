from __future__ import annotations

from pathlib import Path

import nwws_rs


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures"


def framed(path: Path) -> bytes:
    lines = path.read_text(encoding="utf-8").splitlines()
    return f"\u0001\r\r\n{'\r\r\n'.join(lines)}\r\r\n\u0003".encode("utf-8")


warning = nwws_rs.parse_bulletin((FIXTURES / "wmo_tornado_warning.txt").read_bytes())
print("=== Bulletin ===")
print(f"heading: {warning.heading}")
print(f"awips: {warning.awips_id}")
print(f"family: {warning.family}")
print(f"ugc count: {len(warning.segments[0].ugcs)}")
print(f"tornado tag: {warning.segments[0].tornado_tag}")

oi_message = nwws_rs.parse_oi(
    (FIXTURES / "nwws_oi_tornado_warning.xml").read_text(encoding="utf-8")
)
print()
print("=== NWWS-OI ===")
print(f"wrapper id: {oi_message.wrapper.id if oi_message.wrapper else '-'}")
print(f"semantic fingerprint: {oi_message.semantic_fingerprint}")

capture = b"junk" + framed(FIXTURES / "wmo_tornado_warning.txt")
capture += framed(FIXTURES / "wmo_segmented_svs.txt") + b"tail"
split = nwws_rs.split_pid201_bytes(capture)
print()
print("=== PID201 ===")
print(f"records: {len(split.records)}")
print(f"junk bytes: {split.junk_bytes}")
print(f"pending bytes: {split.pending_bytes}")
