use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nwws"))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path(name: &str) -> PathBuf {
    manifest_dir().join("tests").join("fixtures").join(name)
}

fn temp_dir_path(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{unique}"))
}

fn frame_with_wmo_separators(bulletin: &str) -> String {
    let bulletin = bulletin.lines().collect::<Vec<_>>().join("\r\r\n");
    format!("\u{1}\r\r\n{bulletin}\r\r\n\u{3}")
}

fn read_to_string(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn help_mentions_pid201_and_archive_commands() {
    let output = Command::new(bin_path()).arg("--help").output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pid201 split"));
    assert!(stdout.contains("archive import"));
    assert!(stdout.contains("archive verify"));
}

#[test]
fn pid201_split_writes_canonical_bulletin_files() {
    let temp = temp_dir_path("nwws_rs_cli_pid201_split");
    let output_dir = temp.join("split");
    fs::create_dir_all(&temp).unwrap();

    let first = read_to_string(&fixture_path("wmo_tornado_warning.txt"));
    let second = read_to_string(&fixture_path("wmo_segmented_svs.txt"));
    let capture_path = temp.join("capture.pid201");
    fs::write(
        &capture_path,
        format!(
            "{}{}",
            frame_with_wmo_separators(&first),
            frame_with_wmo_separators(&second)
        ),
    )
    .unwrap();

    let output = Command::new(bin_path())
        .args(["pid201", "split"])
        .arg(&capture_path)
        .arg(&output_dir)
        .output()
        .unwrap();

    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("written-files: 2"));

    let mut files = fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();

    assert_eq!(files.len(), 2);
    assert!(read_to_string(&files[0]).contains("WUUS53 KLOT 211600"));
    assert!(read_to_string(&files[1]).contains("WWUS73 KLOT 211620"));

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn archive_import_and_verify_deduplicate_mixed_inputs() {
    let temp = temp_dir_path("nwws_rs_cli_archive");
    let input_dir = temp.join("input");
    let archive_dir = temp.join("archive");
    fs::create_dir_all(&input_dir).unwrap();

    fs::copy(
        fixture_path("wmo_tornado_warning.txt"),
        input_dir.join("warning.txt"),
    )
    .unwrap();
    fs::copy(
        fixture_path("nwws_oi_tornado_warning.xml"),
        input_dir.join("warning.xml"),
    )
    .unwrap();

    let segmented = read_to_string(&fixture_path("wmo_segmented_svs.txt"));
    fs::write(
        input_dir.join("capture.pid201"),
        frame_with_wmo_separators(&segmented),
    )
    .unwrap();

    let import = Command::new(bin_path())
        .args(["archive", "import"])
        .arg(&input_dir)
        .arg(&archive_dir)
        .output()
        .unwrap();

    assert!(import.status.success(), "{:?}", import);
    let import_stdout = String::from_utf8(import.stdout).unwrap();
    assert!(import_stdout.contains("archived-records: 2"));
    assert!(import_stdout.contains("duplicate-records: 1"));
    assert!(archive_dir.join("records.tsv").exists());

    let record_root = archive_dir.join("records");
    let mut record_files = Vec::new();
    collect_record_files(&record_root, &mut record_files);
    assert_eq!(record_files.len(), 2);

    let verify = Command::new(bin_path())
        .args(["archive", "verify"])
        .arg(&archive_dir)
        .output()
        .unwrap();

    assert!(verify.status.success(), "{:?}", verify);
    let verify_stdout = String::from_utf8(verify.stdout).unwrap();
    assert!(verify_stdout.contains("verified-records: 2"));
    assert!(verify_stdout.contains("failures: 0"));

    fs::remove_dir_all(temp).unwrap();
}

fn collect_record_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            collect_record_files(&path, files);
        } else {
            files.push(path);
        }
    }
}
