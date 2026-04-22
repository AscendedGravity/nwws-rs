param(
    [switch]$SkipBench,
    [switch]$Corpus
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
try {
    cargo test --all-targets
    python tools/compare_pyiem.py
    if ($Corpus) {
        python tools/compare_pyiem_corpus.py
    }
    if (-not $SkipBench) {
        cargo bench --bench parse
    }
}
finally {
    Pop-Location
}
