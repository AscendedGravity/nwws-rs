# Fuzz Seeds

This directory is a lightweight seed scaffold for external fuzzing without changing `Cargo.toml`.

- `corpus/wmo_message/` contains valid and near-valid raw bulletin samples, including UGC/VTEC warnings, segmented products, and an invalid AWIPS-line variant.
- `corpus/nwws_oi/` contains NWWS-OI wrapper samples for both valid and metadata-mismatched cases.

The checked-in corpus mirrors the integration fixtures so parser changes can be exercised from both deterministic tests and future fuzz harnesses.
