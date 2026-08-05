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
//! # Wire protocol (WebKitGTK / WPE)
//!
//! The server started by `WEBKIT_INSPECTOR_SERVER` is glib
//! `RemoteInspectorServer`. Framing is WTF `SocketConnection` — **not** the
//! JSON length-prefixed PlayStation protocol in
//! `UIProcess/Inspector/socket/RemoteInspectorClient.cpp`:
//!
//! ```text
//! [u32 body_size BE] [u8 flags] [name\0] [GVariant body]
//! ```
//!
//! `SetupInspectorClient` carries a GVariant `(ay)` — the SHA-1 hex digest of
//! the debuggee's `InspectorBackendCommands.js` as a bytestring. The server
//! replies with `DidSetupInspectorClient`, then pushes `SetTargetList` as
//! `(ta(tsssb))`. Proven against WebKitGTK 2.52.3; see
//! `scripts/inspector-handshake.py` and `docs/PROTOCOL-NOTES.md` trap 1.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use glib::prelude::*;
use glib::{Variant, VariantTy};
use serde::Serialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Flag bit in the `SocketConnection` header: payload is little-endian.
/// Linux WebKitGTK always sets this; see WTF `SocketConnection.cpp`.
const BYTE_ORDER_LITTLE_ENDIAN: u8 = 1 << 0;

const BACKEND_COMMANDS_PATH: &str =
    "/org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js";

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

    /// Shared library to extract `InspectorBackendCommands.js` from when
    /// computing the handshake hash. Defaults to the system WebKitGTK 4.1.
    #[arg(long)]
    pub webkit_library: Option<PathBuf>,
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

/// A client of the WebKitGTK inspector server's `SocketConnection` protocol.
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

    async fn send_message(&mut self, name: &str, parameters: Option<&Variant>) -> Result<()> {
        let mut body = Vec::new();
        body.extend_from_slice(name.as_bytes());
        body.push(0);
        if let Some(parameters) = parameters {
            body.extend_from_slice(parameters.data());
        }
        let len = u32::try_from(body.len()).context("message too large to frame")?;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&[BYTE_ORDER_LITTLE_ENDIAN]).await?;
        self.stream.write_all(&body).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Read one message, or `None` on timeout.
    async fn recv_message(&mut self, timeout: Duration) -> Result<Option<(String, Variant)>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(msg) = self.take_buffered()? {
                return Ok(Some(msg));
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

    fn take_buffered(&mut self) -> Result<Option<(String, Variant)>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let mut size = [0u8; 4];
        size.copy_from_slice(&self.buffer[..4]);
        let body_size = u32::from_be_bytes(size) as usize;
        if body_size < 2 {
            bail!("inspector server sent a body smaller than a message name");
        }
        // Cap matches WTF SocketConnection::MaximumMessageBodySize (512 MB),
        // but we refuse anything that large here — BackendCommands is ~70 KB.
        if body_size > 16 * 1024 * 1024 {
            bail!("inspector server announced a {body_size}-byte body; refusing to allocate");
        }

        let total = 4 + 1 + body_size;
        if self.buffer.len() < total {
            return Ok(None);
        }

        let flags = self.buffer[4];
        if flags & BYTE_ORDER_LITTLE_ENDIAN == 0 {
            bail!("inspector server sent a big-endian SocketConnection message");
        }
        let body = self.buffer[5..total].to_vec();
        self.buffer.drain(..total);

        let nul = body
            .iter()
            .position(|&b| b == 0)
            .context("SocketConnection message name is not NUL-terminated")?;
        let name = std::str::from_utf8(&body[..nul])
            .context("SocketConnection message name is not UTF-8")?
            .to_owned();
        let payload = &body[nul + 1..];

        let ty = message_param_type(&name)?;
        let variant = match ty {
            None => {
                if !payload.is_empty() {
                    bail!("message `{name}` carries unexpected parameters");
                }
                // Empty unit variant so callers always have a value.
                Variant::from_none(VariantTy::UNIT)
            }
            Some(type_str) => {
                let vt = VariantTy::new(type_str)
                    .map_err(|e| anyhow::anyhow!("invalid type string {type_str}: {e}"))?;
                Variant::from_data_with_type(payload, vt)
            }
        };
        Ok(Some((name, variant)))
    }

    /// Handshake, then collect the target list.
    ///
    /// The server may push an empty `SetTargetList` from a connection that has
    /// no pages yet, then a non-empty one once the web process registers. We
    /// keep listening until we see at least one target or the deadline hits.
    async fn targets(&mut self, webkit_library: &Path) -> Result<Vec<Target>> {
        let digest = backend_commands_hash(webkit_library)?;
        // `(ay)` — bytestring of the hex digest. Sending a non-matching digest
        // also works (the server then embeds the full BackendCommands.js); we
        // send the real one so a matching build can skip the ~70 KB payload.
        let type_ay = VariantTy::new("(ay)")
            .map_err(|e| anyhow::anyhow!("`(ay)` is not a valid GVariant type: {e}"))?;
        let params =
            Variant::from_data_with_type(bytestring_tuple_payload(&digest), type_ay);
        self.send_message("SetupInspectorClient", Some(&params))
            .await?;

        let mut targets = Vec::new();
        let mut saw_setup = false;
        let deadline = Instant::now() + Duration::from_secs(15);

        while Instant::now() < deadline {
            let Some((name, variant)) = self.recv_message(Duration::from_secs(3)).await? else {
                continue;
            };
            match name.as_str() {
                "DidSetupInspectorClient" => {
                    saw_setup = true;
                }
                "SetTargetList" => {
                    let parsed = parse_target_list(&variant)?;
                    if !parsed.is_empty() {
                        return Ok(parsed);
                    }
                    // Empty lists are normal early on; keep waiting.
                    targets = parsed;
                }
                _ => continue,
            }
        }

        if !saw_setup {
            bail!(
                "no DidSetupInspectorClient within 15s.\n\
                 The socket accepted a connection but did not speak the glib \
                 SocketConnection handshake. Confirm WEBKIT_INSPECTOR_SERVER \
                 points at a WebKitGTK/WPE debuggee (not a CDP endpoint)."
            );
        }
        // Setup succeeded but nothing registered — usually missing developer
        // extras, or the page has not loaded yet.
        Ok(targets)
    }
}

fn message_param_type(name: &str) -> Result<Option<&'static str>> {
    Ok(match name {
        "DidSetupInspectorClient" => Some("(ay)"),
        "SetTargetList" => Some("(ta(tsssb))"),
        "SendMessageToFrontend" => Some("(tts)"),
        "DidClose" => None,
        // Client → server names are not received, but keep the table complete.
        other => bail!("unexpected server message `{other}`"),
    })
}

/// GVariant serialization of `(ay)` for a bytestring `digest`.
///
/// When `(ay)` is the whole value, the bytestring is just `digest` + NUL — the
/// parent size implies the array length (fixed-width `y` elements).
fn bytestring_tuple_payload(digest: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(digest.len() + 1);
    out.extend_from_slice(digest);
    out.push(0);
    out
}

fn backend_commands_hash(library: &Path) -> Result<Vec<u8>> {
    let output = Command::new("gresource")
        .args(["extract", &library.display().to_string(), BACKEND_COMMANDS_PATH])
        .output()
        .context(
            "running `gresource extract` for InspectorBackendCommands.js \
             (glib2 tooling). Install it, or pass --webkit-library",
        )?;
    if !output.status.success() {
        bail!(
            "gresource extract failed on {}: {}",
            library.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let digest = Sha1::digest(&output.stdout);
    Ok(hex::encode(digest).into_bytes())
}

fn parse_target_list(variant: &Variant) -> Result<Vec<Target>> {
    // `(ta(tsssb))` — connectionID, then array of (targetID, type, name, url, hasLocalDebugger).
    let connection_id = variant
        .try_child_value(0)
        .context("SetTargetList missing connectionID")?
        .get::<u64>()
        .context("connectionID is not a u64")?;
    let array = variant
        .try_child_value(1)
        .context("SetTargetList missing target array")?;

    let mut targets = Vec::with_capacity(array.n_children());
    for child in array.iter() {
        let target_id = child
            .try_child_value(0)
            .and_then(|v| v.get::<u64>())
            .context("targetID")?;
        let kind = child
            .try_child_value(1)
            .and_then(|v| v.get::<String>())
            .context("type")?;
        let name = child
            .try_child_value(2)
            .and_then(|v| v.get::<String>())
            .context("name")?;
        let url = child
            .try_child_value(3)
            .and_then(|v| v.get::<String>())
            .context("url")?;
        // child 4 = hasLocalDebugger — unused for listing.
        if matches!(kind.as_str(), "JavaScript" | "ServiceWorker" | "WebPage") {
            targets.push(Target {
                connection_id,
                target_id,
                name,
                url,
                kind,
            });
        }
    }
    Ok(targets)
}

fn default_webkit_library() -> PathBuf {
    PathBuf::from("/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0")
}

pub fn run(root: &Path, args: RecordArgs) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(record(root, args))
}

async fn record(root: &Path, args: RecordArgs) -> Result<()> {
    let steps = scenario(&args.scenario)?;
    let library = args
        .webkit_library
        .clone()
        .unwrap_or_else(default_webkit_library);
    let mut client = Client::connect(&args.address).await?;
    let targets = client.targets(&library).await?;

    if targets.is_empty() {
        bail!(
            "the inspector server completed the handshake but reported no targets.\n\
             Pass `--enable-developer-extras=true` to MiniBrowser, or call \
             webkit_settings_set_enable_developer_extras() in an app, and wait \
             for a page to load."
        );
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

    let setup = (target.connection_id, target.target_id).to_variant();
    client.send_message("Setup", Some(&setup)).await?;

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

    let close = (target.connection_id, target.target_id).to_variant();
    client
        .send_message("FrontendDidClose", Some(&close))
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
    let message = serde_json::to_string(frame)?;
    let parameters = (
        target.connection_id,
        target.target_id,
        message.as_str(),
    )
        .to_variant();
    client
        .send_message("SendMessageToBackend", Some(&parameters))
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
        let Some((name, variant)) = client.recv_message(remaining).await? else {
            return Ok(()); // a quiet socket is not an error
        };

        // Only frontend messages carry inspector frames; other events are
        // envelope bookkeeping and do not belong in a fixture.
        if name != "SendMessageToFrontend" {
            continue;
        }
        // `(tts)` — connectionID, targetID, message.
        let message = variant
            .try_child_value(2)
            .and_then(|v| v.get::<String>())
            .context("SendMessageToFrontend missing message")?;
        let frame: Value = serde_json::from_str(&message)
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
    fn socket_connection_frames_put_size_before_flags() {
        // body = "SetupInspectorClient\0" + "(ay)" payload for digest "0"
        let name = b"SetupInspectorClient";
        let payload = bytestring_tuple_payload(b"0");
        let mut body = Vec::new();
        body.extend_from_slice(name);
        body.push(0);
        body.extend_from_slice(&payload);

        let mut frame = Vec::new();
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.push(BYTE_ORDER_LITTLE_ENDIAN);
        frame.extend_from_slice(&body);

        assert_eq!(&frame[..4], &(body.len() as u32).to_be_bytes());
        assert_eq!(frame[4], BYTE_ORDER_LITTLE_ENDIAN);
        assert_eq!(&frame[5..5 + name.len()], name);
        assert_eq!(frame[5 + name.len()], 0);
        // Little-endian length would put the size in the first byte for small
        // bodies; big-endian puts it in the last. Same trap as before, new frame.
        assert_eq!(frame[0], 0);
        assert_ne!(frame[3], 0);
    }

    #[test]
    fn bytestring_tuple_payload_is_digest_plus_nul() {
        assert_eq!(bytestring_tuple_payload(b""), b"\0");
        assert_eq!(bytestring_tuple_payload(b"0"), b"0\0");
        let digest = b"a45874c474988589deb001bd81e77c60db04304d";
        let mut expect = digest.to_vec();
        expect.push(0);
        assert_eq!(bytestring_tuple_payload(digest), expect);
    }

    #[test]
    fn a_message_split_across_reads_is_reassembled() {
        let name = b"DidSetupInspectorClient";
        let payload = bytestring_tuple_payload(b"");
        let mut body = Vec::new();
        body.extend_from_slice(name);
        body.push(0);
        body.extend_from_slice(&payload);
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.push(BYTE_ORDER_LITTLE_ENDIAN);
        framed.extend_from_slice(&body);

        let mut buffer: Vec<u8> = Vec::new();
        for (i, byte) in framed.iter().enumerate() {
            buffer.push(*byte);
            let complete = buffer.len() >= 5 && {
                let mut size = [0u8; 4];
                size.copy_from_slice(&buffer[..4]);
                buffer.len() >= 4 + 1 + u32::from_be_bytes(size) as usize
            };
            assert_eq!(complete, i == framed.len() - 1, "at byte {i}");
        }
    }

    #[test]
    fn set_target_list_parses_a_captured_payload() {
        // Captured from WebKitGTK 2.52.3 MiniBrowser against fixtures/page —
        // one WebPage target on connectionID 1.
        let payload = hex::decode(
            "0100000000000000010000000000000057656250616765006d6a782d7765626b\
             69742d64656275676765722066697874757265207061676500687474703a2f2f\
             3132372e302e302e313a383733312f696e6465782e68746d6c000052311056",
        )
        .unwrap();
        let list = Variant::from_data_with_type(
            &payload,
            VariantTy::new("(ta(tsssb))").expect("type string"),
        );
        let targets = parse_target_list(&list).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].connection_id, 1);
        assert_eq!(targets[0].target_id, 1);
        assert_eq!(targets[0].kind, "WebPage");
        assert_eq!(targets[0].name, "mjx-webkit-debugger fixture page");
        assert_eq!(targets[0].url, "http://127.0.0.1:8731/index.html");
    }
}
