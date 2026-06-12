use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesCData, BytesStart, BytesText, Event};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{ErrorKind, ParseError, Result};
use crate::wmo::WmoMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NwwsOiMessage {
    pub stanza_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub summary: Option<String>,
    pub xhtml_summary: Option<String>,
    pub payload: Option<NwwsOiPayload>,
}

impl NwwsOiMessage {
    pub fn parse(input: &str) -> Result<Self> {
        parse_message(input)
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(payload) = &self.payload {
            payload.validate()?;
        }
        Ok(())
    }

    /// Re-serialize this message into canonical NWWS-OI archive XML so live
    /// captures round-trip through the same ingest path as archived stanzas.
    ///
    /// Fails with `MissingField` when the message carries no `<x xmlns='nwws-oi'>`
    /// payload (presence updates, plain chat).
    pub fn to_archive_xml(&self) -> Result<String> {
        let payload = self
            .payload
            .as_ref()
            .ok_or_else(|| ParseError::new(ErrorKind::MissingField("nwws-oi payload")))?;
        let issue = payload
            .issue
            .format(&Rfc3339)
            .map_err(|_| ParseError::new(ErrorKind::InvalidField("nwws-oi issue time")))?;
        let mut xml = String::new();

        xml.push_str("<message");
        push_xml_attr(
            &mut xml,
            "type",
            self.stanza_type.as_deref().unwrap_or("groupchat"),
        );
        if let Some(from) = self.from.as_deref() {
            push_xml_attr(&mut xml, "from", from);
        }
        if let Some(to) = self.to.as_deref() {
            push_xml_attr(&mut xml, "to", to);
        }
        xml.push('>');

        if let Some(summary) = self.summary.as_deref() {
            xml.push_str("<body>");
            xml.push_str(&escape_xml_text(summary));
            xml.push_str("</body>");
        }
        if let Some(summary) = self.xhtml_summary.as_deref() {
            xml.push_str("<html xmlns='http://jabber.org/protocol/xhtml-im'><body xmlns='http://www.w3.org/1999/xhtml'>");
            xml.push_str(&escape_xml_text(summary));
            xml.push_str("</body></html>");
        }

        xml.push_str("<x xmlns='nwws-oi'");
        push_xml_attr(&mut xml, "cccc", &payload.cccc);
        push_xml_attr(&mut xml, "ttaaii", &payload.ttaaii);
        push_xml_attr(&mut xml, "issue", &issue);
        push_xml_attr(&mut xml, "awipsid", &payload.awips_id);
        push_xml_attr(
            &mut xml,
            "id",
            &format!("{}.{}", payload.id.process_id, payload.id.sequence),
        );
        xml.push('>');
        xml.push_str(&escape_xml_text(&payload.raw_bulletin));
        xml.push_str("</x></message>");

        Ok(xml)
    }
}

fn push_xml_attr(xml: &mut String, key: &str, value: &str) {
    xml.push(' ');
    xml.push_str(key);
    xml.push_str("='");
    xml.push_str(&escape_xml_attr(value));
    xml.push('\'');
}

fn escape_xml_attr(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&apos;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NwwsOiPayload {
    pub cccc: String,
    pub ttaaii: String,
    pub issue: OffsetDateTime,
    pub awips_id: String,
    pub id: NwwsOiId,
    pub raw_bulletin: String,
}

impl NwwsOiPayload {
    pub fn parse_bulletin(&self) -> Result<WmoMessage<'_>> {
        let bulletin = WmoMessage::parse_str(&self.raw_bulletin)?;
        bulletin.verify_metadata(&self.ttaaii, &self.cccc, Some(&self.awips_id))?;
        Ok(bulletin)
    }

    pub fn validate(&self) -> Result<()> {
        let _ = self.parse_bulletin()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NwwsOiId {
    pub process_id: u32,
    pub sequence: u64,
}

impl NwwsOiId {
    pub fn parse(input: &str) -> Result<Self> {
        let (process_id, sequence) = input
            .split_once('.')
            .ok_or_else(|| ParseError::new(ErrorKind::InvalidField("nwws id")))?;
        let process_id = process_id
            .parse()
            .map_err(|_| ParseError::new(ErrorKind::InvalidField("nwws id")))?;
        let sequence = sequence
            .parse()
            .map_err(|_| ParseError::new(ErrorKind::InvalidField("nwws id")))?;
        Ok(Self {
            process_id,
            sequence,
        })
    }
}

fn parse_message(input: &str) -> Result<NwwsOiMessage> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);

    let mut message = NwwsOiMessage {
        stanza_type: None,
        from: None,
        to: None,
        summary: None,
        xhtml_summary: None,
        payload: None,
    };

    let mut in_html = false;
    let mut target = None;
    let mut payload_builder: Option<PayloadBuilder> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.name().as_ref() {
                b"message" => {
                    message.stanza_type = find_attr(&element, b"type")?;
                    message.from = find_attr(&element, b"from")?;
                    message.to = find_attr(&element, b"to")?;
                }
                b"html" => in_html = true,
                b"body" => {
                    target = Some(if in_html {
                        TextTarget::XhtmlSummary
                    } else {
                        TextTarget::Summary
                    });
                }
                b"x" if find_attr(&element, b"xmlns")?.as_deref() == Some("nwws-oi") => {
                    payload_builder = Some(PayloadBuilder::from_start(&element)?);
                }
                _ => {}
            },
            Ok(Event::Empty(element)) => {
                if element.name().as_ref() == b"x"
                    && find_attr(&element, b"xmlns")?.as_deref() == Some("nwws-oi")
                {
                    message.payload = Some(PayloadBuilder::from_start(&element)?.finish());
                }
            }
            Ok(Event::End(element)) => match element.name().as_ref() {
                b"body" => target = None,
                b"html" => in_html = false,
                b"x" => {
                    if let Some(builder) = payload_builder.take() {
                        message.payload = Some(builder.finish());
                    }
                }
                _ => {}
            },
            Ok(Event::Text(text)) => {
                let chunk = decode_text(&text)?;
                if let Some(payload) = &mut payload_builder {
                    payload.raw_bulletin.push_str(&chunk);
                } else {
                    append_text(
                        &mut message.summary,
                        &mut message.xhtml_summary,
                        target,
                        &chunk,
                    );
                }
            }
            Ok(Event::CData(text)) => {
                let chunk = decode_cdata(&text)?;
                if let Some(payload) = &mut payload_builder {
                    payload.raw_bulletin.push_str(&chunk);
                } else {
                    append_text(
                        &mut message.summary,
                        &mut message.xhtml_summary,
                        target,
                        &chunk,
                    );
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(ParseError::new(ErrorKind::InvalidXml("reader error"))),
        }
    }

    Ok(message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    Summary,
    XhtmlSummary,
}

#[derive(Debug, Clone)]
struct PayloadBuilder {
    cccc: String,
    ttaaii: String,
    issue: OffsetDateTime,
    awips_id: String,
    id: NwwsOiId,
    raw_bulletin: String,
}

impl PayloadBuilder {
    fn from_start(element: &BytesStart<'_>) -> Result<Self> {
        let cccc = required_attr(element, b"cccc")?;
        let ttaaii = required_attr(element, b"ttaaii")?;
        let issue_raw = required_attr(element, b"issue")?;
        let issue = OffsetDateTime::parse(&issue_raw, &Rfc3339)
            .map_err(|_| ParseError::new(ErrorKind::InvalidField("issue")))?;
        let awips_id = required_attr(element, b"awipsid")?;
        let id = NwwsOiId::parse(&required_attr(element, b"id")?)?;

        Ok(Self {
            cccc,
            ttaaii,
            issue,
            awips_id,
            id,
            raw_bulletin: String::new(),
        })
    }

    fn finish(self) -> NwwsOiPayload {
        NwwsOiPayload {
            cccc: self.cccc,
            ttaaii: self.ttaaii,
            issue: self.issue,
            awips_id: self.awips_id,
            id: self.id,
            raw_bulletin: trim_bulletin_edges(&self.raw_bulletin),
        }
    }
}

fn append_text(
    summary: &mut Option<String>,
    xhtml_summary: &mut Option<String>,
    target: Option<TextTarget>,
    chunk: &str,
) {
    match target {
        Some(TextTarget::Summary) => push_chunk(summary, chunk),
        Some(TextTarget::XhtmlSummary) => push_chunk(xhtml_summary, chunk),
        None => {}
    }
}

fn push_chunk(slot: &mut Option<String>, chunk: &str) {
    if chunk.is_empty() {
        return;
    }

    slot.get_or_insert_with(String::new).push_str(chunk);
}

fn find_attr(element: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in element.attributes().with_checks(false) {
        let attr = attr.map_err(|_| ParseError::new(ErrorKind::InvalidXml("invalid attribute")))?;
        if attr.key.as_ref() == key {
            let raw = std::str::from_utf8(attr.value.as_ref())
                .map_err(|_| ParseError::new(ErrorKind::InvalidUtf8))?;
            let value = unescape(raw)
                .map_err(|_| ParseError::new(ErrorKind::InvalidXml("invalid escape sequence")))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn required_attr(element: &BytesStart<'_>, key: &[u8]) -> Result<String> {
    find_attr(element, key)?.ok_or_else(|| {
        ParseError::new(ErrorKind::MissingField(match key {
            b"cccc" => "cccc",
            b"ttaaii" => "ttaaii",
            b"issue" => "issue",
            b"awipsid" => "awipsid",
            b"id" => "id",
            _ => "attribute",
        }))
    })
}

fn decode_text(text: &BytesText<'_>) -> Result<String> {
    let raw =
        std::str::from_utf8(text.as_ref()).map_err(|_| ParseError::new(ErrorKind::InvalidUtf8))?;
    let decoded = unescape(raw)
        .map_err(|_| ParseError::new(ErrorKind::InvalidXml("invalid escape sequence")))?;
    Ok(decoded.into_owned())
}

fn decode_cdata(text: &BytesCData<'_>) -> Result<String> {
    let raw =
        std::str::from_utf8(text.as_ref()).map_err(|_| ParseError::new(ErrorKind::InvalidUtf8))?;
    Ok(raw.to_owned())
}

fn trim_bulletin_edges(input: &str) -> String {
    input.trim_matches(|ch| ch == '\r' || ch == '\n').to_owned()
}

#[cfg(test)]
mod tests {
    use super::{NwwsOiId, NwwsOiMessage};

    const XML: &str = r#"<message to='enduser@server/laptop' type='groupchat' from='nwws@nwws-oi.weather.gov/nwws-oi'>
<body>KARX issues RR8 valid 2013-05-25T02:20:34Z</body>
<html xmlns='http://jabber.org/protocol/xhtml-im'><body xmlns='http://www.w3.org/1999/xhtml'>KARX issues RR8 valid 2013-05-25T02:20:34Z</body></html>
<x xmlns='nwws-oi' cccc='KARX' ttaaii='SRUS83' issue='2013-05-25T02:20:34Z' awipsid='RR8ARX' id='10313.6'>111
SRUS83 KARX 250220
RR8ARX
:
: AUTOMATED GAUGE DATA COLLECTED FROM IOWA FLOOD CENTER
:
.A CDGI4 20130524 C DH2100/HGIRP 2.63 : MORGAN CREEK NEAR CEDAR RAPIDS</x>
</message>"#;

    #[test]
    fn parses_nwws_oi_message() {
        let message = NwwsOiMessage::parse(XML).unwrap();
        assert_eq!(
            message.summary.as_deref(),
            Some("KARX issues RR8 valid 2013-05-25T02:20:34Z")
        );
        assert_eq!(
            message.xhtml_summary.as_deref(),
            Some("KARX issues RR8 valid 2013-05-25T02:20:34Z")
        );
        let payload = message.payload.unwrap();
        assert_eq!(payload.cccc, "KARX");
        assert_eq!(payload.ttaaii, "SRUS83");
        assert_eq!(payload.awips_id, "RR8ARX");
        assert_eq!(
            payload.id,
            NwwsOiId {
                process_id: 10313,
                sequence: 6
            }
        );
        payload.validate().unwrap();
    }

    #[test]
    fn supports_history_message_without_payload() {
        let xml = r#"<message type='groupchat'><body>KARX issues RR8 valid 2013-05-25T02:20:34Z</body></message>"#;
        let message = NwwsOiMessage::parse(xml).unwrap();
        assert!(message.payload.is_none());
        message.validate().unwrap();
    }

    #[test]
    fn rejects_bad_id() {
        assert!(NwwsOiId::parse("10313").is_err());
    }
}
