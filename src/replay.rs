use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ingest::{IngestHint, ParsedInput, TransportDescriptor, parse_with_hint};
use crate::product::ProductFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayInputKind {
    OpenInterface,
    Bulletin,
    FramedStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRecordSummary {
    pub sequence_number: Option<u16>,
    pub ttaaii: String,
    pub cccc: String,
    pub awips_id: Option<String>,
    pub family: ProductFamily,
    pub segment_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaySummary {
    pub path: Option<PathBuf>,
    pub inferred_hint: IngestHint,
    pub input_kind: ReplayInputKind,
    pub transport: TransportDescriptor,
    pub record_count: usize,
    pub leading_junk_prefix: usize,
    pub pending_bytes: usize,
    pub records: Vec<ReplayRecordSummary>,
}

pub fn collect_input_paths(root: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_recursively(root.as_ref(), &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_recursively(root: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    let metadata = fs::metadata(root)?;
    if metadata.is_file() {
        paths.push(root.to_path_buf());
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|value| value.to_str())
            && matches!(name, ".git" | "target" | "__pycache__")
        {
            continue;
        }
        collect_recursively(&path, paths)?;
    }
    Ok(())
}

pub fn infer_hint_from_path(path: &Path) -> IngestHint {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("xml") => IngestHint::OpenInterface,
        Some("sbn" | "pid201" | "bin" | "dat" | "ldm") => IngestHint::SatellitePid201,
        _ => IngestHint::Auto,
    }
}

pub fn summarize_path(path: impl AsRef<Path>) -> io::Result<ReplaySummary> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let hint = infer_hint_from_path(path);
    summarize_bytes(Some(path), &bytes, hint)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub fn summarize_bytes(
    path: Option<&Path>,
    bytes: &[u8],
    hint: IngestHint,
) -> crate::Result<ReplaySummary> {
    let parsed = parse_with_hint(hint, bytes)?;
    let summary = match parsed {
        ParsedInput::Bulletin(value) => ReplaySummary {
            path: path.map(Path::to_path_buf),
            inferred_hint: hint,
            input_kind: ReplayInputKind::Bulletin,
            transport: value.transport,
            record_count: 1,
            leading_junk_prefix: 0,
            pending_bytes: 0,
            records: vec![ReplayRecordSummary::from_content(&value.content)],
        },
        ParsedInput::OpenInterface(value) => {
            let content = value.content()?;
            ReplaySummary {
                path: path.map(Path::to_path_buf),
                inferred_hint: hint,
                input_kind: ReplayInputKind::OpenInterface,
                transport: value.transport,
                record_count: 1,
                leading_junk_prefix: 0,
                pending_bytes: 0,
                records: vec![ReplayRecordSummary::from_content(&content)],
            }
        }
        ParsedInput::FramedStream(value) => {
            let contents = value.contents()?;
            ReplaySummary {
                path: path.map(Path::to_path_buf),
                inferred_hint: hint,
                input_kind: ReplayInputKind::FramedStream,
                transport: value.transport,
                record_count: contents.len(),
                leading_junk_prefix: value.leading_junk_prefix,
                pending_bytes: value.pending.len(),
                records: contents
                    .iter()
                    .map(ReplayRecordSummary::from_content)
                    .collect(),
            }
        }
    };
    Ok(summary)
}

impl ReplayRecordSummary {
    fn from_content(content: &crate::NwwsContent<'_>) -> Self {
        Self {
            sequence_number: content.bulletin.sequence_number,
            ttaaii: content.bulletin.heading.ttaaii().to_owned(),
            cccc: content.bulletin.heading.cccc().to_owned(),
            awips_id: content
                .bulletin
                .awips_id
                .as_ref()
                .map(|value| value.raw().to_owned()),
            family: content.product.family,
            segment_count: content.product.segments.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{ReplayInputKind, collect_input_paths, infer_hint_from_path, summarize_bytes};
    use crate::ingest::IngestHint;

    #[test]
    fn infers_hint_from_extension() {
        assert_eq!(
            infer_hint_from_path(Path::new("capture.xml")),
            IngestHint::OpenInterface
        );
        assert_eq!(
            infer_hint_from_path(Path::new("capture.pid201")),
            IngestHint::SatellitePid201
        );
        assert_eq!(
            infer_hint_from_path(Path::new("capture.txt")),
            IngestHint::Auto
        );
    }

    #[test]
    fn summarizes_open_interface_fixture() {
        let bytes = include_bytes!("../tests/fixtures/nwws_oi_tornado_warning.xml");
        let summary = summarize_bytes(None, bytes, IngestHint::OpenInterface).unwrap();

        assert_eq!(summary.input_kind, ReplayInputKind::OpenInterface);
        assert_eq!(summary.record_count, 1);
        assert_eq!(summary.records[0].cccc, "KLOT");
        assert_eq!(summary.records[0].segment_count, 1);
    }

    #[test]
    fn summarizes_framed_stream() {
        let input =
            b"junk\x01\r\r\n111\r\r\nNOUS41 KWBC 201530 AAA\r\r\nPNSXXX\r\r\nBody\r\r\n\x03tail";
        let summary = summarize_bytes(None, input, IngestHint::SatellitePid201).unwrap();

        assert_eq!(summary.input_kind, ReplayInputKind::FramedStream);
        assert_eq!(summary.leading_junk_prefix, 4);
        assert_eq!(summary.pending_bytes, 4);
        assert_eq!(summary.records[0].awips_id.as_deref(), Some("PNSXXX"));
    }

    #[test]
    fn collects_files_recursively() {
        let root = temp_dir_path("nwws_rs_replay_collect");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let file_a = root.join("a.txt");
        let file_b = nested.join("b.xml");
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();

        let files = collect_input_paths(&root).unwrap();
        assert_eq!(files, vec![file_a, file_b]);

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir_path(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{unique}"))
    }

    use std::path::{Path, PathBuf};
}
