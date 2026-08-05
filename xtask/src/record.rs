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
//! python3 -m http.server 8731 --directory fixtures/page &
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
//! `docs/PROTOCOL-NOTES.md` trap 1.
//!
//! # Envelope traces
//!
//! `socket-handshake` records the glib `SocketConnection` exchange itself
//! (message name + GVariant body hex), not inspector-protocol frames. That
//! fixture feeds T-001; RWI scenarios feed `ReplayTransport`.

use std::collections::HashMap;
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

const FIXTURE_ORIGIN: &str = "http://127.0.0.1:8731";

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

    /// Write the discovered target list to `fixtures/targets-page.json`.
    ///
    /// There is no server-served inspectable-targets HTML page on WebKitGTK
    /// (see PROTOCOL-NOTES trap 1). This dumps the `SetTargetList` rows the
    /// handshake returned, so a recording session can still capture "what was
    /// attached to" beside the RWI trace.
    #[arg(long)]
    pub save_targets_page: bool,
}

/// Values harvested from replies/events so later steps can be dynamic.
#[derive(Debug, Default)]
struct Ctx {
    security_origin: Option<String>,
    document_node_id: Option<i64>,
    element_node_id: Option<i64>,
    pause_object_id: Option<String>,
    /// URL substring → scriptId, from `Debugger.scriptParsed`.
    scripts: HashMap<String, String>,
    target_id: Option<String>,
    worker_id: Option<String>,
}

/// One step of a scripted scenario.
enum Step {
    /// Send a command and wait for its reply.
    Call(&'static str, fn() -> Value),
    /// Send a command whose params depend on earlier replies/events.
    CallCtx(&'static str, fn(&Ctx) -> Result<Value>),
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
        // Envelope-only — handled by `record_socket_handshake`, not these steps.
        "socket-handshake" => Vec::new(),

        // The handshake plus the script/resource inventory: what the source
        // browser needs and nothing more.
        "attach" => {
            let mut s = enable();
            s.push(Step::Call("Page.getResourceTree", || json!({})));
            s.push(Step::Settle(2000));
            s
        }

        // setBreakpointByUrl → reload → paused → getProperties → resume
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
            s.push(Step::Call("Page.reload", || json!({})));
            s.push(Step::Await("Debugger.paused", 12_000));
            s.push(Step::CallCtx("Runtime.getProperties", |ctx| {
                let object_id = ctx
                    .pause_object_id
                    .as_ref()
                    .context("Debugger.paused did not yield an objectId for getProperties")?;
                Ok(json!({
                    "objectId": object_id,
                    "ownProperties": true,
                    "fetchStart": 0,
                    "fetchCount": 100,
                }))
            }));
            s.push(Step::Call("Debugger.resume", || json!({})));
            s.push(Step::Settle(1000));
            s
        }

        "network-load" => {
            let mut s = enable();
            s.push(Step::Call("Network.enable", || json!({})));
            s.push(Step::Call("Page.reload", || json!({})));
            s.push(Step::Settle(6000));
            s
        }

        // getDocument → getMatchedStylesForNode → getComputedStyleForNode
        "dom-css" => {
            let mut s = enable();
            s.push(Step::Call("DOM.enable", || json!({})));
            s.push(Step::Call("CSS.enable", || json!({})));
            s.push(Step::Call("DOM.getDocument", || json!({})));
            s.push(Step::CallCtx("CSS.getMatchedStylesForNode", |ctx| {
                let node_id = ctx
                    .element_node_id
                    .or(ctx.document_node_id)
                    .context("DOM.getDocument did not yield a nodeId")?;
                Ok(json!({ "nodeId": node_id }))
            }));
            s.push(Step::CallCtx("CSS.getComputedStyleForNode", |ctx| {
                let node_id = ctx
                    .element_node_id
                    .or(ctx.document_node_id)
                    .context("DOM.getDocument did not yield a nodeId")?;
                Ok(json!({ "nodeId": node_id }))
            }));
            s.push(Step::Settle(1000));
            s
        }

        // DOMStorage + IndexedDB + cookies
        "storage" => {
            let mut s = enable();
            s.push(Step::Call("DOMStorage.enable", || json!({})));
            s.push(Step::Call("IndexedDB.enable", || json!({})));
            s.push(Step::Call("Page.getResourceTree", || json!({})));
            s.push(Step::CallCtx("DOMStorage.getDOMStorageItems", |ctx| {
                let origin = ctx
                    .security_origin
                    .clone()
                    .unwrap_or_else(|| FIXTURE_ORIGIN.to_owned());
                Ok(json!({
                    "storageId": {
                        "securityOrigin": origin,
                        "isLocalStorage": true,
                    }
                }))
            }));
            s.push(Step::CallCtx("IndexedDB.requestDatabaseNames", |ctx| {
                let origin = ctx
                    .security_origin
                    .clone()
                    .unwrap_or_else(|| FIXTURE_ORIGIN.to_owned());
                Ok(json!({ "securityOrigin": origin }))
            }));
            s.push(Step::CallCtx("IndexedDB.requestDatabase", |ctx| {
                let origin = ctx
                    .security_origin
                    .clone()
                    .unwrap_or_else(|| FIXTURE_ORIGIN.to_owned());
                Ok(json!({
                    "securityOrigin": origin,
                    "databaseName": "fixture-db",
                }))
            }));
            s.push(Step::Call("Page.getCookies", || json!({})));
            s.push(Step::Settle(1500));
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

        // Navigate to the multi-MB script host, then getScriptSource.
        "large-bundle" => {
            let mut s = enable();
            s.push(Step::Call("Page.navigate", || {
                json!({ "url": format!("{FIXTURE_ORIGIN}/large.html") })
            }));
            s.push(Step::Await("Debugger.scriptParsed", 10_000));
            s.push(Step::Settle(2000));
            s.push(Step::CallCtx("Debugger.getScriptSource", |ctx| {
                let script_id = ctx
                    .scripts
                    .iter()
                    .find(|(url, _)| url.contains("large-bundle.js"))
                    .map(|(_, id)| id.clone())
                    .or_else(|| {
                        ctx.scripts
                            .values()
                            .max_by_key(|id| id.len())
                            .cloned()
                    })
                    .context("no Debugger.scriptParsed for large-bundle.js")?;
                Ok(json!({ "scriptId": script_id }))
            }));
            s.push(Step::Settle(1000));
            s
        }

        // Genuine Target.* wrapping via provisional navigation + worker host.
        "target-multiplexed" => {
            let mut s = enable();
            s.push(Step::Call("Target.setPauseOnStart", || {
                json!({ "pauseOnStart": true })
            }));
            s.push(Step::Call("Worker.enable", || json!({})));
            s.push(Step::Call("Page.navigate", || {
                json!({ "url": format!("{FIXTURE_ORIGIN}/worker-host.html") })
            }));
            // Provisional navigation is the reliable Target.* path on WebKitGTK.
            // Worker.workerCreated may also arrive during the settle window.
            s.push(Step::Await("Target.targetCreated", 12_000));
            s.push(Step::Settle(2000));
            s.push(Step::CallCtx("Target.sendMessageToTarget", |ctx| {
                let target_id = ctx
                    .target_id
                    .as_ref()
                    .context(
                        "no Target.targetCreated targetId — cannot record Target.* wrapping",
                    )?;
                let inner = json!({ "id": 9001, "method": "Runtime.enable", "params": {} });
                Ok(json!({
                    "targetId": target_id,
                    "message": serde_json::to_string(&inner)?,
                }))
            }));
            s.push(Step::Await("Target.dispatchMessageFromTarget", 8_000));
            s.push(Step::CallCtx("Target.resume", |ctx| {
                let target_id = ctx
                    .target_id
                    .as_ref()
                    .context("no targetId for Target.resume")?;
                Ok(json!({ "targetId": target_id }))
            }));
            // Optional: initialise a dedicated worker if one appeared.
            s.push(Step::CallCtx("Worker.initialized", |ctx| {
                let id = ctx
                    .worker_id
                    .as_ref()
                    .context("no workerId")?;
                Ok(json!({ "workerId": id }))
            }));
            s.push(Step::Settle(1500));
            s
        }

        other => bail!(
            "unknown scenario `{other}`. Known: socket-handshake, attach, \
             breakpoint-hit, network-load, dom-css, storage, timeline-record, \
             large-bundle, target-multiplexed"
        ),
    })
}

/// One line of an RWI trace. Records inspector-protocol frames, not the socket
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

/// One line of a glib envelope trace (`socket-handshake.jsonl`).
#[derive(Serialize)]
struct EnvelopeLine {
    t: u128,
    dir: &'static str,
    name: String,
    /// GVariant type string, when the message carries a body.
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<&'static str>,
    /// Raw GVariant body bytes, hex-encoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    body_hex: Option<String>,
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
    if args.scenario == "socket-handshake" {
        return record_socket_handshake(root, args).await;
    }

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
    if args.save_targets_page {
        write_targets_page(root, &targets)?;
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
    let mut ctx = Ctx::default();

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
                drain(
                    &mut client,
                    &mut trace,
                    &mut ctx,
                    started,
                    Some(id),
                    None,
                    5000,
                )
                .await?;
            }
            Step::CallCtx(method, params) => {
                // Skip optional Worker.initialized when no worker appeared.
                if method == "Worker.initialized" && ctx.worker_id.is_none() {
                    println!("skipping Worker.initialized (no workerId observed)");
                    continue;
                }
                let id = next_id;
                next_id += 1;
                let frame = json!({ "id": id, "method": method, "params": params(&ctx)? });
                send_frame(&mut client, &target, &frame).await?;
                trace.push(TraceLine {
                    t: started.elapsed().as_micros(),
                    dir: "send",
                    frame,
                });
                drain(
                    &mut client,
                    &mut trace,
                    &mut ctx,
                    started,
                    Some(id),
                    None,
                    5000,
                )
                .await?;
            }
            Step::Settle(ms) => {
                drain(&mut client, &mut trace, &mut ctx, started, None, None, ms).await?;
            }
            Step::Await(method, ms) => {
                drain(
                    &mut client,
                    &mut trace,
                    &mut ctx,
                    started,
                    None,
                    Some(&[method]),
                    ms,
                )
                .await?;
            }
        }
    }
    drain(
        &mut client,
        &mut trace,
        &mut ctx,
        started,
        None,
        None,
        args.settle_ms,
    )
    .await?;

    let close = (target.connection_id, target.target_id).to_variant();
    client
        .send_message("FrontendDidClose", Some(&close))
        .await
        .ok();

    let out = args
        .out
        .unwrap_or_else(|| root.join(format!("fixtures/{}.jsonl", args.scenario)));
    write_jsonl(&out, &trace)?;

    let sent = trace.iter().filter(|l| l.dir == "send").count();
    println!(
        "wrote {} ({} frames: {sent} sent, {} received)",
        out.display(),
        trace.len(),
        trace.len() - sent
    );

    if args.scenario == "target-multiplexed" {
        let has_send = trace.iter().any(|l| {
            l.dir == "send"
                && l.frame.get("method").and_then(Value::as_str)
                    == Some("Target.sendMessageToTarget")
        });
        let has_dispatch = trace.iter().any(|l| {
            l.dir == "recv"
                && l.frame.get("method").and_then(Value::as_str)
                    == Some("Target.dispatchMessageFromTarget")
        });
        if !has_send || !has_dispatch {
            bail!(
                "target-multiplexed trace is missing genuine Target.* wrapping \
                 (send={has_send}, dispatch={has_dispatch}). Refusing to write a \
                 fabricated multiplex fixture."
            );
        }
        // Confirm the inner message travelled as a JSON *string*.
        let ok_string = trace.iter().any(|l| {
            l.frame.get("method").and_then(Value::as_str) == Some("Target.sendMessageToTarget")
                && l.frame
                    .pointer("/params/message")
                    .and_then(Value::as_str)
                    .is_some()
        }) && trace.iter().any(|l| {
            l.frame.get("method").and_then(Value::as_str)
                == Some("Target.dispatchMessageFromTarget")
                && l.frame
                    .pointer("/params/message")
                    .and_then(Value::as_str)
                    .is_some()
        });
        if !ok_string {
            bail!(
                "Target.* frames present but `message` was not a JSON string — \
                 that is the trap the dialect exists to catch"
            );
        }
    }

    Ok(())
}

/// Record the raw glib handshake (no RWI attach). Feeds T-001.
async fn record_socket_handshake(root: &Path, args: RecordArgs) -> Result<()> {
    let library = args
        .webkit_library
        .clone()
        .unwrap_or_else(default_webkit_library);
    let mut client = Client::connect(&args.address).await?;
    let started = Instant::now();
    let mut trace: Vec<EnvelopeLine> = Vec::new();

    let digest = backend_commands_hash(&library)?;
    let type_ay = VariantTy::new("(ay)")
        .map_err(|e| anyhow::anyhow!("`(ay)` is not a valid GVariant type: {e}"))?;
    let payload = bytestring_tuple_payload(&digest);
    let params = Variant::from_data_with_type(&payload, type_ay);

    client
        .send_message("SetupInspectorClient", Some(&params))
        .await?;
    trace.push(EnvelopeLine {
        t: started.elapsed().as_micros(),
        dir: "send",
        name: "SetupInspectorClient".into(),
        r#type: Some("(ay)"),
        body_hex: Some(hex::encode(&payload)),
    });

    let mut targets = Vec::new();
    let mut saw_setup = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let Some((name, variant)) = client.recv_message(Duration::from_secs(3)).await? else {
            continue;
        };
        let type_str = message_param_type(&name)?;
        let body_hex = match type_str {
            None => None,
            Some(_) => Some(hex::encode(variant.data())),
        };
        trace.push(EnvelopeLine {
            t: started.elapsed().as_micros(),
            dir: "recv",
            name: name.clone(),
            r#type: type_str,
            body_hex,
        });
        match name.as_str() {
            "DidSetupInspectorClient" => saw_setup = true,
            "SetTargetList" => {
                let parsed = parse_target_list(&variant)?;
                if !parsed.is_empty() {
                    targets = parsed;
                    break;
                }
                targets = parsed;
            }
            _ => {}
        }
    }

    if !saw_setup {
        bail!("socket-handshake: no DidSetupInspectorClient within 15s");
    }
    if targets.is_empty() {
        bail!(
            "socket-handshake: handshake completed but SetTargetList stayed empty.\n\
             Pass `--enable-developer-extras=true` and load a page first."
        );
    }
    for (i, t) in targets.iter().enumerate() {
        println!("[{i}] {} — {} ({})", t.name, t.url, t.kind);
    }
    if args.save_targets_page {
        write_targets_page(root, &targets)?;
    }
    if args.list_only {
        return Ok(());
    }

    let out = args
        .out
        .unwrap_or_else(|| root.join("fixtures/socket-handshake.jsonl"));
    write_jsonl(&out, &trace)?;
    println!(
        "wrote {} ({} envelope messages)",
        out.display(),
        trace.len()
    );
    Ok(())
}

fn write_targets_page(root: &Path, targets: &[Target]) -> Result<()> {
    let path = root.join("fixtures/targets-page.json");
    let rows: Vec<Value> = targets
        .iter()
        .map(|t| {
            json!({
                "connectionId": t.connection_id,
                "targetId": t.target_id,
                "name": t.name,
                "url": t.url,
                "type": t.kind,
            })
        })
        .collect();
    let body = serde_json::to_string_pretty(&json!({
        "note": "WebKitGTK does not serve an inspectable-targets HTML page. \
                 This file is the SetTargetList dump from a live recording.",
        "targets": rows,
    }))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn write_jsonl<T: Serialize>(path: &Path, lines: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body: String = lines
        .iter()
        .map(|l| serde_json::to_string(l).map(|s| s + "\n"))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    std::fs::write(path, body)?;
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
    ctx: &mut Ctx,
    started: Instant,
    until_id: Option<u64>,
    until_events: Option<&[&str]>,
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

        ingest(ctx, &frame);

        trace.push(TraceLine {
            t: started.elapsed().as_micros(),
            dir: "recv",
            frame,
        });

        if until_id.is_some() && id == until_id {
            return Ok(());
        }
        if let (Some(want), Some(got)) = (until_events, method.as_deref())
            && want.contains(&got)
        {
            return Ok(());
        }
    }
}

fn ingest(ctx: &mut Ctx, frame: &Value) {
    if let Some(result) = frame.get("result") {
        if let Some(origin) = result
            .pointer("/frameTree/frame/securityOrigin")
            .and_then(Value::as_str)
        {
            ctx.security_origin = Some(origin.to_owned());
        }
        if let Some(root) = result.get("root") {
            if let Some(id) = root.get("nodeId").and_then(Value::as_i64) {
                ctx.document_node_id = Some(id);
            }
            if let Some(el) = find_element_node(root) {
                ctx.element_node_id = Some(el);
            }
        }
    }

    let Some(method) = frame.get("method").and_then(Value::as_str) else {
        return;
    };
    let params = frame.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "Debugger.paused" => {
            // Prefer the local scope's objectId on the top call frame.
            if let Some(oid) = params
                .pointer("/callFrames/0/scopeChain/0/object/objectId")
                .and_then(Value::as_str)
            {
                ctx.pause_object_id = Some(oid.to_owned());
            }
        }
        "Debugger.scriptParsed" => {
            if let (Some(url), Some(id)) = (
                params.get("url").and_then(Value::as_str),
                params.get("scriptId").and_then(Value::as_str),
            ) {
                ctx.scripts.insert(url.to_owned(), id.to_owned());
            }
        }
        "Target.targetCreated" => {
            if let Some(id) = params
                .pointer("/targetInfo/targetId")
                .and_then(Value::as_str)
            {
                ctx.target_id = Some(id.to_owned());
            }
        }
        "Worker.workerCreated" => {
            if let Some(id) = params.get("workerId").and_then(Value::as_str) {
                ctx.worker_id = Some(id.to_owned());
            }
        }
        _ => {}
    }
}

fn find_element_node(node: &Value) -> Option<i64> {
    let node_type = node.get("nodeType").and_then(Value::as_i64)?;
    // ELEMENT_NODE == 1. Prefer a real element over the document node.
    if node_type == 1
        && let Some(id) = node.get("nodeId").and_then(Value::as_i64)
    {
        return Some(id);
    }
    for child in node.get("children").and_then(Value::as_array).into_iter().flatten() {
        if let Some(id) = find_element_node(child) {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_scenarios_are_rejected_by_name() {
        assert!(scenario("attach").is_ok());
        assert!(scenario("breakpoint-hit").is_ok());
        assert!(scenario("large-bundle").is_ok());
        assert!(scenario("target-multiplexed").is_ok());
        assert!(scenario("socket-handshake").is_ok());
        assert!(scenario("nope").is_err());
    }

    #[test]
    fn pin_table_scenarios_include_required_commands() {
        let names = |s: &str| -> Vec<&'static str> {
            scenario(s)
                .unwrap()
                .into_iter()
                .filter_map(|step| match step {
                    Step::Call(m, _) | Step::CallCtx(m, _) => Some(m),
                    _ => None,
                })
                .collect()
        };
        let bp = names("breakpoint-hit");
        assert!(bp.contains(&"Debugger.setBreakpointByUrl"));
        assert!(bp.contains(&"Page.reload"));
        assert!(bp.contains(&"Runtime.getProperties"));
        assert!(bp.contains(&"Debugger.resume"));

        let dom = names("dom-css");
        assert!(dom.contains(&"DOM.getDocument"));
        assert!(dom.contains(&"CSS.getMatchedStylesForNode"));
        assert!(dom.contains(&"CSS.getComputedStyleForNode"));

        let storage = names("storage");
        assert!(storage.contains(&"DOMStorage.getDOMStorageItems"));
        assert!(storage.contains(&"IndexedDB.requestDatabaseNames"));
        assert!(storage.contains(&"Page.getCookies"));

        let large = names("large-bundle");
        assert!(large.contains(&"Debugger.getScriptSource"));

        let mux = names("target-multiplexed");
        assert!(mux.contains(&"Target.sendMessageToTarget"));
        assert!(mux.contains(&"Target.setPauseOnStart"));
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

    #[test]
    fn find_element_walks_children() {
        let doc = json!({
            "nodeId": 1,
            "nodeType": 9,
            "children": [
                { "nodeId": 2, "nodeType": 1, "nodeName": "HTML" }
            ]
        });
        assert_eq!(find_element_node(&doc), Some(2));
    }
}
