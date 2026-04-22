use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde::Serialize;

use crate::api::{
    MessageSummary, archive_import, archive_verify, inspect_bytes, inspect_oi_message,
    inspect_path, inspect_text, scan_path, split_pid201_bytes, split_pid201_path, to_json,
    write_pid201_split,
};
use crate::ingest::IngestHint;
use crate::oi_client::{NwwsOiClient, OiClientConfig};
use crate::pid201::Pid201StreamAdapter;

fn parse_hint(value: Option<&str>) -> PyResult<IngestHint> {
    match value.unwrap_or("auto").to_ascii_lowercase().as_str() {
        "auto" => Ok(IngestHint::Auto),
        "oi" | "open-interface" | "openinterface" | "xmpp" => Ok(IngestHint::OpenInterface),
        "pid201" | "satellite" | "sat" => Ok(IngestHint::SatellitePid201),
        "bulletin" | "raw" | "wmo" => Ok(IngestHint::RawBulletin),
        "stream" | "framed-stream" | "framed" => Ok(IngestHint::FramedStream),
        other => Err(PyValueError::new_err(format!("unknown hint: {other}"))),
    }
}

fn runtime_err(err: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

#[pyfunction(signature = (input, hint=None))]
fn inspect_bytes_json(input: &[u8], hint: Option<&str>) -> PyResult<String> {
    let report = inspect_bytes(input, parse_hint(hint)?).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction(signature = (input, hint=None))]
fn inspect_text_json(input: &str, hint: Option<&str>) -> PyResult<String> {
    let report = inspect_text(input, parse_hint(hint)?).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction(signature = (path, hint=None))]
fn inspect_path_json(path: &str, hint: Option<&str>) -> PyResult<String> {
    let hint = hint.map(|value| parse_hint(Some(value))).transpose()?;
    let report = inspect_path(path, hint).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction(signature = (path, hint=None))]
fn scan_path_json(path: &str, hint: Option<&str>) -> PyResult<String> {
    let hint = hint.map(|value| parse_hint(Some(value))).transpose()?;
    let report = scan_path(path, hint).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction]
fn split_pid201_bytes_json(input: &[u8]) -> PyResult<String> {
    let report = split_pid201_bytes(input).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction]
fn split_pid201_path_json(path: &str) -> PyResult<String> {
    let report = split_pid201_path(path).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction]
fn write_pid201_split_json(input_path: &str, output_dir: &str) -> PyResult<String> {
    let report = write_pid201_split(input_path, output_dir).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction(signature = (input_path, archive_dir, hint=None))]
fn archive_import_json(
    input_path: &str,
    archive_dir: &str,
    hint: Option<&str>,
) -> PyResult<String> {
    let hint = hint.map(|value| parse_hint(Some(value))).transpose()?;
    let report = archive_import(input_path, archive_dir, hint).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction]
fn archive_verify_json(archive_dir: &str) -> PyResult<String> {
    let report = archive_verify(archive_dir).map_err(runtime_err)?;
    to_json(&report).map_err(runtime_err)
}

#[pyfunction]
fn collect_input_paths(root: &str) -> PyResult<Vec<String>> {
    crate::replay::collect_input_paths(root)
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(runtime_err)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Pid201PushRecord {
    offset: usize,
    leading_junk_prefix: usize,
    message: MessageSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Pid201PushReport {
    records: Vec<Pid201PushRecord>,
    pending_bytes: usize,
}

#[pyclass(name = "NativePid201Stream", unsendable)]
struct PyPid201Stream {
    adapter: Pid201StreamAdapter,
}

#[pymethods]
impl PyPid201Stream {
    #[new]
    #[pyo3(signature = (max_message_len=None))]
    fn new(max_message_len: Option<usize>) -> Self {
        let adapter = if let Some(max_message_len) = max_message_len {
            Pid201StreamAdapter::with_max_message_len(max_message_len)
        } else {
            Pid201StreamAdapter::new()
        };
        Self { adapter }
    }

    fn push_json(&mut self, input: &[u8]) -> PyResult<String> {
        let raw_records = self.adapter.push(input).map_err(runtime_err)?;
        let mut records = Vec::with_capacity(raw_records.len());
        for record in raw_records {
            let inspection =
                inspect_bytes(&record.raw_message, IngestHint::RawBulletin).map_err(runtime_err)?;
            let Some(message) = inspection.messages.into_iter().next() else {
                return Err(PyRuntimeError::new_err(
                    "pid201 stream record did not contain a parsed bulletin",
                ));
            };
            records.push(Pid201PushRecord {
                offset: record.offset,
                leading_junk_prefix: record.leading_junk_prefix,
                message,
            });
        }

        to_json(&Pid201PushReport {
            records,
            pending_bytes: self.adapter.pending().len(),
        })
        .map_err(runtime_err)
    }

    fn pending<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.adapter.pending())
    }

    fn finish_json(&mut self) -> PyResult<String> {
        let state = self.adapter.finish();
        to_json(&state).map_err(runtime_err)
    }
}

#[pyclass(name = "NativeOiClient", unsendable)]
struct PyOiClient {
    client: Option<NwwsOiClient>,
    room_address: String,
}

#[pymethods]
impl PyOiClient {
    #[new]
    #[pyo3(signature = (
        username,
        password,
        count=None,
        history=None,
        host=None,
        domain=None,
        port=None,
        room=None,
        room_service=None,
        nickname=None,
        resource=None,
        room_password=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        username: String,
        password: String,
        count: Option<usize>,
        history: Option<u32>,
        host: Option<String>,
        domain: Option<String>,
        port: Option<u16>,
        room: Option<String>,
        room_service: Option<String>,
        nickname: Option<String>,
        resource: Option<String>,
        room_password: Option<String>,
    ) -> PyResult<Self> {
        let _ = count;
        let mut config = OiClientConfig::new(username, password);
        if let Some(history) = history {
            config.history_stanzas = history;
        }
        if let Some(host) = host {
            config.host = host;
        }
        if let Some(domain) = domain {
            config.domain = domain;
        }
        if let Some(port) = port {
            config.port = port;
        }
        if let Some(room) = room {
            config.room = room;
        }
        if let Some(room_service) = room_service {
            config.room_service = room_service;
        }
        if let Some(nickname) = nickname {
            config.nickname = nickname;
        }
        if let Some(resource) = resource {
            config.resource = resource;
        }
        if let Some(room_password) = room_password {
            config.room_password = Some(room_password);
        }
        let room_address = config.room_address();
        let client = NwwsOiClient::connect(config).map_err(runtime_err)?;
        Ok(Self {
            client: Some(client),
            room_address,
        })
    }

    fn jid(&self) -> Option<String> {
        self.client
            .as_ref()
            .and_then(|client| client.jid().map(str::to_owned))
    }

    fn room_address(&self) -> String {
        self.room_address.clone()
    }

    fn next_message_json(&mut self) -> PyResult<String> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client is closed"))?;
        let message = client.next_message().map_err(runtime_err)?;
        let report = inspect_oi_message(&message).map_err(runtime_err)?;
        to_json(&report).map_err(runtime_err)
    }

    fn read_messages_json(&mut self, count: usize) -> PyResult<String> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client is closed"))?;
        let mut reports = Vec::with_capacity(count);
        for _ in 0..count {
            let message = client.next_message().map_err(runtime_err)?;
            reports.push(inspect_oi_message(&message).map_err(runtime_err)?);
        }
        to_json(&reports).map_err(runtime_err)
    }

    fn close(&mut self) {
        self.client = None;
    }
}

#[pymodule]
#[pyo3(name = "_native")]
fn python_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(inspect_bytes_json, m)?)?;
    m.add_function(wrap_pyfunction!(inspect_text_json, m)?)?;
    m.add_function(wrap_pyfunction!(inspect_path_json, m)?)?;
    m.add_function(wrap_pyfunction!(scan_path_json, m)?)?;
    m.add_function(wrap_pyfunction!(split_pid201_bytes_json, m)?)?;
    m.add_function(wrap_pyfunction!(split_pid201_path_json, m)?)?;
    m.add_function(wrap_pyfunction!(write_pid201_split_json, m)?)?;
    m.add_function(wrap_pyfunction!(archive_import_json, m)?)?;
    m.add_function(wrap_pyfunction!(archive_verify_json, m)?)?;
    m.add_function(wrap_pyfunction!(collect_input_paths, m)?)?;
    m.add_class::<PyPid201Stream>()?;
    m.add_class::<PyOiClient>()?;
    Ok(())
}
