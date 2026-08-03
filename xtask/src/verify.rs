//! Repo invariants that CI enforces.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Where `codegen` records what it generated, so `verify-protocol` can work
/// without `reference/` being present.
pub const MANIFEST: &str = "crates/mjx-wk-protocol/protocol-manifest.json";

/// The resource path of the generated protocol description inside a WebKit
/// build's GResource bundle.
const IBC_RESOURCE: &str =
    "/org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js";

/// Libraries to try when `--library` is not given.
const DEFAULT_LIBRARIES: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0",
    "/usr/lib/x86_64-linux-gnu/libwebkitgtk-6.0.so.4",
    "/usr/lib64/libwebkit2gtk-4.1.so.0",
    "/usr/lib/libwebkit2gtk-4.1.so.0",
];

/// What the generator produced, per domain.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProtocolManifest {
    /// The WebKit ref the descriptions were taken from.
    pub pinned_ref: String,
    /// Domain name → the command and event member names generated for it.
    pub domains: BTreeMap<String, DomainMembers>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DomainMembers {
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

/// Diff the generated protocol against the WebKit that is actually installed.
pub fn protocol(root: &Path, library: Option<&Path>, allow_drift: bool) -> Result<()> {
    let manifest_path = root.join(MANIFEST);
    let manifest: ProtocolManifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .context("parsing the protocol manifest")?;

    let library = match library {
        Some(p) => p.to_path_buf(),
        None => find_library().context(
            "no system WebKit library found. Pass --library, or skip this check on a \
             machine without WebKitGTK installed",
        )?,
    };

    let runtime = extract_runtime_protocol(&library)?;
    println!(
        "comparing {} generated domains (ref {}) against {}",
        manifest.domains.len(),
        manifest.pinned_ref,
        library.display()
    );

    let mut missing = Vec::new(); // runtime has it, we did not generate it
    let mut extra = Vec::new(); // we generated it, this build does not expose it

    for (domain, members) in &runtime {
        match manifest.domains.get(domain) {
            None => missing.push(format!("{domain} (whole domain)")),
            Some(ours) => {
                for m in &members.commands {
                    if !ours.commands.contains(m) {
                        missing.push(format!("{domain}.{m} (command)"));
                    }
                }
                for m in &members.events {
                    if !ours.events.contains(m) {
                        missing.push(format!("{domain}.{m} (event)"));
                    }
                }
            }
        }
    }

    for (domain, ours) in &manifest.domains {
        match runtime.get(domain) {
            None => extra.push(format!("{domain} (whole domain)")),
            Some(theirs) => {
                for m in &ours.commands {
                    if !theirs.commands.contains(m) {
                        extra.push(format!("{domain}.{m} (command)"));
                    }
                }
                for m in &ours.events {
                    if !theirs.events.contains(m) {
                        extra.push(format!("{domain}.{m} (event)"));
                    }
                }
            }
        }
    }

    // Generating more than a build exposes is normal and safe: the session
    // gates on what the debuggee announces, so an unexposed member is simply
    // never sent. It is reported, not failed.
    if !extra.is_empty() {
        println!(
            "\n{} generated member(s) this build does not expose (expected; \
             gated at runtime):",
            extra.len()
        );
        for e in extra.iter().take(20) {
            println!("  - {e}");
        }
        if extra.len() > 20 {
            println!("  … and {} more", extra.len() - 20);
        }
    }

    // The other direction is a real problem: the debuggee can speak something
    // we have no types for, and we would silently ignore it.
    if !missing.is_empty() {
        println!(
            "\n{} member(s) this WebKit exposes but we did not generate:",
            missing.len()
        );
        for m in &missing {
            println!("  - {m}");
        }
        if !allow_drift {
            bail!(
                "protocol drift: the installed WebKit is ahead of the pinned ref.\n\
                 Update `reference/README.md`'s pinned ref, repopulate \
                 reference/webkit-protocol/, and re-run `cargo run -p xtask -- codegen`.\n\
                 Pass --allow-drift to report without failing."
            );
        }
    }

    if missing.is_empty() && extra.is_empty() {
        println!("\nprotocol matches exactly.");
    }
    Ok(())
}

fn find_library() -> Option<PathBuf> {
    DEFAULT_LIBRARIES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

/// Pull the protocol description out of a WebKit build and parse it.
///
/// WebKit compiles the inspector frontend into a GResource bundle linked into
/// the shared library, and `InspectorBackendCommands.js` inside it is generated
/// from the same descriptions we generate from. It is therefore the ground
/// truth for what *this* build speaks, which can differ from upstream source —
/// WebKitGTK ships `Security` in source but does not activate it.
fn extract_runtime_protocol(library: &Path) -> Result<BTreeMap<String, DomainMembers>> {
    if !library.exists() {
        bail!("{} does not exist", library.display());
    }
    let output = Command::new("gresource")
        .arg("extract")
        .arg(library)
        .arg(IBC_RESOURCE)
        .output()
        .context(
            "running `gresource` (from glib2 tooling). Install it, or pass \
             --library pointing at a build you can read",
        )?;

    if !output.status.success() {
        bail!(
            "gresource extract failed on {}: {}",
            library.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let js = String::from_utf8_lossy(&output.stdout);
    if js.trim().is_empty() {
        bail!(
            "{} contains no inspector protocol resource. Is it really a WebKit build?",
            library.display()
        );
    }
    Ok(parse_backend_commands(&js))
}

/// Extract `Domain.member` pairs from `InspectorBackendCommands.js`.
///
/// The file is a flat list of registration calls:
///
/// ```text
/// InspectorBackend.registerCommand("Debugger.setBreakpointByUrl", null, [ … ], [ … ]);
/// InspectorBackend.registerEvent("Debugger.paused", null, [ … ]);
/// ```
fn parse_backend_commands(js: &str) -> BTreeMap<String, DomainMembers> {
    let mut out: BTreeMap<String, DomainMembers> = BTreeMap::new();

    for line in js.lines() {
        let line = line.trim_start();
        let (kind, rest) = if let Some(r) = line.strip_prefix("InspectorBackend.registerCommand(\"")
        {
            (Kind::Command, r)
        } else if let Some(r) = line.strip_prefix("InspectorBackend.registerEvent(\"") {
            (Kind::Event, r)
        } else {
            continue;
        };

        let Some(qualified) = rest.split('"').next() else {
            continue;
        };
        let Some((domain, member)) = qualified.split_once('.') else {
            continue;
        };

        let entry = out.entry(domain.to_owned()).or_default();
        let bucket = match kind {
            Kind::Command => &mut entry.commands,
            Kind::Event => &mut entry.events,
        };
        let member = member.to_owned();
        if !bucket.contains(&member) {
            bucket.push(member);
        }
    }
    out
}

enum Kind {
    Command,
    Event,
}

/// Crates that mean a webview got into the graph.
///
/// This is the project's central architectural claim: the debugger is not a
/// WebKit program, so a crash or hang in the debuggee cannot take it down.
const FORBIDDEN: &[&str] = &[
    "wry",
    "tauri",
    "tauri-runtime",
    "webkit2gtk",
    "webkit2gtk-sys",
    "javascriptcore-rs",
    "javascriptcore-rs-sys",
    "webview2-com",
    "webview2-com-sys",
    "webkit6",
    "servo",
];

/// Fail if any crate in the workspace graph links a webview.
pub fn no_webview(root: &Path) -> Result<()> {
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "tree",
            "--workspace",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .context("running `cargo tree`")?;

    if !output.status.success() {
        bail!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let tree = String::from_utf8_lossy(&output.stdout);
    let mut found: Vec<&str> = Vec::new();

    for line in tree.lines() {
        let name = line.split_whitespace().next().unwrap_or_default();
        if FORBIDDEN.contains(&name) && !found.contains(&name) {
            found.push(name);
        }
    }

    if !found.is_empty() {
        bail!(
            "a webview reached the dependency graph: {}.\n\
             The debugger must not embed the engine it debugs — see the \
             architecture rules in CLAUDE.md.",
            found.join(", ")
        );
    }

    println!(
        "no webview in the dependency graph ({} crates checked)",
        tree.lines().count()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_and_events_into_domains() {
        let js = r#"
InspectorBackend.registerCommand("Debugger.setBreakpointByUrl", null, [{"name": "lineNumber"}], ["breakpointId"]);
InspectorBackend.registerEvent("Debugger.paused", null, ["callFrames", "reason"]);
InspectorBackend.registerEnum("Debugger.ScopeType", {Global: "global"});
InspectorBackend.activateDomain("Debugger", ["web-page"]);
"#;
        let parsed = parse_backend_commands(js);
        let debugger = &parsed["Debugger"];
        assert_eq!(debugger.commands, ["setBreakpointByUrl"]);
        assert_eq!(debugger.events, ["paused"]);
        // Enums and activations are not members and must not be counted.
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn ignores_lines_that_are_not_registrations() {
        let parsed = parse_backend_commands("// a comment\nvar x = 1;\n");
        assert!(parsed.is_empty());
    }

    #[test]
    fn a_member_registered_twice_is_recorded_once() {
        let js = "InspectorBackend.registerCommand(\"DOM.highlightNode\", null, [], []);\n\
                  InspectorBackend.registerCommand(\"DOM.highlightNode\", null, [], []);\n";
        assert_eq!(parse_backend_commands(js)["DOM"].commands.len(), 1);
    }
}
