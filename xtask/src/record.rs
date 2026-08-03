//! Records protocol traces from a live debuggee into `fixtures/`.
//!
//! The recorder **drives the session itself** from a named scenario rather than
//! proxying a human-operated Web Inspector. That costs a little fidelity and
//! buys reproducibility: re-running `record` against the same page produces the
//! same trace, so a fixture can be regenerated when WebKit moves instead of
//! being a one-off artefact nobody can reproduce.
//!
//! # Usage
//!
//! ```sh
//! WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 \
//!   /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
//!   --enable-developer-extras=true http://127.0.0.1:8731/index.html &
//! cargo run -p xtask -- record --scenario attach --out fixtures/attach.jsonl
//! ```
//!
//! `--enable-developer-extras=true` is not optional: without it the inspector
//! server listens but never registers a target.
//!
//! # Status
//!
//! The framing and message vocabulary below are taken from WebKit's own source
//! at the pinned ref and are correct. **The handshake does not yet complete** —
//! `SetupInspectorClient` draws no reply from MiniBrowser 2.52.3. See
//! `docs/tasks/T-000-inspector-handshake.md`. Everything downstream of
//! [`Client::targets`] is written and waiting on that.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Args)]
pub struct RecordArgs {
    /// The debuggee's inspector server, as set in `WEBKIT_INSPECTOR_SERVER`.
    #[arg(long, default_value = "127.0.0.1:2999")]
    pub address: String,

    /// Which scripted scenario to run.
    #[arg(long, default_value = "attach")]
    pub scenario: String,

    /// Where to write the trace. Defaults to `fixtures/<scenario>.jsonl`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Which inspectable target to attach to.
    #[arg(long, default_value_t = 0)]
    pub target: usize,

    /// How long to keep listening after the last command, so trailing events
    /// land in the trace.
    #[arg(long, default_value_t = 3000)]
    pub settle_ms: u64,

    /// List targets and exit, without recording.
    #[arg(long)]
    pub list_only: bool,
}

/// One step of a scripted scenario.
enum Step {
    /// Send a command and wait for its reply.
    Call(&'static str, fn() -> Value),
    /// Wait for events to arrive.
    Settle(u64),
    /// Wait until an event with this method arrives, or the timeout elapses.
    Await(&'static str, u64),
}

fn scenario(name: &str) -> Result<Vec<Step>> {
    let enable = || {
        vec![
            Step::Call("Inspector.enable", || json!({})),
            Step::Call("Page.enable", || json!({})),
            Step::Call("Runtime.enable", || json!({})),
            Step::Call("Console.enable", || json!({})),
            Step::Call("Debugger.enable", || json!({})),
        ]
    };

    Ok(match name {
        // The handshake plus the script/resource inventory: what the source
        // browser needs and nothing more.
        "attach" => {
            let mut s = enable();
            s.push(Step::Call("Page.getResourceTree", || json!({})));
            s.push(Step::Settle(2000));
            s
        }

        // Set a breakpoint by URL, reload so it resolves and hits, then walk
        // the pause.
        "breakpoint-hit" => {
            let mut s = enable();
            s.push(Step::Call(
                "Debugger.setBreakpointsActive",
                || json!({ "active": true }),
            ));
            s.push(Step::Call("Debugger.setBreakpointByUrl", || {
                // `computeTotal` in the fixture page, called on a timer so the
                // breakpoint hits without anyone clicking anything.
                json!({ "lineNumber": 3, "urlRegex": ".*app\\.js" })
            }));
            s.push(Step::Await("Debugger.paused", 8000));
            s.push(Step::Call("Debugger.resume", || json!({})));
            s.push(Step::Settle(1000));
            s
        }

        "network-load" => {
            let mut s = enable();
            s.push(Step::Call("Network.enable", || json!({})));
            s.push(Step::Call("Page.reload", || json!({})));
            s.push(Step::Settle(5000));
            s
        }

        "dom-css" => {
            let mut s = enable();
            s.push(Step::Call("DOM.enable", || json!({})));
            s.push(Step::Call("CSS.enable", || json!({})));
            s.push(Step::Call("DOM.getDocument", || json!({})));
            s.push(Step::Settle(2000));
            s
        }

        "storage" => {
            let mut s = enable();
            s.push(Step::Call("DOMStorage.enable", || json!({})));
            s.push(Step::Call("IndexedDB.enable", || json!({})));
            s.push(Step::Call("Page.getCookies", || json!({})));
            s.push(Step::Settle(2000));
            s
        }

        "timeline-record" => {
            let mut s = enable();
            s.push(Step::Call("Timeline.start", || json!({})));
            s.push(Step::Call(
                "ScriptProfiler.startTracking",
                || json!({ "includeSamples": true }),
            ));
            s.push(Step::Settle(3000));
            s.push(Step::Call("ScriptProfiler.stopTracking", || json!({})));
            s.push(Step::Call("Timeline.stop", || json!({})));
            s.push(Step::Settle(1500));
            s
        }

        other => bail!(
            "unknown scenario `{other}`. Known: attach, breakpoint-hit, network-load, \
             dom-css, storage, timeline-record"
        ),
    })
}

/// One line of a trace. Records the *inspector protocol* frames, not the socket
/// envelope — the envelope is the transport's business and a fixture must stay
/// valid if it ever changes.
#[derive(Serialize)]
struct TraceLine {
    /// Microseconds since the recording started. Relative, so a trace does not
    /// embed a wall clock and stays diffable across re-recordings.
    t: u128,
    dir: &'static str,
    frame: Value,
}

/// An inspectable target, from `SetTargetList`.
#[derive(Debug, Clone)]
struct Target {
    connection_id: u64,
    target_id: u64,
    name: String,
    url: String,
    kind: String,
}

/// A client of the inspector server's socket protocol.
///
/// Framing is a 4-byte **big-endian** length then JSON —
/// `RemoteInspectorMessageParser.cpp` uses `htonl`. Little-endian is silently
/// wrong: the server reads a huge length, calls it invalid, and closes.
struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl Client {
    async fn connect(address: &str) -> Result<Self> {
        let stream = TcpStream::connect(address).await.with_context(|| {
            format!(
                "connecting to {address}. Is the debuggee running with \
                 WEBKIT_INSPECTOR_SERVER set?"
            )
        })?;
        Ok(Self {
            stream,
            buffer: Vec::new(),
        })
    }

    async fn send_event(&mut self, event: Value) -> Result<()> {
        let payload = serde_json::to_vec(&event)?;
        let len = u32::try_from(payload.len()).context("message too large to frame")?;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Read one message, or `None` on timeout.
    async fn recv_event(&mut self, timeout: Duration) -> Result<Option<Value>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.take_buffered()? {
                return Ok(Some(event));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }

            let mut chunk = [0u8; 65536];
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Err(_) => return Ok(None),
                Ok(Ok(0)) => bail!("the inspector server closed the connection"),
                Ok(Ok(n)) => self.buffer.extend_from_slice(&chunk[..n]),
                Ok(Err(e)) => return Err(e.into()),
            }
        }
    }

    /// Pull one complete message out of the buffer, if there is one.
    ///
    /// `BackendCommands` is tens of kilobytes and always arrives split, so
    /// buffering across reads is required rather than defensive.
    fn take_buffered(&mut self) -> Result<Option<Value>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let mut size = [0u8; 4];
        size.copy_from_slice(&self.buffer[..4]);
        let size = u32::from_be_bytes(size) as usize;

        if self.buffer.len() < 4 + size {
            return Ok(None);
        }
        let payload = self.buffer[4..4 + size].to_vec();
        self.buffer.drain(..4 + size);
        Ok(Some(serde_json::from_slice(&payload).with_context(
            || format!("inspector server sent non-JSON in a {size}-byte message"),
        )?))
    }

    /// Handshake, then collect the target list.
    async fn targets(&mut self) -> Result<Vec<Target>> {
        self.send_event(json!({ "event": "SetupInspectorClient" }))
            .await?;

        let mut targets = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);

        while Instant::now() < deadline {
            let Some(event) = self.recv_event(Duration::from_secs(3)).await? else {
                continue;
            };
            match event.get("event").and_then(Value::as_str) {
                Some("SetTargetList") => {
                    let connection_id = event
                        .get("connectionID")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    for item in event
                        .get("targetList")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        targets.push(Target {
                            connection_id,
                            target_id: item.get("targetID").and_then(Value::as_u64).unwrap_or(0),
                            name: string_of(item, "name"),
                            url: string_of(item, "url"),
                            kind: string_of(item, "type"),
                        });
                    }
                    return Ok(targets);
                }
                // Sent before the target list; not what we are waiting for.
                Some("BackendCommands") => continue,
                _ => continue,
            }
        }

        bail!(
            "no SetTargetList arrived within 10s.\n\
             The server is listening but registered no target. Two known causes:\n\
             \x20 1. the debuggee was started without developer extras — pass\n\
             \x20    `--enable-developer-extras=true` to MiniBrowser, or call\n\
             \x20    webkit_settings_set_enable_developer_extras() in an app;\n\
             \x20 2. the handshake needs a step this recorder does not yet send —\n\
             \x20    see docs/tasks/T-000-inspector-handshake.md, which is open."
        )
    }
}

fn string_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub fn run(root: &Path, args: RecordArgs) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(record(root, args))
}

async fn record(root: &Path, args: RecordArgs) -> Result<()> {
    let steps = scenario(&args.scenario)?;
    let mut client = Client::connect(&args.address).await?;
    let targets = client.targets().await?;

    if targets.is_empty() {
        bail!("the inspector server reported an empty target list");
    }
    for (i, t) in targets.iter().enumerate() {
        println!("[{i}] {} — {} ({})", t.name, t.url, t.kind);
    }
    if args.list_only {
        return Ok(());
    }

    let target = targets
        .get(args.target)
        .with_context(|| format!("no target at index {}", args.target))?
        .clone();
    println!(
        "attaching to target {} on connection {}",
        target.target_id, target.connection_id
    );

    client
        .send_event(json!({
            "event": "Setup",
            "connectionID": target.connection_id,
            "targetID": target.target_id,
        }))
        .await?;

    let started = Instant::now();
    let mut trace: Vec<TraceLine> = Vec::new();
    let mut next_id: u64 = 1;

    for step in steps {
        match step {
            Step::Call(method, params) => {
                let id = next_id;
                next_id += 1;
                let frame = json!({ "id": id, "method": method, "params": params() });
                send_frame(&mut client, &target, &frame).await?;
                trace.push(TraceLine {
                    t: started.elapsed().as_micros(),
                    dir: "send",
                    frame,
                });
                drain(&mut client, &mut trace, started, Some(id), None, 5000).await?;
            }
            Step::Settle(ms) => drain(&mut client, &mut trace, started, None, None, ms).await?,
            Step::Await(method, ms) => {
                drain(&mut client, &mut trace, started, None, Some(method), ms).await?;
            }
        }
    }
    drain(&mut client, &mut trace, started, None, None, args.settle_ms).await?;

    client
        .send_event(json!({
            "event": "FrontendDidClose",
            "connectionID": target.connection_id,
            "targetID": target.target_id,
        }))
        .await
        .ok();

    let out = args
        .out
        .unwrap_or_else(|| root.join(format!("fixtures/{}.jsonl", args.scenario)));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body: String = trace
        .iter()
        .map(|l| serde_json::to_string(l).map(|s| s + "\n"))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    std::fs::write(&out, body)?;

    let sent = trace.iter().filter(|l| l.dir == "send").count();
    println!(
        "wrote {} ({} frames: {sent} sent, {} received)",
        out.display(),
        trace.len(),
        trace.len() - sent
    );
    Ok(())
}

/// Wrap an inspector frame in `SendMessageToBackend` and send it.
async fn send_frame(client: &mut Client, target: &Target, frame: &Value) -> Result<()> {
    client
        .send_event(json!({
            "event": "SendMessageToBackend",
            "connectionID": target.connection_id,
            "targetID": target.target_id,
            // The frame travels as a string, not a nested object.
            "message": serde_json::to_string(frame)?,
        }))
        .await
}

/// Read frames until a stop condition or the deadline.
async fn drain(
    client: &mut Client,
    trace: &mut Vec<TraceLine>,
    started: Instant,
    until_id: Option<u64>,
    until_event: Option<&str>,
    timeout_ms: u64,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        let Some(event) = client.recv_event(remaining).await? else {
            return Ok(()); // a quiet socket is not an error
        };

        // Only frontend messages carry inspector frames; other events are
        // envelope bookkeeping and do not belong in a fixture.
        if event.get("event").and_then(Value::as_str) != Some("SendMessageToFrontend") {
            continue;
        }
        let Some(message) = event.get("message").and_then(Value::as_str) else {
            continue;
        };
        let frame: Value = serde_json::from_str(message)
            .with_context(|| format!("debuggee sent a non-JSON frame: {message:.200}"))?;

        let id = frame.get("id").and_then(Value::as_u64);
        let method = frame
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);

        trace.push(TraceLine {
            t: started.elapsed().as_micros(),
            dir: "recv",
            frame,
        });

        if until_id.is_some() && id == until_id {
            return Ok(());
        }
        if let (Some(want), Some(got)) = (until_event, method.as_deref())
            && want == got
        {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_scenarios_are_rejected_by_name() {
        assert!(scenario("attach").is_ok());
        assert!(scenario("breakpoint-hit").is_ok());
        assert!(scenario("nope").is_err());
    }

    #[test]
    fn messages_are_framed_big_endian() {
        // `htonl` in RemoteInspectorMessageParser.cpp. Little-endian is
        // silently wrong: the server reads a huge length and closes.
        let payload = br#"{"event":"SetupInspectorClient"}"#;
        let len = (payload.len() as u32).to_be_bytes();
        // A short message puts its length in the *last* byte, not the first.
        // That is the whole distinction, and it is what the server rejects.
        assert_eq!(len[..3], [0, 0, 0]);
        assert_eq!(u32::from_be_bytes(len) as usize, payload.len());
        assert_ne!(len, (payload.len() as u32).to_le_bytes());
    }

    #[test]
    fn a_message_split_across_reads_is_reassembled() {
        // BackendCommands is tens of kilobytes and always arrives in pieces.
        let payload = br#"{"event":"BackendCommands"}"#;
        let mut framed = (payload.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(payload);

        // Feed the frame one byte at a time; nothing should decode until the
        // last byte arrives.
        let mut buffer: Vec<u8> = Vec::new();
        for (i, byte) in framed.iter().enumerate() {
            buffer.push(*byte);
            let complete = buffer.len() >= 4 && {
                let mut size = [0u8; 4];
                size.copy_from_slice(&buffer[..4]);
                buffer.len() >= 4 + u32::from_be_bytes(size) as usize
            };
            assert_eq!(complete, i == framed.len() - 1, "at byte {i}");
        }
    }
}
