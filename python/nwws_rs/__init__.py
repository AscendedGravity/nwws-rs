from __future__ import annotations

import json
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, Iterator, Mapping, Optional, Sequence, Union

from . import _native

try:
    from enum import StrEnum
except ImportError:  # pragma: no cover
    class StrEnum(str, Enum):
        pass


PathLike = Union[str, Path]


class Hint(StrEnum):
    AUTO = "auto"
    OPEN_INTERFACE = "oi"
    PID201 = "pid201"
    BULLETIN = "bulletin"
    FRAMED_STREAM = "stream"


class InputKind(StrEnum):
    OPEN_INTERFACE = "open-interface"
    BULLETIN = "bulletin"
    FRAMED_STREAM = "framed-stream"


@dataclass(frozen=True, slots=True)
class ByteRange:
    start: int
    end: int


@dataclass(frozen=True, slots=True)
class WrapperSummary:
    summary: Optional[str]
    id: Optional[str]
    issue: Optional[str]


@dataclass(frozen=True, slots=True)
class TransportInfo:
    kind: str
    satellite_channel: Optional[int]
    requires_authentication: bool
    paired_transport_recommended: bool


@dataclass(frozen=True, slots=True)
class Point:
    lat: float
    lon: float


@dataclass(frozen=True, slots=True)
class TimeMotLoc:
    time: str
    direction_degrees: int
    speed_knots: int
    locations: tuple[Point, ...]


@dataclass(frozen=True, slots=True)
class Segment:
    headline: Optional[str]
    body_lines: tuple[str, ...]
    separated_by_dollars: bool
    contains_andand: bool
    ugc_raw: str
    ugcs: tuple[str, ...]
    pvtec: tuple[str, ...]
    hvtec: tuple[str, ...]
    tornado_tag: Optional[str]
    flash_flood_observed: bool
    flash_flood_emergency: bool
    hail_inches: Optional[float]
    wind_mph: Optional[int]
    damage_threat: Optional[str]
    lat_lon: Optional[tuple[Point, ...]]
    time_mot_loc: Optional[TimeMotLoc]


@dataclass(frozen=True, slots=True)
class Message:
    byte_range: Optional[ByteRange]
    wrapper: Optional[WrapperSummary]
    frame_kind: str
    sequence_number: Optional[int]
    heading: str
    ttaaii: str
    cccc: str
    yygggg: str
    bbb: Optional[str]
    awips_id: Optional[str]
    family: str
    semantic_fingerprint: str
    segment_count: int
    segments: tuple[Segment, ...]
    raw_bulletin: str


@dataclass(frozen=True, slots=True)
class InspectionReport:
    path: Optional[Path]
    input_kind: str
    transport: TransportInfo
    junk_bytes: int
    pending_bytes: int
    messages: tuple[Message, ...]


@dataclass(frozen=True, slots=True)
class ScanCount:
    input_kind: str
    transport: str
    files: int
    messages: int


@dataclass(frozen=True, slots=True)
class ScanFileResult:
    path: Path
    report: Optional[InspectionReport]
    error: Optional[str]


@dataclass(frozen=True, slots=True)
class ScanReport:
    root: Path
    scanned_files: int
    parsed_files: int
    messages: int
    failures: int
    counts: tuple[ScanCount, ...]
    families: dict[str, int]
    files: tuple[ScanFileResult, ...]


@dataclass(frozen=True, slots=True)
class Pid201SplitRecord:
    index: int
    suggested_filename: str
    message: Message


@dataclass(frozen=True, slots=True)
class Pid201SplitReport:
    source_path: Optional[Path]
    transport: TransportInfo
    junk_bytes: int
    pending_bytes: int
    records: tuple[Pid201SplitRecord, ...]


@dataclass(frozen=True, slots=True)
class Pid201WriteRecord:
    path: Path
    message: Message


@dataclass(frozen=True, slots=True)
class Pid201WriteReport:
    input_path: Path
    output_dir: Path
    junk_bytes: int
    pending_bytes: int
    written: tuple[Pid201WriteRecord, ...]


@dataclass(frozen=True, slots=True)
class ArchivePersistResult:
    source_path: Path
    action: str
    relative_path: Path
    transport: str
    heading: str
    family: str


@dataclass(frozen=True, slots=True)
class ArchiveFailure:
    source_path: Path
    error: str


@dataclass(frozen=True, slots=True)
class ArchiveImportReport:
    archive_dir: Path
    scanned_inputs: int
    parsed_inputs: int
    archived_records: int
    duplicate_records: int
    failures: int
    transports: dict[str, int]
    families: dict[str, int]
    records: tuple[ArchivePersistResult, ...]
    errors: tuple[ArchiveFailure, ...]


@dataclass(frozen=True, slots=True)
class ArchiveVerifyRecord:
    path: Path
    status: str
    heading: Optional[str]
    family: Optional[str]
    error: Optional[str]


@dataclass(frozen=True, slots=True)
class ArchiveVerifyReport:
    archive_dir: Path
    verified_records: int
    failures: int
    families: dict[str, int]
    records: tuple[ArchiveVerifyRecord, ...]


@dataclass(frozen=True, slots=True)
class ActiveWarningRecord:
    key: str
    source_path: Path
    message_index: int
    segment_index: int
    vtec_index: int
    heading: str
    issued_at: Optional[str]
    office: str
    message_office: str
    awips_id: Optional[str]
    product_family: str
    event_family: str
    event_class: str
    action: str
    phenomenon: str
    significance: str
    event_tracking_number: int
    start_time: Optional[str]
    end_time: Optional[str]
    vtec: str
    ugc_raw: str
    ugcs: tuple[str, ...]
    headline: Optional[str]
    raw_bulletin_blake3: str
    archive_id: str


@dataclass(frozen=True, slots=True)
class ActiveWarningFailure:
    path: Path
    error: str


@dataclass(frozen=True, slots=True)
class ActiveWarningReport:
    root: Path
    reference_utc: str
    scanned_files: int
    parsed_files: int
    messages: int
    warning_vtec_segments: int
    future_messages: int
    active_records: int
    failures: int
    families: dict[str, int]
    records: tuple[ActiveWarningRecord, ...]
    errors: tuple[ActiveWarningFailure, ...]


@dataclass(frozen=True, slots=True)
class Pid201PushRecord:
    offset: int
    leading_junk_prefix: int
    message: Message


@dataclass(frozen=True, slots=True)
class Pid201PushReport:
    records: tuple[Pid201PushRecord, ...]
    pending_bytes: int


@dataclass(frozen=True, slots=True)
class Pid201DrainState:
    discarded_junk: int
    pending_bytes: int


def _coerce_hint(hint: Optional[Union[Hint, str]]) -> Optional[str]:
    if hint is None:
        return None
    if isinstance(hint, Hint):
        return hint.value
    return str(hint)


def _loads(raw: str) -> Any:
    return json.loads(raw)


def _as_path(value: Optional[str]) -> Optional[Path]:
    return None if value is None else Path(value)


def _decode_transport(payload: Mapping[str, Any]) -> TransportInfo:
    return TransportInfo(
        kind=payload["kind"],
        satellite_channel=payload["satellite_channel"],
        requires_authentication=payload["requires_authentication"],
        paired_transport_recommended=payload["paired_transport_recommended"],
    )


def _decode_point(payload: Mapping[str, Any]) -> Point:
    return Point(lat=payload["lat"], lon=payload["lon"])


def _decode_time_mot_loc(payload: Optional[Mapping[str, Any]]) -> Optional[TimeMotLoc]:
    if payload is None:
        return None
    return TimeMotLoc(
        time=payload["time"],
        direction_degrees=payload["direction_degrees"],
        speed_knots=payload["speed_knots"],
        locations=tuple(_decode_point(value) for value in payload["locations"]),
    )


def _decode_segment(payload: Mapping[str, Any]) -> Segment:
    lat_lon = payload["lat_lon"]
    return Segment(
        headline=payload["headline"],
        body_lines=tuple(payload["body_lines"]),
        separated_by_dollars=payload["separated_by_dollars"],
        contains_andand=payload["contains_andand"],
        ugc_raw=payload["ugc_raw"],
        ugcs=tuple(payload["ugcs"]),
        pvtec=tuple(payload["pvtec"]),
        hvtec=tuple(payload["hvtec"]),
        tornado_tag=payload["tornado_tag"],
        flash_flood_observed=payload["flash_flood_observed"],
        flash_flood_emergency=payload["flash_flood_emergency"],
        hail_inches=payload["hail_inches"],
        wind_mph=payload["wind_mph"],
        damage_threat=payload["damage_threat"],
        lat_lon=None if lat_lon is None else tuple(_decode_point(value) for value in lat_lon),
        time_mot_loc=_decode_time_mot_loc(payload["time_mot_loc"]),
    )


def _decode_wrapper(payload: Optional[Mapping[str, Any]]) -> Optional[WrapperSummary]:
    if payload is None:
        return None
    return WrapperSummary(
        summary=payload["summary"],
        id=payload["id"],
        issue=payload["issue"],
    )


def _decode_message(payload: Mapping[str, Any]) -> Message:
    byte_range = payload["byte_range"]
    return Message(
        byte_range=None if byte_range is None else ByteRange(**byte_range),
        wrapper=_decode_wrapper(payload["wrapper"]),
        frame_kind=payload["frame_kind"],
        sequence_number=payload["sequence_number"],
        heading=payload["heading"],
        ttaaii=payload["ttaaii"],
        cccc=payload["cccc"],
        yygggg=payload["yygggg"],
        bbb=payload["bbb"],
        awips_id=payload["awips_id"],
        family=payload["family"],
        semantic_fingerprint=payload["semantic_fingerprint"],
        segment_count=payload["segment_count"],
        segments=tuple(_decode_segment(value) for value in payload["segments"]),
        raw_bulletin=payload["raw_bulletin"],
    )


def _decode_inspection(payload: Mapping[str, Any]) -> InspectionReport:
    return InspectionReport(
        path=_as_path(payload["path"]),
        input_kind=payload["input_kind"],
        transport=_decode_transport(payload["transport"]),
        junk_bytes=payload["junk_bytes"],
        pending_bytes=payload["pending_bytes"],
        messages=tuple(_decode_message(value) for value in payload["messages"]),
    )


def _decode_scan_count(payload: Mapping[str, Any]) -> ScanCount:
    return ScanCount(
        input_kind=payload["input_kind"],
        transport=payload["transport"],
        files=payload["files"],
        messages=payload["messages"],
    )


def _decode_scan_file(payload: Mapping[str, Any]) -> ScanFileResult:
    report = payload["report"]
    return ScanFileResult(
        path=Path(payload["path"]),
        report=None if report is None else _decode_inspection(report),
        error=payload["error"],
    )


def _decode_scan_report(payload: Mapping[str, Any]) -> ScanReport:
    return ScanReport(
        root=Path(payload["root"]),
        scanned_files=payload["scanned_files"],
        parsed_files=payload["parsed_files"],
        messages=payload["messages"],
        failures=payload["failures"],
        counts=tuple(_decode_scan_count(value) for value in payload["counts"]),
        families=dict(payload["families"]),
        files=tuple(_decode_scan_file(value) for value in payload["files"]),
    )


def _decode_pid201_split_record(payload: Mapping[str, Any]) -> Pid201SplitRecord:
    return Pid201SplitRecord(
        index=payload["index"],
        suggested_filename=payload["suggested_filename"],
        message=_decode_message(payload["message"]),
    )


def _decode_pid201_split_report(payload: Mapping[str, Any]) -> Pid201SplitReport:
    return Pid201SplitReport(
        source_path=_as_path(payload["source_path"]),
        transport=_decode_transport(payload["transport"]),
        junk_bytes=payload["junk_bytes"],
        pending_bytes=payload["pending_bytes"],
        records=tuple(_decode_pid201_split_record(value) for value in payload["records"]),
    )


def _decode_pid201_write_record(payload: Mapping[str, Any]) -> Pid201WriteRecord:
    return Pid201WriteRecord(
        path=Path(payload["path"]),
        message=_decode_message(payload["message"]),
    )


def _decode_pid201_write_report(payload: Mapping[str, Any]) -> Pid201WriteReport:
    return Pid201WriteReport(
        input_path=Path(payload["input_path"]),
        output_dir=Path(payload["output_dir"]),
        junk_bytes=payload["junk_bytes"],
        pending_bytes=payload["pending_bytes"],
        written=tuple(_decode_pid201_write_record(value) for value in payload["written"]),
    )


def _decode_archive_persist(payload: Mapping[str, Any]) -> ArchivePersistResult:
    return ArchivePersistResult(
        source_path=Path(payload["source_path"]),
        action=payload["action"],
        relative_path=Path(payload["relative_path"]),
        transport=payload["transport"],
        heading=payload["heading"],
        family=payload["family"],
    )


def _decode_archive_failure(payload: Mapping[str, Any]) -> ArchiveFailure:
    return ArchiveFailure(
        source_path=Path(payload["source_path"]),
        error=payload["error"],
    )


def _decode_archive_import(payload: Mapping[str, Any]) -> ArchiveImportReport:
    return ArchiveImportReport(
        archive_dir=Path(payload["archive_dir"]),
        scanned_inputs=payload["scanned_inputs"],
        parsed_inputs=payload["parsed_inputs"],
        archived_records=payload["archived_records"],
        duplicate_records=payload["duplicate_records"],
        failures=payload["failures"],
        transports=dict(payload["transports"]),
        families=dict(payload["families"]),
        records=tuple(_decode_archive_persist(value) for value in payload["records"]),
        errors=tuple(_decode_archive_failure(value) for value in payload["errors"]),
    )


def _decode_archive_verify_record(payload: Mapping[str, Any]) -> ArchiveVerifyRecord:
    return ArchiveVerifyRecord(
        path=Path(payload["path"]),
        status=payload["status"],
        heading=payload["heading"],
        family=payload["family"],
        error=payload["error"],
    )


def _decode_archive_verify(payload: Mapping[str, Any]) -> ArchiveVerifyReport:
    return ArchiveVerifyReport(
        archive_dir=Path(payload["archive_dir"]),
        verified_records=payload["verified_records"],
        failures=payload["failures"],
        families=dict(payload["families"]),
        records=tuple(_decode_archive_verify_record(value) for value in payload["records"]),
    )


def _decode_active_warning_record(payload: Mapping[str, Any]) -> ActiveWarningRecord:
    return ActiveWarningRecord(
        key=payload["key"],
        source_path=Path(payload["source_path"]),
        message_index=payload["message_index"],
        segment_index=payload["segment_index"],
        vtec_index=payload["vtec_index"],
        heading=payload["heading"],
        issued_at=payload["issued_at"],
        office=payload["office"],
        message_office=payload["message_office"],
        awips_id=payload["awips_id"],
        product_family=payload["product_family"],
        event_family=payload["event_family"],
        event_class=payload["event_class"],
        action=payload["action"],
        phenomenon=payload["phenomenon"],
        significance=payload["significance"],
        event_tracking_number=payload["event_tracking_number"],
        start_time=payload["start_time"],
        end_time=payload["end_time"],
        vtec=payload["vtec"],
        ugc_raw=payload["ugc_raw"],
        ugcs=tuple(payload["ugcs"]),
        headline=payload["headline"],
        raw_bulletin_blake3=payload["raw_bulletin_blake3"],
        archive_id=payload["archive_id"],
    )


def _decode_active_warning_failure(payload: Mapping[str, Any]) -> ActiveWarningFailure:
    return ActiveWarningFailure(
        path=Path(payload["path"]),
        error=payload["error"],
    )


def _decode_active_warning_report(payload: Mapping[str, Any]) -> ActiveWarningReport:
    return ActiveWarningReport(
        root=Path(payload["root"]),
        reference_utc=payload["reference_utc"],
        scanned_files=payload["scanned_files"],
        parsed_files=payload["parsed_files"],
        messages=payload["messages"],
        warning_vtec_segments=payload["warning_vtec_segments"],
        future_messages=payload["future_messages"],
        active_records=payload["active_records"],
        failures=payload["failures"],
        families=dict(payload["families"]),
        records=tuple(_decode_active_warning_record(value) for value in payload["records"]),
        errors=tuple(_decode_active_warning_failure(value) for value in payload["errors"]),
    )


def _decode_pid201_push_record(payload: Mapping[str, Any]) -> Pid201PushRecord:
    return Pid201PushRecord(
        offset=payload["offset"],
        leading_junk_prefix=payload["leading_junk_prefix"],
        message=_decode_message(payload["message"]),
    )


def _decode_pid201_push(payload: Mapping[str, Any]) -> Pid201PushReport:
    return Pid201PushReport(
        records=tuple(_decode_pid201_push_record(value) for value in payload["records"]),
        pending_bytes=payload["pending_bytes"],
    )


def _decode_pid201_drain(payload: Mapping[str, Any]) -> Pid201DrainState:
    return Pid201DrainState(
        discarded_junk=payload["discarded_junk"],
        pending_bytes=payload["pending_bytes"],
    )


def inspect_bytes(data: Union[bytes, bytearray, memoryview], hint: Union[Hint, str] = Hint.AUTO) -> InspectionReport:
    payload = _loads(_native.inspect_bytes_json(bytes(data), _coerce_hint(hint)))
    return _decode_inspection(payload)


def inspect_text(text: str, hint: Union[Hint, str] = Hint.AUTO) -> InspectionReport:
    payload = _loads(_native.inspect_text_json(text, _coerce_hint(hint)))
    return _decode_inspection(payload)


def inspect_path(path: PathLike, hint: Optional[Union[Hint, str]] = None) -> InspectionReport:
    payload = _loads(_native.inspect_path_json(str(path), _coerce_hint(hint)))
    return _decode_inspection(payload)


def parse(data: Union[bytes, bytearray, memoryview, str], hint: Union[Hint, str] = Hint.AUTO) -> InspectionReport:
    if isinstance(data, str):
        return inspect_text(data, hint=hint)
    return inspect_bytes(data, hint=hint)


def parse_path(path: PathLike, hint: Optional[Union[Hint, str]] = None) -> InspectionReport:
    return inspect_path(path, hint=hint)


def parse_bulletin(data: Union[bytes, bytearray, memoryview, str]) -> Message:
    report = parse(data, hint=Hint.BULLETIN)
    if len(report.messages) != 1:
        raise ValueError(f"expected one bulletin, found {len(report.messages)}")
    return report.messages[0]


def parse_oi(xml: str) -> Message:
    report = inspect_text(xml, hint=Hint.OPEN_INTERFACE)
    if len(report.messages) != 1:
        raise ValueError(f"expected one NWWS-OI message, found {len(report.messages)}")
    return report.messages[0]


def scan_path(path: PathLike, hint: Optional[Union[Hint, str]] = None) -> ScanReport:
    payload = _loads(_native.scan_path_json(str(path), _coerce_hint(hint)))
    return _decode_scan_report(payload)


def collect_input_paths(root: PathLike) -> list[Path]:
    return [Path(value) for value in _native.collect_input_paths(str(root))]


def split_pid201_bytes(data: Union[bytes, bytearray, memoryview]) -> Pid201SplitReport:
    payload = _loads(_native.split_pid201_bytes_json(bytes(data)))
    return _decode_pid201_split_report(payload)


def split_pid201_file(path: PathLike) -> Pid201SplitReport:
    payload = _loads(_native.split_pid201_path_json(str(path)))
    return _decode_pid201_split_report(payload)


def split_pid201(data: Union[bytes, bytearray, memoryview]) -> Pid201SplitReport:
    return split_pid201_bytes(data)


def write_pid201_split(input_path: PathLike, output_dir: PathLike) -> Pid201WriteReport:
    payload = _loads(_native.write_pid201_split_json(str(input_path), str(output_dir)))
    return _decode_pid201_write_report(payload)


def archive_import(
    input_path: PathLike,
    archive_dir: PathLike,
    hint: Optional[Union[Hint, str]] = None,
) -> ArchiveImportReport:
    payload = _loads(_native.archive_import_json(str(input_path), str(archive_dir), _coerce_hint(hint)))
    return _decode_archive_import(payload)


def archive_verify(archive_dir: PathLike) -> ArchiveVerifyReport:
    payload = _loads(_native.archive_verify_json(str(archive_dir)))
    return _decode_archive_verify(payload)


def active_warnings_at(
    path: PathLike,
    reference_utc: str,
    hint: Optional[Union[Hint, str]] = None,
) -> ActiveWarningReport:
    payload = _loads(_native.active_warnings_at_json(str(path), reference_utc, _coerce_hint(hint)))
    return _decode_active_warning_report(payload)


def semantic_fingerprint(message: Message) -> str:
    return message.semantic_fingerprint


class Pid201Stream:
    def __init__(self, max_message_len: Optional[int] = None) -> None:
        self._native = _native.NativePid201Stream(max_message_len)

    def push(self, data: Union[bytes, bytearray, memoryview]) -> Pid201PushReport:
        payload = _loads(self._native.push_json(bytes(data)))
        return _decode_pid201_push(payload)

    def pending(self) -> bytes:
        return bytes(self._native.pending())

    def finish(self) -> Pid201DrainState:
        payload = _loads(self._native.finish_json())
        return _decode_pid201_drain(payload)


class OiClient:
    def __init__(
        self,
        username: str,
        password: str,
        *,
        history: Optional[int] = None,
        host: Optional[str] = None,
        domain: Optional[str] = None,
        port: Optional[int] = None,
        room: Optional[str] = None,
        room_service: Optional[str] = None,
        nickname: Optional[str] = None,
        resource: Optional[str] = None,
        room_password: Optional[str] = None,
    ) -> None:
        self._native = _native.NativeOiClient(
            username,
            password,
            None,
            history,
            host,
            domain,
            port,
            room,
            room_service,
            nickname,
            resource,
            room_password,
        )

    @property
    def jid(self) -> Optional[str]:
        return self._native.jid()

    @property
    def room_address(self) -> str:
        return self._native.room_address()

    def next_message(self) -> InspectionReport:
        payload = _loads(self._native.next_message_json())
        return _decode_inspection(payload)

    def read_messages(self, count: int) -> tuple[InspectionReport, ...]:
        payload = _loads(self._native.read_messages_json(count))
        return tuple(_decode_inspection(value) for value in payload)

    def iter_messages(self, limit: Optional[int] = None) -> Iterator[InspectionReport]:
        seen = 0
        while limit is None or seen < limit:
            yield self.next_message()
            seen += 1

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> "OiClient":
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.close()


OpenInterfaceClient = OiClient
__version__ = _native.__version__

__all__ = [
    "ArchiveFailure",
    "ArchiveImportReport",
    "ArchivePersistResult",
    "ArchiveVerifyRecord",
    "ArchiveVerifyReport",
    "ActiveWarningFailure",
    "ActiveWarningRecord",
    "ActiveWarningReport",
    "ByteRange",
    "Hint",
    "InputKind",
    "InspectionReport",
    "Message",
    "OiClient",
    "OpenInterfaceClient",
    "PathLike",
    "Pid201DrainState",
    "Pid201PushRecord",
    "Pid201PushReport",
    "Pid201SplitRecord",
    "Pid201SplitReport",
    "Pid201Stream",
    "Pid201WriteRecord",
    "Pid201WriteReport",
    "Point",
    "ScanCount",
    "ScanFileResult",
    "ScanReport",
    "Segment",
    "TimeMotLoc",
    "TransportInfo",
    "WrapperSummary",
    "active_warnings_at",
    "archive_import",
    "archive_verify",
    "collect_input_paths",
    "inspect_bytes",
    "inspect_path",
    "inspect_text",
    "parse",
    "parse_bulletin",
    "parse_oi",
    "parse_path",
    "scan_path",
    "semantic_fingerprint",
    "split_pid201",
    "split_pid201_bytes",
    "split_pid201_file",
    "write_pid201_split",
]
