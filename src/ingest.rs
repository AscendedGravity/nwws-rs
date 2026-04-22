use crate::error::Result;
use crate::oi::NwwsOiMessage;
use crate::product::NwwsContent;
use crate::stream::{FramedChunk, WmoStreamScanner};
use crate::{ETX, SOH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestHint {
    Auto,
    OpenInterface,
    SatellitePid201,
    RawBulletin,
    FramedStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    OpenInterface,
    SatellitePid201,
    PlainWmoText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportDescriptor {
    pub kind: TransportKind,
    pub satellite_channel: Option<u16>,
    pub requires_authentication: bool,
    pub highest_availability_requires_pairing: bool,
}

impl TransportDescriptor {
    pub const fn open_interface() -> Self {
        Self {
            kind: TransportKind::OpenInterface,
            satellite_channel: None,
            requires_authentication: true,
            highest_availability_requires_pairing: true,
        }
    }

    pub const fn satellite_pid201() -> Self {
        Self {
            kind: TransportKind::SatellitePid201,
            satellite_channel: Some(201),
            requires_authentication: false,
            highest_availability_requires_pairing: true,
        }
    }

    pub const fn plain_wmo_text() -> Self {
        Self {
            kind: TransportKind::PlainWmoText,
            satellite_channel: None,
            requires_authentication: false,
            highest_availability_requires_pairing: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BulletinIngest<'a> {
    pub transport: TransportDescriptor,
    pub content: NwwsContent<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OiWrapperMetadata {
    pub cccc: String,
    pub ttaaii: String,
    pub awips_id: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OiIngest {
    pub transport: TransportDescriptor,
    pub message: NwwsOiMessage,
    pub wrapper: Option<OiWrapperMetadata>,
}

impl OiIngest {
    pub fn content(&self) -> Result<NwwsContent<'_>> {
        NwwsContent::from_oi_message(&self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedStreamIngest<'a> {
    pub transport: TransportDescriptor,
    pub leading_junk_prefix: usize,
    pub chunks: Vec<FramedChunk<'a>>,
    pub pending: &'a [u8],
}

impl<'a> FramedStreamIngest<'a> {
    pub fn contents(&self) -> Result<Vec<NwwsContent<'a>>> {
        self.chunks
            .iter()
            .map(|chunk| NwwsContent::parse_bulletin(chunk.bytes))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedInput<'a> {
    Bulletin(BulletinIngest<'a>),
    OpenInterface(OiIngest),
    FramedStream(FramedStreamIngest<'a>),
}

impl ParsedInput<'_> {
    pub const fn transport(&self) -> TransportDescriptor {
        match self {
            Self::Bulletin(value) => value.transport,
            Self::OpenInterface(value) => value.transport,
            Self::FramedStream(value) => value.transport,
        }
    }
}

pub fn parse_with_hint<'a>(hint: IngestHint, input: &'a [u8]) -> Result<ParsedInput<'a>> {
    match hint {
        IngestHint::Auto => parse_auto(input),
        IngestHint::OpenInterface => parse_oi(input),
        IngestHint::SatellitePid201 => {
            parse_framed_stream(input, TransportDescriptor::satellite_pid201())
        }
        IngestHint::RawBulletin => parse_bulletin(input, TransportDescriptor::plain_wmo_text()),
        IngestHint::FramedStream => {
            parse_framed_stream(input, TransportDescriptor::plain_wmo_text())
        }
    }
}

pub fn parse_auto<'a>(input: &'a [u8]) -> Result<ParsedInput<'a>> {
    if looks_like_oi(input) {
        return parse_oi(input);
    }
    if looks_like_framed_stream(input) {
        return parse_framed_stream(input, TransportDescriptor::satellite_pid201());
    }
    parse_bulletin(input, TransportDescriptor::plain_wmo_text())
}

fn parse_oi(input: &[u8]) -> Result<ParsedInput<'_>> {
    let text = std::str::from_utf8(input)
        .map_err(|_| crate::ParseError::new(crate::ErrorKind::InvalidUtf8))?;
    let message = NwwsOiMessage::parse(text)?;
    let wrapper = message.payload.as_ref().map(|payload| OiWrapperMetadata {
        cccc: payload.cccc.clone(),
        ttaaii: payload.ttaaii.clone(),
        awips_id: payload.awips_id.clone(),
        id: format!("{}.{}", payload.id.process_id, payload.id.sequence),
    });
    Ok(ParsedInput::OpenInterface(OiIngest {
        transport: TransportDescriptor::open_interface(),
        message,
        wrapper,
    }))
}

fn parse_bulletin<'a>(input: &'a [u8], transport: TransportDescriptor) -> Result<ParsedInput<'a>> {
    let content = NwwsContent::parse_bulletin(input)?;
    Ok(ParsedInput::Bulletin(BulletinIngest { transport, content }))
}

fn parse_framed_stream<'a>(
    input: &'a [u8],
    transport: TransportDescriptor,
) -> Result<ParsedInput<'a>> {
    let scanner = WmoStreamScanner::new();
    let first = scanner.scan_next(input)?;
    let mut chunks = Vec::new();
    let mut remaining = input;
    let mut offset = 0usize;

    loop {
        let outcome = scanner.scan_next(remaining)?;
        let Some(mut chunk) = outcome.chunk else {
            let pending = if first.chunk.is_none() && chunks.is_empty() {
                first.pending
            } else {
                remaining
            };
            return Ok(ParsedInput::FramedStream(FramedStreamIngest {
                transport,
                leading_junk_prefix: first.junk_prefix,
                chunks,
                pending,
            }));
        };

        chunk.range = (offset + chunk.range.start)..(offset + chunk.range.end);
        offset = chunk.range.end;
        remaining = outcome.pending;
        chunks.push(chunk);

        if remaining.is_empty() {
            return Ok(ParsedInput::FramedStream(FramedStreamIngest {
                transport,
                leading_junk_prefix: first.junk_prefix,
                chunks,
                pending: remaining,
            }));
        }
    }
}

pub fn looks_like_oi(input: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(input) else {
        return false;
    };
    let trimmed = text.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    trimmed.starts_with('<')
        && (trimmed.contains("xmlns='nwws-oi'")
            || trimmed.contains("xmlns=\"nwws-oi\"")
            || trimmed.starts_with("<message"))
}

pub fn looks_like_framed_stream(input: &[u8]) -> bool {
    match (
        input.iter().position(|byte| *byte == SOH),
        input.iter().position(|byte| *byte == ETX),
    ) {
        (Some(start), Some(end)) => start < end,
        _ => input.first() == Some(&SOH),
    }
}

#[cfg(test)]
mod tests {
    use super::{IngestHint, ParsedInput, TransportKind, parse_auto, parse_with_hint};

    #[test]
    fn auto_detects_open_interface() {
        let xml = include_bytes!("../tests/fixtures/nwws_oi_example.xml");
        let parsed = parse_auto(xml).unwrap();
        match parsed {
            ParsedInput::OpenInterface(value) => {
                assert_eq!(value.transport.kind, TransportKind::OpenInterface);
                assert_eq!(value.wrapper.as_ref().unwrap().awips_id, "RR8ARX");
                let content = value.content().unwrap();
                assert_eq!(content.bulletin.awips_id.unwrap().raw(), "RR8ARX");
            }
            other => panic!("unexpected parse variant: {other:?}"),
        }
    }

    #[test]
    fn auto_detects_bare_bulletin() {
        let bulletin = include_bytes!("../tests/fixtures/wmo_tornado_warning.txt");
        let parsed = parse_auto(bulletin).unwrap();
        match parsed {
            ParsedInput::Bulletin(value) => {
                assert_eq!(value.transport.kind, TransportKind::PlainWmoText);
                assert_eq!(value.content.bulletin.heading.cccc(), "KLOT");
            }
            other => panic!("unexpected parse variant: {other:?}"),
        }
    }

    #[test]
    fn satellite_hint_parses_framed_stream() {
        let framed =
            "\u{1}\r\r\n111\r\r\nNOUS41 KWBC 201530 AAA\r\r\nPNSXXX\r\r\nHeadline\r\r\n\u{3}";
        let parsed = parse_with_hint(IngestHint::SatellitePid201, framed.as_bytes()).unwrap();
        match parsed {
            ParsedInput::FramedStream(value) => {
                assert_eq!(value.transport.kind, TransportKind::SatellitePid201);
                assert_eq!(value.transport.satellite_channel, Some(201));
                assert_eq!(value.chunks.len(), 1);
                let contents = value.contents().unwrap();
                assert_eq!(contents[0].bulletin.awips_id.unwrap().raw(), "PNSXXX");
            }
            other => panic!("unexpected parse variant: {other:?}"),
        }
    }
}
