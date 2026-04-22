use core::ops::Range;

use memchr::memchr;

use crate::error::{ErrorKind, ParseError, Result};
use crate::wmo::WmoMessage;
use crate::{ETX, SOH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedChunk<'a> {
    pub range: Range<usize>,
    pub bytes: &'a [u8],
}

impl<'a> FramedChunk<'a> {
    pub fn parse(&self) -> Result<WmoMessage<'a>> {
        WmoMessage::parse(self.bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOutcome<'a> {
    pub junk_prefix: usize,
    pub chunk: Option<FramedChunk<'a>>,
    pub pending: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmoStreamScanner {
    max_message_len: usize,
}

impl Default for WmoStreamScanner {
    fn default() -> Self {
        Self {
            max_message_len: 1 << 20,
        }
    }
}

impl WmoStreamScanner {
    pub const fn new() -> Self {
        Self {
            max_message_len: 1 << 20,
        }
    }

    pub const fn with_max_message_len(max_message_len: usize) -> Self {
        Self { max_message_len }
    }

    pub fn scan_next<'a>(&self, input: &'a [u8]) -> Result<ScanOutcome<'a>> {
        let Some(start) = memchr(SOH, input) else {
            return Ok(ScanOutcome {
                junk_prefix: input.len(),
                chunk: None,
                pending: &[],
            });
        };

        let search = &input[start + 1..];
        let Some(end_rel) = memchr(ETX, search) else {
            return Ok(ScanOutcome {
                junk_prefix: start,
                chunk: None,
                pending: &input[start..],
            });
        };

        let end = start + 1 + end_rel + 1;
        if end - start > self.max_message_len {
            return Err(ParseError::new(ErrorKind::Oversized("framed WMO message")));
        }

        Ok(ScanOutcome {
            junk_prefix: start,
            chunk: Some(FramedChunk {
                range: start..end,
                bytes: &input[start..end],
            }),
            pending: &input[end..],
        })
    }

    pub fn iter<'a>(&'a self, input: &'a [u8]) -> FramedMessageIter<'a> {
        FramedMessageIter {
            scanner: self,
            remaining: input,
            offset: 0,
            finished: false,
        }
    }
}

pub struct FramedMessageIter<'a> {
    scanner: &'a WmoStreamScanner,
    remaining: &'a [u8],
    offset: usize,
    finished: bool,
}

impl<'a> FramedMessageIter<'a> {
    pub fn pending(&self) -> &'a [u8] {
        self.remaining
    }
}

impl<'a> Iterator for FramedMessageIter<'a> {
    type Item = Result<FramedChunk<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let base_offset = self.offset;
        let outcome = match self.scanner.scan_next(self.remaining) {
            Ok(outcome) => outcome,
            Err(err) => {
                self.finished = true;
                return Some(Err(err));
            }
        };

        let Some(mut chunk) = outcome.chunk else {
            self.offset = base_offset + outcome.junk_prefix;
            self.remaining = outcome.pending;
            self.finished = true;
            return None;
        };

        chunk.range = (base_offset + chunk.range.start)..(base_offset + chunk.range.end);

        self.offset = chunk.range.end;
        self.remaining = outcome.pending;
        Some(Ok(chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::WmoStreamScanner;

    const MESSAGE: &[u8] = b"\x01\r\r\n111\r\r\nNOUS41 KWBC 201530\r\r\nPNSXXX\r\r\nbody\r\r\n\x03";

    #[test]
    fn scans_single_message() {
        let scanner = WmoStreamScanner::new();
        let outcome = scanner.scan_next(MESSAGE).unwrap();
        assert_eq!(outcome.junk_prefix, 0);
        let chunk = outcome.chunk.unwrap();
        assert_eq!(chunk.bytes, MESSAGE);
        assert!(outcome.pending.is_empty());
    }

    #[test]
    fn skips_junk_and_yields_message() {
        let mut bytes = b"junk".to_vec();
        bytes.extend_from_slice(MESSAGE);
        let scanner = WmoStreamScanner::new();
        let outcome = scanner.scan_next(&bytes).unwrap();
        assert_eq!(outcome.junk_prefix, 4);
        assert_eq!(outcome.chunk.unwrap().bytes, MESSAGE);
    }

    #[test]
    fn leaves_partial_message_pending() {
        let scanner = WmoStreamScanner::new();
        let partial = &MESSAGE[..MESSAGE.len() - 1];
        let outcome = scanner.scan_next(partial).unwrap();
        assert_eq!(outcome.junk_prefix, 0);
        assert!(outcome.chunk.is_none());
        assert_eq!(outcome.pending, partial);
    }

    #[test]
    fn iterates_multiple_messages() {
        let mut input = Vec::new();
        input.extend_from_slice(MESSAGE);
        input.extend_from_slice(b"junk");
        input.extend_from_slice(MESSAGE);

        let scanner = WmoStreamScanner::new();
        let mut iter = scanner.iter(&input);
        let first = iter.next().unwrap().unwrap();
        let second = iter.next().unwrap().unwrap();
        assert_eq!(first.bytes, MESSAGE);
        assert_eq!(second.bytes, MESSAGE);
        assert!(iter.next().is_none());
    }
}
