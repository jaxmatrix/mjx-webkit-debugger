//! Discovery and attachment, shared by the CLI and the picker.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use anyhow::{Context, Result};
use mjx_wk_transport::{Discovery, TcpInspectorServer};

/// Print the inspectable targets at an address.
///
/// Useful on its own — "is my app exposing an inspector at all?" is the first
/// question when an attach fails, and answering it should not require opening
/// a window.
pub fn list(address: &str) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building tokio runtime for discovery")?;

    let targets = runtime.block_on(async {
        let server = TcpInspectorServer::new(address);
        server.list().await
    })?;

    if targets.is_empty() {
        println!("No inspectable targets at {address}.");
        println!(
            "Start the debuggee with WEBKIT_INSPECTOR_SERVER={address} and \
             developer extras enabled."
        );
        return Ok(());
    }

    println!("Inspectable targets at {address}:");
    for (index, target) in targets.iter().enumerate() {
        let name = if target.name.is_empty() {
            "(untitled)"
        } else {
            target.name.as_str()
        };
        let url = if target.url.is_empty() {
            "—"
        } else {
            target.url.as_str()
        };
        println!(
            "  [{index}] {name}  {url}  ({kind})  key={key}",
            kind = target.kind,
            key = target.key,
        );
    }
    Ok(())
}
