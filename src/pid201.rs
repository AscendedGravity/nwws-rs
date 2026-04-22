use serde::Serialize;

use crate::Result;
use crate::error::{ErrorKind, ParseError};
use crate::ingest::TransportDescriptor;
use crate::product::NwwsContent;
use crate::stream::WmoStreamScanner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pid201Record {
    pub transport: TransportDescriptor,
    pub offset: usize,
    pub leading_junk_prefix: usize,
    pub raw_message: Vec<u8>,
}

impl Pid201Record {
    pub fn content(&self) -> Result<NwwsContent<'_>> {
        NwwsContent::parse_bulletin(&self.raw_message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Pid201DrainState {
    pub discarded_junk: usize,
    pub pending_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct Pid201StreamAdapter {
    scanner: WmoStreamScanner,
    buffer: Vec<u8>,
    consumed_bytes: usize,
    discarded_junk: usize,
    max_buffer_len: usize,
}

impl Default for Pid201StreamAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Pid201StreamAdapter {
    pub fn new() -> Self {
        Self::with_max_message_len(1 << 20)
    }

    pub fn with_max_message_len(max_message_len: usize) -> Self {
        Self {
            scanner: WmoStreamScanner::with_max_message_len(max_message_len),
            buffer: Vec::new(),
            consumed_bytes: 0,
            discarded_junk: 0,
            max_buffer_len: max_message_len,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Pid201Record>> {
        self.buffer.extend_from_slice(chunk);
        let mut records = Vec::new();

        loop {
            let outcome = self.scanner.scan_next(&self.buffer)?;
            let Some(chunk) = outcome.chunk else {
                if outcome.junk_prefix > 0 {
                    self.discarded_junk += outcome.junk_prefix;
                    self.consumed_bytes += outcome.junk_prefix;
                    self.buffer.drain(..outcome.junk_prefix);
                }
                self.ensure_buffer_limit()?;
                break;
            };

            let offset = self.consumed_bytes + chunk.range.start;
            let raw_message = self.buffer[chunk.range.start..chunk.range.end].to_vec();
            records.push(Pid201Record {
                transport: TransportDescriptor::satellite_pid201(),
                offset,
                leading_junk_prefix: chunk.range.start,
                raw_message,
            });

            self.discarded_junk += chunk.range.start;
            self.consumed_bytes += chunk.range.end;
            self.buffer.drain(..chunk.range.end);
        }

        Ok(records)
    }

    pub fn pending(&self) -> &[u8] {
        &self.buffer
    }

    pub fn finish(&mut self) -> Pid201DrainState {
        let pending_bytes = self.buffer.len();
        let state = Pid201DrainState {
            discarded_junk: self.discarded_junk,
            pending_bytes,
        };
        self.consumed_bytes += pending_bytes;
        self.buffer.clear();
        state
    }

    fn ensure_buffer_limit(&self) -> Result<()> {
        if self.buffer.len() > self.max_buffer_len {
            return Err(ParseError::new(ErrorKind::Oversized(
                "pid201 receiver buffer",
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Pid201StreamAdapter;

    fn frame(bulletin: &str) -> Vec<u8> {
        let bulletin = bulletin.lines().collect::<Vec<_>>().join("\r\r\n");
        format!("\u{1}\r\r\n{bulletin}\r\r\n\u{3}").into_bytes()
    }

    #[test]
    fn emits_message_after_chunked_pushes() {
        let mut adapter = Pid201StreamAdapter::new();
        let framed = frame(include_str!("../tests/fixtures/wmo_tornado_warning.txt"));
        let split = framed.len() / 2;

        assert!(adapter.push(&framed[..split]).unwrap().is_empty());
        let records = adapter.push(&framed[split..]).unwrap();

        assert_eq!(records.len(), 1);
        let content = records[0].content().unwrap();
        assert_eq!(content.bulletin.heading.ttaaii(), "WUUS53");
        assert_eq!(content.bulletin.heading.cccc(), "KLOT");
        assert_eq!(content.bulletin.awips_id.unwrap().raw(), "TORLOT");
        assert!(adapter.pending().is_empty());
    }

    #[test]
    fn discards_junk_before_message() {
        let mut adapter = Pid201StreamAdapter::new();
        let mut input = b"noise".to_vec();
        input.extend_from_slice(&frame(include_str!("../tests/fixtures/wmo_bulletin.txt")));

        let records = adapter.push(&input).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].leading_junk_prefix, 5);

        let state = adapter.finish();
        assert_eq!(state.discarded_junk, 5);
        assert_eq!(state.pending_bytes, 0);
    }

    #[test]
    fn preserves_pending_partial_message() {
        let mut adapter = Pid201StreamAdapter::new();
        let framed = frame(include_str!("../tests/fixtures/wmo_bulletin.txt"));
        let partial = &framed[..framed.len() - 1];

        assert!(adapter.push(partial).unwrap().is_empty());
        assert_eq!(adapter.pending(), partial);

        let state = adapter.finish();
        assert_eq!(state.pending_bytes, partial.len());
    }

    #[test]
    fn extracts_multiple_messages_from_single_push() {
        let mut adapter = Pid201StreamAdapter::new();
        let mut input = frame(include_str!("../tests/fixtures/wmo_tornado_warning.txt"));
        input.extend_from_slice(b"junk");
        input.extend_from_slice(&frame(include_str!(
            "../tests/fixtures/wmo_segmented_svs.txt"
        )));

        let records = adapter.push(&input).unwrap();
        assert_eq!(records.len(), 2);

        let first = records[0].content().unwrap();
        let second = records[1].content().unwrap();
        assert_eq!(first.bulletin.heading.ttaaii(), "WUUS53");
        assert_eq!(second.bulletin.heading.ttaaii(), "WWUS73");
    }

    #[test]
    fn rejects_oversized_pending_partial() {
        let mut adapter = Pid201StreamAdapter::with_max_message_len(16);
        let partial = &frame(include_str!("../tests/fixtures/wmo_bulletin.txt"))[..17];

        let error = adapter.push(partial).unwrap_err();
        assert_eq!(
            error.kind,
            crate::ErrorKind::Oversized("pid201 receiver buffer")
        );
    }
}
