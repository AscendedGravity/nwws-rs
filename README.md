# nwws-rs

[![CI](https://github.com/FahrenheitResearch/nwws-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/FahrenheitResearch/nwws-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/nwws-rs.svg)](https://crates.io/crates/nwws-rs)
[![PyPI](https://img.shields.io/pypi/v/nwws-rs.svg)](https://pypi.org/project/nwws-rs/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

**A self-hosted NOAA Weather Wire Service (NWWS-OI) platform in one static binary.**
Connects to the NWS's lowest-latency public text-product feed, parses every
bulletin strictly (WMO headers, AWIPS/PIL, UGC, VTEC/HVTEC, segments, warning
tags, polygons, storm motion), dedupes, archives, and serves it all back over a
local HTTP API with a live Server-Sent Events stream. Rust core, Python
bindings, CLI.

```bash
# the entire setup, start to finish:
docker run -e NWWS_USERNAME=you -e NWWS_PASSWORD=secret \
  -v nwws-archive:/archive -p 8080:8080 ghcr.io/fahrenheitresearch/nwws-rs

curl 'http://127.0.0.1:8080/v1/stream?pil=TOR'     # tornado warnings, live, as SSE
curl 'http://127.0.0.1:8080/v1/warnings/active'    # everything in effect right now
```

NWWS-OI delivers warnings seconds after the WFO hits send — typically well
ahead of the public CAP/REST mirrors. Credentials are free:
[request them from the NWS here](https://www.weather.gov/nwws/nwws_oi_request).

## Why this exists

Running your own NWWS-OI consumer has traditionally meant gluing together an
XMPP library, a parser (usually [pyIEM](https://github.com/akrherz/pyIEM)),
hand-rolled reconnect logic, deduplication, and storage. nwws-rs ships that
whole stack as one tool, and the parsing core is fast enough to chew through
years of archives:

| | products/sec (full parse) | relative |
|---|---:|---:|
| **nwws-rs** (via Python bindings) | **31,265** | **19.0x** |
| pyIEM 1.x (pure Python) | 1,642 | 1x |

Same bulletins, same machine, full parse path on both sides (headers, UGC,
VTEC, segments, geometry). The nwws-rs number *includes* the Python boundary
overhead; the pure-Rust path is faster still. Reproduce it:
`python tools/bench_pyiem_speed.py` (methodology in the script header).

Honest scope: nwws-rs covers NWWS **text products**. pyIEM also parses METAR,
SHEF, and other formats and is the right tool for those. For NWWS-OI ingest,
warning parsing, alerting, and archive work, nwws-rs is designed to be the
last tool you need.

## Quick start

### 1. Self-hosted API server (the headline feature)

Grab a binary from [Releases](https://github.com/FahrenheitResearch/nwws-rs/releases)
(Linux, macOS, Windows) or build with `cargo install nwws-rs --features serve`,
then:

```bash
export NWWS_USERNAME=you NWWS_PASSWORD=secret
nwws serve ./archive --bind 127.0.0.1:8080
```

That one process connects to NWWS-OI, auto-reconnects forever with jittered
backoff and MUC history backfill, validates and dedupes every product, archives
them under date-partitioned directories, and serves:

| Endpoint | What it returns |
|---|---|
| `GET /v1/stream` | live products as Server-Sent Events; filter with `?office=KLOT`, `?pil=TOR`, `?family=tornado` |
| `GET /v1/products/recent` | newest archived products (`limit`, `days`, `office`, `pil`, `family`) |
| `GET /v1/products/{fingerprint}` | metadata + full raw text of one product |
| `GET /v1/warnings/active` | VTEC warnings in effect at `?at=` (default now), collapsed per event |
| `GET /v1/timeline` | warning lifecycle records: issued/canceled/expired, polygons, motion, tags |
| `GET /healthz` | ingest connection state and counters |

CORS is permissive, so a browser dashboard can consume the API directly.
`--no-ingest` serves an existing archive with **zero credentials** — useful for
replaying captured data or fronting a shared archive.

A tornado-warning webhook is a shell one-liner:

```bash
curl -N 'http://127.0.0.1:8080/v1/stream?pil=TOR' | while read -r line; do
  case "$line" in data:*) echo "${line#data: }" | your-notifier ;; esac
done
```

### 2. Python

```bash
pip install nwws-rs
```

```python
import nwws_rs

# Parse any NWS text product
msg = nwws_rs.parse_bulletin(open("tor.txt", "rb").read())
print(msg.heading, msg.awips_id, msg.family)
print(msg.segments[0].tornado_tag)

# Or consume NWWS-OI live, no server needed
client = nwws_rs.OiClient.connect("user", "password")
while True:
    message = client.next_message()
    print(message.wrapper.id, message.heading)
```

Full surface: `parse`, `parse_bulletin`, `parse_oi`, `inspect_*`, `scan_path`,
`active_warnings_at`, `split_pid201_*`, `archive_import`, `archive_verify`,
`Pid201Stream`, `OiClient`. Typed, object-oriented, returns structured objects.

### 3. Rust

```rust
use nwws_rs::NwwsContent;

let bytes = include_bytes!("tests/fixtures/wmo_tornado_warning.txt");
let content = NwwsContent::parse_bulletin(bytes)?;

assert_eq!(content.bulletin.heading.ttaaii(), "WUUS53");
assert_eq!(content.bulletin.heading.cccc(), "KLOT");
assert_eq!(content.bulletin.awips_id.unwrap().raw(), "TORLOT");
# Ok::<(), Box<dyn std::error::Error>>(())
```

The supervised ingest loop is a library API too ([`daemon`](src/daemon.rs)):

```rust,no_run
use std::sync::atomic::AtomicBool;
use nwws_rs::{
    ArchiveStore, DaemonOptions, DedupeStore, IngestService, MessageRouter, OiClientConfig,
    run_oi_daemon,
};

let config = OiClientConfig::new("user", "password");
let router = MessageRouter::new(Some(ArchiveStore::new("archive")));
let dedupe = DedupeStore::open("archive/state/dedupe.txt")?;
let mut service = IngestService::new(router, dedupe);
let shutdown = AtomicBool::new(false);
run_oi_daemon(&config, &mut service, &DaemonOptions::default(), |_event| {}, &shutdown);
# Ok::<(), std::io::Error>(())
```

### 4. Headless ingest (no HTTP)

```bash
nwws oi daemon ./archive            # credentials from NWWS_USERNAME/NWWS_PASSWORD
```

Same supervision as `serve` (reconnect, backfill, dedupe), just without the API.
A hardened systemd unit and env template live in [`deploy/`](deploy/).

## How it compares

| | nwws-rs | pyIEM + slixmpp | api.weather.gov |
|---|---|---|---|
| Latency | seconds (NWWS-OI direct) | seconds (NWWS-OI direct) | tens of seconds to minutes |
| Setup | one binary / `docker run` | assemble client, parser, reconnect, storage yourself | none (hosted) |
| Self-hosted / offline archive | yes | DIY | no |
| Live push | SSE out of the box | DIY | no (poll) |
| Bulk reparse throughput | ~31k products/sec | ~1.6k products/sec | n/a |
| Non-NWWS formats (METAR, SHEF...) | no | yes | partial |
| Credentials needed | free NWS signup | free NWS signup | none |

## CLI reference

The `nwws` binary also covers inspection, replay, and research workflows over
files, directories, and archives:

```text
nwws inspect <file>                              parse + validate one input (WMO, NWWS-OI XML, PID201)
nwws replay <dir>                                stream a captured corpus through the parser
nwws summary <dir>                               source/transport/family counts
nwws active-at <path> --at <rfc3339>             VTEC warnings active at a moment
nwws timeline <path> [--at <rfc3339>]            warning lifecycle records (issued/canceled/expired, polygons)
nwws lead-time <path> --event-at <t> --lat --lon point-event warning lead-time metrics
nwws oi connect <user> <pass> [--count n]        print live NWWS-OI messages
nwws oi archive <user> <pass> <dir> [--duration] bounded live capture into an archive
nwws oi daemon <dir>                             supervised always-on ingest (auto-reconnect + backfill)
nwws serve <dir> [--bind addr] [--no-ingest]     ingest daemon + HTTP API        (build feature: serve)
nwws pid201 inspect|split|archive ...            PID201 framed-stream (NOAAPORT/EMWIN-style) tooling
nwws archive import|verify|active-at|timeline    canonical archive workflows
```

Most commands accept `--format json|jsonl|tool-result` for machine-readable
output. `tool-result` wraps reports in a `wx.tool_result.v1` envelope with
`artifacts`, `evidence`, `limitations`, and `provenance` — built for AI-agent
consumers.

## Design notes

### Parsing model

NWWS is treated as multiple transport surfaces over one bulletin semantics:
raw WMO text, NWWS-OI XMPP stanzas, and PID201 framed streams. The rule
throughout: **never trust the wrapper more than the bulletin.** NWWS-OI
metadata is validated against the embedded WMO bulletin instead of being
accepted at face value.

### Library layers

- [`src/wmo.rs`](src/wmo.rs) — WMO bulletin framing and headers
- [`src/oi.rs`](src/oi.rs) — NWWS-OI messages, wrapper-vs-bulletin validation
- [`src/product.rs`](src/product.rs) — product families, segments, warning tags
- [`src/ugc.rs`](src/ugc.rs) / [`src/vtec.rs`](src/vtec.rs) — UGC expansion, P-VTEC/H-VTEC
- [`src/geo.rs`](src/geo.rs) — `LAT...LON` polygons, `TIME...MOT...LOC` motion
- [`src/pid201.rs`](src/pid201.rs) — incremental framed-stream ingest
- [`src/runtime.rs`](src/runtime.rs) — dedupe, archive store, routing
- [`src/oi_client.rs`](src/oi_client.rs) — blocking NWWS-OI XMPP client (rustls, no OpenSSL)
- [`src/daemon.rs`](src/daemon.rs) — supervised ingest: jittered exponential backoff
  (equal-jitter; Brooker 2015, AWS Architecture Blog), XMPP whitespace keepalives
  (RFC 6120 §4.6), MUC history backfill on reconnect
- [`src/serve.rs`](src/serve.rs) — axum HTTP API + SSE (feature `serve`)
- [`src/warning.rs`](src/warning.rs) — warning timelines, lead-time and
  area-time-polygon verification metrics
- [`src/api.rs`](src/api.rs) — inspect/scan/split/archive surface
- [`src/python.rs`](src/python.rs) — PyO3 bindings (abi3, one wheel per platform)

### Archive layout

`oi daemon` / `oi archive` / `serve` write
`archive/yyyy/mm/dd/<source>/<office>/<pil>/<family>/<fingerprint>.{xml,json}` —
raw stanza plus metadata sidecar, deduplicated by normalized bulletin content
(BLAKE3). Date partitioning keeps HTTP query cost bounded by the lookback
window (`?days=`), not by total archive size. The dedupe index survives
restarts, so reconnect backfill never double-archives.

### Verification

- 140+ unit/integration/property/CLI/HTTP tests, including SSE end-to-end
- differential comparison against pyIEM on overlapping raw-bulletin semantics
  (`tools/compare_pyiem.py`, `tools/compare_pyiem_corpus.py`)
- Python API tests against the built wheel
- CI: 3-OS test matrix, clippy `-D warnings`, MSRV check, Docker smoke test

```powershell
.\tools\verify.ps1            # fmt + clippy + tests + python suite
.\tools\verify.ps1 -Corpus    # plus the pyIEM corpus comparison
```

### Accuracy scope

Strict and heavily verified, but bounded to what is implemented and tested:
WMO/NWWS-OI/PID201 parsing, AWIPS/UGC/VTEC/HVTEC/segments/tags/geometry,
archive ingest and verification. It is not a NOAAPORT demodulator, not a
satellite receiver, and not proof against every malformed message ever emitted.

## License

MIT or Apache-2.0, at your option.
