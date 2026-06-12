//! Supervised, always-on NWWS-OI ingest.
//!
//! [`run`] connects to NWWS-OI, feeds every payload-bearing stanza through an
//! [`IngestService`] (dedupe + archive), and reconnects forever on failure with
//! jittered exponential backoff and MUC history backfill. Dedupe persists in the
//! archive's index file, so messages replayed by history backfill after a
//! reconnect are recognized and skipped instead of archived twice.
//!
//! Backoff uses the "equal jitter" strategy from Brooker, M. (2015),
//! "Exponential Backoff And Jitter", AWS Architecture Blog.

use std::io::{self, ErrorKind as IoErrorKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use crate::ingest::IngestHint;
use crate::oi::NwwsOiMessage;
use crate::oi_client::{NwwsOiClient, OiClientConfig, OiClientError, Result as OiResult};
use crate::runtime::{ArchiveRecord, IngestService};

/// Anything that can produce live NWWS-OI messages. Implemented for
/// [`NwwsOiClient`]; tests substitute scripted fakes.
pub trait OiMessageSource {
    fn next_message(&mut self) -> OiResult<NwwsOiMessage>;

    fn jid(&self) -> Option<&str> {
        None
    }

    fn send_keepalive(&mut self) -> OiResult<()> {
        Ok(())
    }

    fn close(&mut self) {}
}

impl<S> OiMessageSource for NwwsOiClient<S>
where
    S: io::Read + io::Write,
{
    fn next_message(&mut self) -> OiResult<NwwsOiMessage> {
        NwwsOiClient::next_message(self)
    }

    fn jid(&self) -> Option<&str> {
        NwwsOiClient::jid(self)
    }

    fn send_keepalive(&mut self) -> OiResult<()> {
        NwwsOiClient::send_keepalive(self)
    }

    fn close(&mut self) {
        let _ = NwwsOiClient::close(self);
    }
}

/// Jittered exponential backoff schedule for reconnect attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub multiplier: f64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
        }
    }
}

impl BackoffPolicy {
    /// Delay before reconnect attempt `attempt` (0-based), with `jitter01`
    /// drawn uniformly from `[0, 1)`. Equal jitter: half the exponential delay
    /// is kept as a floor so retries never collapse to zero.
    pub fn delay(&self, attempt: u32, jitter01: f64) -> Duration {
        let exponent = self.multiplier.powi(attempt.min(64) as i32);
        let base = self.initial.as_secs_f64() * exponent;
        let capped = base.min(self.max.as_secs_f64());
        let jittered = capped / 2.0 + capped / 2.0 * jitter01.clamp(0.0, 1.0);
        Duration::from_secs_f64(jittered)
    }
}

#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// MUC history stanzas requested on the first connection.
    pub initial_history: u32,
    /// MUC history stanzas requested on every reconnection, so products that
    /// arrived during the outage are backfilled (dedupe drops any overlap).
    pub reconnect_history: u32,
    pub backoff: BackoffPolicy,
    /// Consecutive read timeouts tolerated (with whitespace keepalives in
    /// between) before the connection is declared dead and rebuilt.
    pub max_silent_reads: u32,
    /// Stop after this many payload messages have been processed. Bounded runs
    /// are for tests and smoke checks; daemons run unbounded.
    pub max_messages: Option<u64>,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            initial_history: 0,
            reconnect_history: 30,
            backoff: BackoffPolicy::default(),
            max_silent_reads: 10,
            max_messages: None,
        }
    }
}

/// Lifecycle and per-message notifications delivered to the observer callback.
#[derive(Debug)]
pub enum DaemonEvent<'a> {
    Connected {
        jid: Option<&'a str>,
        attempt: u32,
    },
    ConnectFailed {
        error: &'a OiClientError,
        attempt: u32,
        retry_in: Duration,
    },
    MessageProcessed {
        message: &'a NwwsOiMessage,
        records: &'a [ArchiveRecord],
        duplicate: bool,
    },
    MessageSkipped {
        reason: &'a str,
    },
    IngestFailed {
        error: String,
    },
    SilentRead {
        consecutive: u32,
    },
    Disconnected {
        error: Option<&'a OiClientError>,
        retry_in: Duration,
    },
    ShuttingDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DaemonSummary {
    pub connections: u64,
    pub connect_failures: u64,
    pub reconnects: u64,
    pub messages_read: u64,
    pub messages_processed: u64,
    pub archived_records: u64,
    pub duplicate_records: u64,
    pub ingest_failures: u64,
    pub skipped_messages: u64,
}

/// Run the supervised ingest loop against a real NWWS-OI connection until
/// `shutdown` is set (checked at least every 250 ms while sleeping and on
/// every read timeout while connected).
pub fn run(
    config: &OiClientConfig,
    service: &mut IngestService,
    options: &DaemonOptions,
    observer: impl FnMut(DaemonEvent<'_>),
    shutdown: &AtomicBool,
) -> DaemonSummary {
    run_with(
        |history| {
            let mut config = config.clone();
            config.history_stanzas = history;
            NwwsOiClient::connect(config)
        },
        service,
        options,
        observer,
        shutdown,
        |delay| {
            let mut remaining = delay;
            while remaining > Duration::ZERO {
                if shutdown.load(Ordering::Relaxed) {
                    return false;
                }
                let step = remaining.min(Duration::from_millis(250));
                std::thread::sleep(step);
                remaining = remaining.saturating_sub(step);
            }
            !shutdown.load(Ordering::Relaxed)
        },
    )
}

/// Supervision loop with injectable connector and sleeper, so reconnect,
/// backfill, and backoff behavior is testable without sockets or wall time.
///
/// `connect` receives the MUC history depth to request for that attempt.
/// `sleep` returns `false` to abort the wait (shutdown observed mid-sleep).
pub fn run_with<S, C, O, F>(
    mut connect: C,
    service: &mut IngestService,
    options: &DaemonOptions,
    mut observer: O,
    shutdown: &AtomicBool,
    mut sleep: F,
) -> DaemonSummary
where
    S: OiMessageSource,
    C: FnMut(u32) -> OiResult<S>,
    O: FnMut(DaemonEvent<'_>),
    F: FnMut(Duration) -> bool,
{
    let mut summary = DaemonSummary::default();
    let mut attempt: u32 = 0;
    let mut connected_before = false;
    let mut jitter = JitterState::seeded();

    'supervise: while !shutdown.load(Ordering::Relaxed) {
        if reached_message_limit(&summary, options) {
            break;
        }

        let history = if connected_before {
            options.reconnect_history
        } else {
            options.initial_history
        };
        let mut source = match connect(history) {
            Ok(source) => source,
            Err(error) => {
                summary.connect_failures += 1;
                let retry_in = options.backoff.delay(attempt, jitter.next01());
                observer(DaemonEvent::ConnectFailed {
                    error: &error,
                    attempt,
                    retry_in,
                });
                attempt = attempt.saturating_add(1);
                if !sleep(retry_in) {
                    break;
                }
                continue;
            }
        };

        summary.connections += 1;
        if connected_before {
            summary.reconnects += 1;
        }
        observer(DaemonEvent::Connected {
            jid: source.jid(),
            attempt,
        });
        connected_before = true;

        let mut silent_reads: u32 = 0;
        let mut stable = false;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                source.close();
                observer(DaemonEvent::ShuttingDown);
                break 'supervise;
            }
            if reached_message_limit(&summary, options) {
                source.close();
                break 'supervise;
            }

            match source.next_message() {
                Ok(message) => {
                    silent_reads = 0;
                    if !stable {
                        // The connection produced data: future failures restart
                        // the backoff schedule from the beginning.
                        stable = true;
                        attempt = 0;
                    }
                    summary.messages_read += 1;
                    process_message(&message, service, &mut summary, &mut observer);
                }
                Err(OiClientError::Io(error)) if is_read_timeout(&error) => {
                    silent_reads += 1;
                    observer(DaemonEvent::SilentRead {
                        consecutive: silent_reads,
                    });
                    if silent_reads >= options.max_silent_reads {
                        source.close();
                        let retry_in = options.backoff.delay(attempt, jitter.next01());
                        observer(DaemonEvent::Disconnected {
                            error: None,
                            retry_in,
                        });
                        attempt = attempt.saturating_add(1);
                        if !sleep(retry_in) {
                            break 'supervise;
                        }
                        continue 'supervise;
                    }
                    let _ = source.send_keepalive();
                }
                Err(error) => {
                    source.close();
                    let retry_in = options.backoff.delay(attempt, jitter.next01());
                    observer(DaemonEvent::Disconnected {
                        error: Some(&error),
                        retry_in,
                    });
                    attempt = attempt.saturating_add(1);
                    if !sleep(retry_in) {
                        break 'supervise;
                    }
                    continue 'supervise;
                }
            }
        }
    }

    summary
}

fn reached_message_limit(summary: &DaemonSummary, options: &DaemonOptions) -> bool {
    options
        .max_messages
        .is_some_and(|max| summary.messages_processed >= max)
}

fn process_message(
    message: &NwwsOiMessage,
    service: &mut IngestService,
    summary: &mut DaemonSummary,
    observer: &mut impl FnMut(DaemonEvent<'_>),
) {
    let xml = match message.to_archive_xml() {
        Ok(xml) => xml,
        Err(_) => {
            summary.skipped_messages += 1;
            observer(DaemonEvent::MessageSkipped {
                reason: "message has no NWWS-OI payload",
            });
            return;
        }
    };

    match service.process_bytes(IngestHint::OpenInterface, xml.as_bytes()) {
        Ok(report) => {
            summary.messages_processed += 1;
            // An empty record list means the dedupe store already held this
            // fingerprint and duplicate archiving is off.
            let duplicate =
                report.records.is_empty() || report.records.iter().any(|record| record.duplicate);
            if duplicate {
                summary.duplicate_records += 1;
            } else {
                summary.archived_records += report.records.len() as u64;
            }
            observer(DaemonEvent::MessageProcessed {
                message,
                records: &report.records,
                duplicate,
            });
        }
        Err(error) => {
            summary.ingest_failures += 1;
            observer(DaemonEvent::IngestFailed {
                error: error.to_string(),
            });
        }
    }
}

fn is_read_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        IoErrorKind::TimedOut | IoErrorKind::WouldBlock
    )
}

/// xorshift64* PRNG for backoff jitter; avoids a rand dependency. Seeded from
/// wall time so concurrent daemons desynchronize their retry storms.
struct JitterState(u64);

impl JitterState {
    fn seeded() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos() as u64 ^ elapsed.as_secs())
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(nanos | 1)
    }

    fn next01(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (value >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{ArchiveStore, DedupeStore, MessageRouter};
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicBool;

    const TORNADO_XML: &str = include_str!("../tests/fixtures/nwws_oi_tornado_warning.xml");

    enum Step {
        Message(Box<NwwsOiMessage>),
        Timeout,
        Fail(&'static str),
    }

    struct ScriptedSource {
        steps: VecDeque<Step>,
        keepalives: u64,
        closed: bool,
    }

    impl OiMessageSource for ScriptedSource {
        fn next_message(&mut self) -> OiResult<NwwsOiMessage> {
            match self.steps.pop_front() {
                Some(Step::Message(message)) => Ok(*message),
                Some(Step::Timeout) => Err(OiClientError::Io(io::Error::new(
                    IoErrorKind::TimedOut,
                    "read timeout",
                ))),
                Some(Step::Fail(detail)) => Err(OiClientError::Protocol(detail)),
                None => Err(OiClientError::Protocol("script exhausted")),
            }
        }

        fn send_keepalive(&mut self) -> OiResult<()> {
            self.keepalives += 1;
            Ok(())
        }

        fn close(&mut self) {
            self.closed = true;
        }
    }

    fn fixture_message() -> NwwsOiMessage {
        NwwsOiMessage::parse(TORNADO_XML.trim()).expect("fixture parses")
    }

    fn test_service(root: &std::path::Path) -> IngestService {
        let router = MessageRouter::new(Some(ArchiveStore::new(root.join("archive"))));
        let dedupe = DedupeStore::open(root.join("dedupe.txt")).expect("dedupe store");
        IngestService::new(router, dedupe)
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("nwws-daemon-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn backoff_grows_to_cap_with_floor() {
        let policy = BackoffPolicy::default();
        assert_eq!(policy.delay(0, 0.0), Duration::from_secs_f64(0.5));
        assert_eq!(policy.delay(0, 1.0), Duration::from_secs(1));
        assert_eq!(policy.delay(3, 0.0), Duration::from_secs(4));
        // Attempt 10 would be 1024s unjittered; capped at 60s.
        assert_eq!(policy.delay(10, 1.0), Duration::from_secs(60));
        assert_eq!(policy.delay(10, 0.0), Duration::from_secs(30));
    }

    #[test]
    fn reconnects_after_failure_and_backfills_history() {
        let root = temp_root("reconnect");
        let mut service = test_service(&root);
        let mut histories = Vec::new();
        let mut sessions = VecDeque::from([
            VecDeque::from([
                Step::Message(Box::new(fixture_message())),
                Step::Fail("stream reset"),
            ]),
            VecDeque::from([Step::Message(Box::new(fixture_message()))]),
        ]);
        let mut slept = Vec::new();

        let options = DaemonOptions {
            reconnect_history: 25,
            max_messages: Some(2),
            ..DaemonOptions::default()
        };
        let shutdown = AtomicBool::new(false);
        let summary = run_with(
            |history| {
                histories.push(history);
                Ok(ScriptedSource {
                    steps: sessions.pop_front().expect("scripted session"),
                    keepalives: 0,
                    closed: false,
                })
            },
            &mut service,
            &options,
            |_event| {},
            &shutdown,
            |delay| {
                slept.push(delay);
                true
            },
        );

        assert_eq!(histories, vec![0, 25], "reconnect must request backfill");
        assert_eq!(summary.connections, 2);
        assert_eq!(summary.reconnects, 1);
        assert_eq!(summary.messages_processed, 2);
        assert_eq!(
            summary.archived_records, 1,
            "second copy must dedupe, not re-archive"
        );
        assert_eq!(summary.duplicate_records, 1);
        assert_eq!(slept.len(), 1, "one backoff sleep between sessions");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keepalives_then_forced_reconnect_after_silence() {
        let root = temp_root("silence");
        let mut service = test_service(&root);
        let mut sessions = VecDeque::from([
            VecDeque::from([Step::Timeout, Step::Timeout, Step::Timeout]),
            VecDeque::from([Step::Message(Box::new(fixture_message()))]),
        ]);
        let keepalive_counts = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        let options = DaemonOptions {
            max_silent_reads: 3,
            max_messages: Some(1),
            ..DaemonOptions::default()
        };
        let shutdown = AtomicBool::new(false);
        let mut silent_events = 0;
        let summary = run_with(
            |_history| {
                Ok(CountingSource {
                    inner: ScriptedSource {
                        steps: sessions.pop_front().expect("scripted session"),
                        keepalives: 0,
                        closed: false,
                    },
                    on_close: keepalive_counts.clone(),
                })
            },
            &mut service,
            &options,
            |event| {
                if matches!(event, DaemonEvent::SilentRead { .. }) {
                    silent_events += 1;
                }
            },
            &shutdown,
            |_delay| true,
        );

        assert_eq!(summary.connections, 2);
        assert_eq!(silent_events, 3);
        assert_eq!(summary.messages_processed, 1);
        assert_eq!(
            *keepalive_counts.borrow(),
            vec![2, 0],
            "keepalive after each tolerated timeout except the one that forces reconnect"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    struct CountingSource {
        inner: ScriptedSource,
        on_close: std::rc::Rc<std::cell::RefCell<Vec<u64>>>,
    }

    impl OiMessageSource for CountingSource {
        fn next_message(&mut self) -> OiResult<NwwsOiMessage> {
            self.inner.next_message()
        }

        fn send_keepalive(&mut self) -> OiResult<()> {
            self.inner.send_keepalive()
        }

        fn close(&mut self) {
            self.inner.close();
            self.on_close.borrow_mut().push(self.inner.keepalives);
        }
    }

    #[test]
    fn shutdown_aborts_backoff_sleep() {
        let root = temp_root("shutdown");
        let mut service = test_service(&root);
        let options = DaemonOptions::default();
        let shutdown = AtomicBool::new(false);
        let mut attempts = 0;
        let summary = run_with(
            |_history| -> OiResult<ScriptedSource> {
                attempts += 1;
                Err(OiClientError::Protocol("unreachable host"))
            },
            &mut service,
            &options,
            |_event| {},
            &shutdown,
            |_delay| false,
        );

        assert_eq!(attempts, 1, "aborted sleep must stop the supervise loop");
        assert_eq!(summary.connect_failures, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn jitter_stays_in_unit_interval() {
        let mut jitter = JitterState::seeded();
        for _ in 0..10_000 {
            let value = jitter.next01();
            assert!((0.0..1.0).contains(&value));
        }
    }
}
