//! Replaying a recorded protocol trace as if it were a live debuggee.
//!
//! This is what makes the parallel task plan work. Every task ships with a
//! fixture-backed test, and a fixture-backed test needs no browser, no port, no
//! platform: `cargo test` on a laptop with no WebKit installed exercises the
//! same code path a real attach does.
//!
//! # Trace format
//!
//! One JSON object per line, as written by `cargo run -p xtask -- record`:
//!
//! ```text
//! {"t":1043,"dir":"send","frame":{"id":1,"method":"Debugger.enable","params":{}}}
//! {"t":2871,"dir":"recv","frame":{"id":1,"result":{}}}
//! {"t":3122,"dir":"recv","frame":{"method":"Debugger.scriptParsed","params":{…}}}
//! ```
//!
//! # A trace is a contract, not a stub
//!
//! Sends are matched by method, searching forward, so a caller may enable
//! domains in a different order than the recorder did. But a send that matches
//! *nothing* in the remainder of the trace is an error, not a silent no-op.
//! That is what stops a fixture test from passing while the code under test
//! quietly asks for something the recording never covered.
//!
//! # Request ids are rewritten
//!
//! The recorded session allocated its own ids, and so does the session under
//! test; they will not agree. Replay maps each recorded id onto the id the
//! caller actually used, so correlation works exactly as it does live. Without
//! this every replayed response would be dropped as unsolicited.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use async_trait::async_trait;
use mjx_wk_dialect::DialectKind;
use serde::Deserialize;
use serde_json::Value;

use crate::{Transport, TransportError};

/// One line of a recorded trace.
#[derive(Debug, Clone, Deserialize)]
struct TraceEntry {
    /// Microseconds since the recording began. Kept for tooling; replay does
    /// not sleep, because a test should not take as long as the session did.
    #[allow(dead_code)]
    #[serde(default)]
    t: u128,
    dir: Direction,
    frame: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    Send,
    Recv,
}

/// A [`Transport`] backed by a recorded trace.
#[derive(Debug)]
pub struct ReplayTransport {
    /// Trace entries not yet reached.
    remaining: VecDeque<TraceEntry>,
    /// Frames ready to hand to [`Transport::recv`].
    pending: VecDeque<String>,
    /// Recorded request id → the id the caller actually used.
    id_map: HashMap<u64, u64>,
    dialect: DialectKind,
    closed: bool,
    /// Where this came from, for error messages.
    source: String,
}

impl ReplayTransport {
    /// Load a trace from a `.jsonl` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Self::from_str(&text, path.display().to_string())
    }

    /// Load a trace from text already in memory.
    pub fn from_str(text: &str, source: impl Into<String>) -> Result<Self, TransportError> {
        let source = source.into();
        let mut remaining = VecDeque::new();

        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: TraceEntry = serde_json::from_str(line)
                .map_err(|e| TransportError::Malformed(format!("{source}:{}: {e}", n + 1)))?;
            remaining.push_back(entry);
        }

        Ok(Self {
            remaining,
            pending: VecDeque::new(),
            id_map: HashMap::new(),
            dialect: DialectKind::WebKitRwi,
            closed: false,
            source,
        })
    }

    /// Replay a trace recorded from a Chromium debuggee.
    pub fn with_dialect(mut self, dialect: DialectKind) -> Self {
        self.dialect = dialect;
        self
    }

    /// Whether every recorded frame has been consumed.
    ///
    /// A test that attaches and then asserts this has shown it exercised the
    /// whole recording rather than the first few frames of it.
    pub fn is_exhausted(&self) -> bool {
        self.remaining.is_empty() && self.pending.is_empty()
    }

    /// Methods still expected, for a helpful failure message.
    fn upcoming_sends(&self) -> Vec<String> {
        self.remaining
            .iter()
            .filter(|e| e.dir == Direction::Send)
            .filter_map(|e| e.frame.get("method").and_then(Value::as_str))
            .map(str::to_owned)
            .collect()
    }

    /// Move leading received frames onto the pending queue.
    fn queue_leading_receives(&mut self) {
        while let Some(entry) = self.remaining.front() {
            if entry.dir != Direction::Recv {
                break;
            }
            // `front` was just checked, so this cannot fail.
            let Some(entry) = self.remaining.pop_front() else {
                break;
            };
            let frame = self.rewrite_id(entry.frame);
            if let Ok(text) = serde_json::to_string(&frame) {
                self.pending.push_back(text);
            }
        }
    }

    /// Translate a recorded request id into the caller's.
    fn rewrite_id(&self, mut frame: Value) -> Value {
        let recorded = frame.get("id").and_then(Value::as_u64);
        if let Some(recorded) = recorded
            && let Some(&ours) = self.id_map.get(&recorded)
            && let Some(slot) = frame.get_mut("id")
        {
            *slot = Value::from(ours);
        }
        frame
    }
}

#[async_trait]
impl Transport for ReplayTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost("replay is closed".into()));
        }

        let sent: Value = serde_json::from_str(&text)
            .map_err(|e| TransportError::Malformed(format!("outgoing frame: {e}")))?;
        let method = sent
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| TransportError::Malformed("outgoing frame has no method".into()))?
            .to_owned();
        let our_id = sent.get("id").and_then(Value::as_u64);

        // Search forward for the matching send, queueing anything received
        // along the way — the caller may legitimately order its commands
        // differently from the recording.
        let matched = self
            .remaining
            .iter()
            .position(|e| {
                e.dir == Direction::Send
                    && e.frame.get("method").and_then(Value::as_str) == Some(method.as_str())
            })
            .ok_or_else(|| {
                TransportError::Malformed(format!(
                    "`{method}` is not in {}.\n\
                     A replayed send must appear in the trace; a fixture that does not \
                     cover it would let this test pass without exercising anything.\n\
                     Still expected: {:?}",
                    self.source,
                    self.upcoming_sends()
                ))
            })?;

        // Everything before the match is either a received frame (queue it) or
        // a send the caller skipped (drop it — the trace is not a script the
        // caller must follow exactly, only a superset of what it may ask for).
        for _ in 0..matched {
            if let Some(entry) = self.remaining.pop_front()
                && entry.dir == Direction::Recv
            {
                let frame = self.rewrite_id(entry.frame);
                if let Ok(text) = serde_json::to_string(&frame) {
                    self.pending.push_back(text);
                }
            }
        }

        if let Some(entry) = self.remaining.pop_front()
            && let (Some(recorded), Some(ours)) =
                (entry.frame.get("id").and_then(Value::as_u64), our_id)
        {
            self.id_map.insert(recorded, ours);
        }

        self.queue_leading_receives();
        Ok(())
    }

    async fn recv(&mut self) -> Option<Result<String, TransportError>> {
        if let Some(text) = self.pending.pop_front() {
            return Some(Ok(text));
        }
        self.queue_leading_receives();
        // `None` once the trace is exhausted: the debuggee "closed the
        // connection", which is an ordinary end of session, not a failure.
        self.pending.pop_front().map(Ok)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        Ok(())
    }

    fn dialect(&self) -> DialectKind {
        self.dialect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = r#"
{"t":1,"dir":"send","frame":{"id":10,"method":"Debugger.enable","params":{}}}
{"t":2,"dir":"recv","frame":{"id":10,"result":{}}}
{"t":3,"dir":"recv","frame":{"method":"Debugger.scriptParsed","params":{"scriptId":"1"}}}
{"t":4,"dir":"send","frame":{"id":11,"method":"Page.getResourceTree","params":{}}}
{"t":5,"dir":"recv","frame":{"id":11,"result":{"frameTree":{}}}}
"#;

    fn replay() -> ReplayTransport {
        ReplayTransport::from_str(TRACE, "test-trace").unwrap()
    }

    #[tokio::test]
    async fn a_reply_is_renumbered_to_the_callers_request_id() {
        // The recording used id 10; this caller uses 99. Without rewriting,
        // the session would drop the reply as unsolicited.
        let mut t = replay();
        t.send(r#"{"id":99,"method":"Debugger.enable","params":{}}"#.into())
            .await
            .unwrap();

        let reply: Value = serde_json::from_str(&t.recv().await.unwrap().unwrap()).unwrap();
        assert_eq!(reply["id"], 99);
    }

    #[tokio::test]
    async fn events_following_a_reply_are_delivered_in_order() {
        let mut t = replay();
        t.send(r#"{"id":1,"method":"Debugger.enable","params":{}}"#.into())
            .await
            .unwrap();

        let first: Value = serde_json::from_str(&t.recv().await.unwrap().unwrap()).unwrap();
        assert_eq!(first["id"], 1);
        let second: Value = serde_json::from_str(&t.recv().await.unwrap().unwrap()).unwrap();
        assert_eq!(second["method"], "Debugger.scriptParsed");
    }

    #[tokio::test]
    async fn commands_may_be_sent_in_a_different_order_than_recorded() {
        // A caller that enables domains in its own order must still work; the
        // trace is a superset of what may be asked, not a script.
        let mut t = replay();
        t.send(r#"{"id":1,"method":"Page.getResourceTree","params":{}}"#.into())
            .await
            .unwrap();

        // The scriptParsed event passed over on the way is still delivered.
        let mut methods = Vec::new();
        while let Some(Ok(text)) = t.recv().await {
            let v: Value = serde_json::from_str(&text).unwrap();
            methods.push(
                v.get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("<reply>")
                    .to_owned(),
            );
        }
        assert!(methods.contains(&"Debugger.scriptParsed".to_string()));
        assert!(methods.contains(&"<reply>".to_string()));
    }

    #[tokio::test]
    async fn a_send_the_trace_does_not_cover_is_an_error() {
        // The point of the whole harness: a fixture that does not exercise
        // what the test claims must fail loudly.
        let mut t = replay();
        let err = t
            .send(r#"{"id":1,"method":"Network.enable","params":{}}"#.into())
            .await
            .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("Network.enable"), "{message}");
        assert!(message.contains("Debugger.enable"), "{message}");
    }

    #[tokio::test]
    async fn an_exhausted_trace_reads_as_a_clean_close() {
        let mut t = replay();
        t.send(r#"{"id":1,"method":"Page.getResourceTree","params":{}}"#.into())
            .await
            .unwrap();
        while t.recv().await.is_some() {}
        assert!(t.recv().await.is_none());
        assert!(t.is_exhausted());
    }

    #[test]
    fn a_malformed_line_names_its_line_number() {
        // A hand-edited fixture is the common case, so the error has to point
        // at the line rather than just saying the file is bad. The first line
        // is deliberately valid, so this proves the count, not just the report.
        let text = "{\"dir\":\"send\",\"frame\":{\"method\":\"X.y\"}}\nnot json\n";
        let err = ReplayTransport::from_str(text, "f.jsonl").unwrap_err();
        assert!(err.to_string().contains("f.jsonl:2"), "{err}");
    }

    #[test]
    fn blank_lines_are_ignored() {
        assert!(ReplayTransport::from_str("\n\n  \n", "empty").is_ok());
    }
}
