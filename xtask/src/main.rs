//! Repo tooling: protocol codegen, fixture recording, perf benches, and invariant checks.
//!
//! `xtask` ships nothing. It is a workspace member so that `cargo run -p xtask`
//! works without installing anything, and it is never a dependency of a crate
//! that does ship.

mod bench;
mod codegen;
mod record;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "mjx-webkit-debugger repo tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Regenerate `mjx-wk-protocol` from `reference/webkit-protocol/`.
    ///
    /// The output is committed, so run this only when the pinned WebKit ref
    /// changes, and review the diff.
    Codegen,

    /// Check the generated protocol against the WebKit actually installed.
    ///
    /// Extracts `InspectorBackendCommands.js` from the system WebKit library
    /// and diffs the domains and members it announces against what we generated.
    /// This is what catches a WebKit upgrade at build time.
    VerifyProtocol {
        /// The library to read. Defaults to the system WebKitGTK 4.1.
        #[arg(long)]
        library: Option<PathBuf>,
        /// Report differences without failing. Useful when deliberately
        /// developing against a newer WebKit than the pinned ref.
        #[arg(long)]
        allow_drift: bool,
    },

    /// Assert no shipped crate links a webview.
    ///
    /// The whole point of the project is that the debugger is not itself a
    /// WebKit program. This is that rule, enforced.
    VerifyNoWebview,

    /// Replay every RWI fixture through `ReplayTransport` (no unmatched send).
    VerifyFixtures,

    /// Record a protocol trace from a live debuggee into `fixtures/`.
    Record(record::RecordArgs),

    /// Run performance budget benches (operation-count gates from `mjx-wk-perf`).
    ///
    /// Discovers workspace `[[bench]]` targets, including peer-owned ones such
    /// as T-005's `text` when present, and fails if any required CLAUDE.md
    /// budget regresses.
    Bench,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let root = repo_root();
    match Cli::parse().command {
        Command::Codegen => codegen::run(&root),
        Command::VerifyProtocol {
            library,
            allow_drift,
        } => verify::protocol(&root, library.as_deref(), allow_drift),
        Command::VerifyNoWebview => verify::no_webview(&root),
        Command::VerifyFixtures => verify::fixtures(&root),
        Command::Record(args) => record::run(&root, args),
        Command::Bench => bench::run(&root),
    }
}

/// The workspace root — `xtask/`'s parent, which is stable regardless of the
/// directory the command was invoked from.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `xtask/` always has a parent, but falling back beats panicking in a tool
    // whose whole job is to report problems clearly.
    manifest
        .parent()
        .map_or(manifest.clone(), Path::to_path_buf)
}
