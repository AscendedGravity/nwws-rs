use memchr::memchr;

use crate::error::{ErrorKind, ParseError, Result};
use crate::header::{AwipsId, WmoHeading, looks_like_awips_id};
use crate::{CR, ETX, LF, SOH, WMO_SEPARATOR};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmoFrameKind {
    Framed,
    Bare,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmoMessage<'a> {
    pub frame_kind: WmoFrameKind,
    pub raw_bytes: &'a [u8],
    pub bulletin: &'a str,
    pub sequence_number: Option<u16>,
    pub heading: WmoHeading<'a>,
    pub awips_id: Option<AwipsId<'a>>,
    pub body: &'a str,
}

impl<'a> WmoMessage<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self> {
        let text =
            std::str::from_utf8(input).map_err(|_| ParseError::new(ErrorKind::InvalidUtf8))?;

        if input.first() == Some(&SOH) {
            parse_framed(input, text)
        } else {
            parse_bare(input, text)
        }
    }

    pub fn parse_str(input: &'a str) -> Result<Self> {
        Self::parse(input.as_bytes())
    }

    pub fn verify_metadata(&self, ttaaii: &str, cccc: &str, awips_id: Option<&str>) -> Result<()> {
        if self.heading.ttaaii() != ttaaii {
            return Err(ParseError::new(ErrorKind::Mismatch("ttaaii")));
        }

        if self.heading.cccc() != cccc {
            return Err(ParseError::new(ErrorKind::Mismatch("cccc")));
        }

        if let Some(expected) = awips_id {
            let actual = self
                .awips_id
                .as_ref()
                .map(|value| value.raw())
                .ok_or_else(|| ParseError::new(ErrorKind::Mismatch("awips id")))?;
            if actual != expected {
                return Err(ParseError::new(ErrorKind::Mismatch("awips id")));
            }
        }

        Ok(())
    }
}

fn parse_framed<'a>(input: &'a [u8], text: &'a str) -> Result<WmoMessage<'a>> {
    if input.len() < 5 {
        return Err(ParseError::new(ErrorKind::UnexpectedEof("framed message")));
    }

    if input[1..4] != *WMO_SEPARATOR {
        return Err(ParseError::at(
            ErrorKind::InvalidControl("leading SOH separator"),
            1,
        ));
    }

    if input.last() != Some(&ETX) {
        return Err(ParseError::at(
            ErrorKind::InvalidControl("missing trailing ETX"),
            input.len().saturating_sub(1),
        ));
    }

    let bulletin = strip_one_line_break(&text[4..text.len() - 1]);
    parse_bulletin(input, bulletin, WmoFrameKind::Framed)
}

fn parse_bare<'a>(input: &'a [u8], text: &'a str) -> Result<WmoMessage<'a>> {
    let bulletin = strip_one_line_break(text);
    parse_bulletin(input, bulletin, WmoFrameKind::Bare)
}

fn parse_bulletin<'a>(
    raw_bytes: &'a [u8],
    bulletin: &'a str,
    frame_kind: WmoFrameKind,
) -> Result<WmoMessage<'a>> {
    let (sequence_number, after_sequence) = match split_line(bulletin) {
        Some((line, rest)) if is_sequence_number(line) => (Some(parse_sequence(line)), rest),
        _ => (None, bulletin),
    };

    let (heading_line, after_heading) = split_line(after_sequence)
        .ok_or_else(|| ParseError::new(ErrorKind::UnexpectedEof("wmo heading line")))?;
    let heading = WmoHeading::parse(heading_line)?;

    let (awips_id, body) = match split_line(after_heading) {
        Some((candidate, rest)) if looks_like_awips_id(candidate) => {
            (Some(AwipsId::parse(candidate)?), rest)
        }
        _ => (None, after_heading),
    };

    Ok(WmoMessage {
        frame_kind,
        raw_bytes,
        bulletin,
        sequence_number,
        heading,
        awips_id,
        body: strip_one_line_break(body),
    })
}

fn split_line(input: &str) -> Option<(&str, &str)> {
    let bytes = input.as_bytes();
    let lf_index = memchr(LF, bytes)?;
    if lf_index >= 2 && bytes[lf_index - 2] == CR && bytes[lf_index - 1] == CR {
        Some((&input[..lf_index - 2], &input[lf_index + 1..]))
    } else if lf_index >= 1 && bytes[lf_index - 1] == CR {
        Some((&input[..lf_index - 1], &input[lf_index + 1..]))
    } else {
        Some((&input[..lf_index], &input[lf_index + 1..]))
    }
}

fn trim_ascii_whitespace(input: &str) -> &str {
    input.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn strip_one_line_break(input: &str) -> &str {
    let bytes = input.as_bytes();
    if bytes.ends_with(WMO_SEPARATOR) {
        &input[..input.len() - WMO_SEPARATOR.len()]
    } else if bytes.ends_with(b"\r\n") {
        &input[..input.len() - 2]
    } else if bytes.ends_with(b"\n") {
        &input[..input.len() - 1]
    } else {
        input
    }
}

fn is_sequence_number(line: &str) -> bool {
    let trimmed = trim_ascii_whitespace(line);
    trimmed.len() == 3 && trimmed.as_bytes().iter().all(|byte| byte.is_ascii_digit())
}

fn parse_sequence(line: &str) -> u16 {
    trim_ascii_whitespace(line)
        .bytes()
        .fold(0u16, |value, digit| (value * 10) + u16::from(digit - b'0'))
}

#[cfg(test)]
mod tests {
    use super::{WmoFrameKind, WmoMessage};

    const FRAMED: &str = "\u{1}\r\r\n111\r\r\nSRUS83 KARX 250220\r\r\nRR8ARX\r\r\n: RAW PRODUCT\r\r\n.A CDGI4 20130524 C DH2100/HGIRP 2.63\r\r\n\u{3}";

    #[test]
    fn parses_framed_message() {
        let message = WmoMessage::parse_str(FRAMED).unwrap();
        assert_eq!(message.frame_kind, WmoFrameKind::Framed);
        assert_eq!(message.sequence_number, Some(111));
        assert_eq!(message.heading.ttaaii(), "SRUS83");
        assert_eq!(message.heading.cccc(), "KARX");
        assert_eq!(message.awips_id.unwrap().raw(), "RR8ARX");
        assert!(message.body.contains(".A CDGI4"));
    }

    #[test]
    fn parses_bare_bulletin() {
        let input = "111\nNOUS41 KWBC 201530 AAA\nPNSXXX\nHeadline\nBody line";
        let message = WmoMessage::parse_str(input).unwrap();
        assert_eq!(message.frame_kind, WmoFrameKind::Bare);
        assert_eq!(message.sequence_number, Some(111));
        assert_eq!(message.heading.bbb(), Some("AAA"));
        assert_eq!(message.awips_id.unwrap().raw(), "PNSXXX");
        assert_eq!(message.body, "Headline\nBody line");
    }

    #[test]
    fn parses_sequence_number_with_trailing_spaces() {
        let input = "701 \nWUUS55 KPSR 090029\nSVRPSR\nBody line";
        let message = WmoMessage::parse_str(input).unwrap();
        assert_eq!(message.sequence_number, Some(701));
        assert_eq!(message.heading.ttaaii(), "WUUS55");
        assert_eq!(message.heading.cccc(), "KPSR");
        assert_eq!(message.awips_id.unwrap().raw(), "SVRPSR");
    }

    #[test]
    fn detects_bad_framing() {
        let broken = "\u{1}\r\n111\r\r\nSRUS83 KARX 250220\r\r\nRR8ARX\r\r\nbody\r\r\n\u{3}";
        assert!(WmoMessage::parse_str(broken).is_err());
    }

    #[test]
    fn validates_payload_metadata() {
        let message = WmoMessage::parse_str(FRAMED).unwrap();
        message
            .verify_metadata("SRUS83", "KARX", Some("RR8ARX"))
            .unwrap();
        assert!(
            message
                .verify_metadata("SRUS84", "KARX", Some("RR8ARX"))
                .is_err()
        );
    }
}
