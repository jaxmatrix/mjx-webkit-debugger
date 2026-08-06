//! mjx-webkit-debugger — a native debugger for WebKit programs.
//!
//! # Why this is not itself a webview
//!
//! Every webview-based debugger shares a process model with the thing it
//! debugs. When the debuggee wedges the compositor or exhausts memory, the
//! tools go with it — exactly when you need them. This binary renders with
//! `wgpu` through `egui` and links no engine at all, which CI enforces:
//! `cargo run -p xtask -- verify-no-webview`.

mod app;
mod attach;
mod fixture_seed;
mod phase2_wiring;
mod session_host;
mod snapshot;
mod support;
mod ui_thread;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mjx-webkit-debugger", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Attach to a WebKit inspector server.
    ///
    /// Start the debuggee with the server enabled first:
    ///
    ///   WEBKIT_INSPECTOR_SERVER=127.0.0.1:2999 ./your-app
    Attach {
        /// The inspector server's address.
        #[arg(default_value = "127.0.0.1:2999")]
        address: String,

        /// Attach to this target without showing the picker.
        #[arg(long)]
        target: Option<usize>,
    },

    /// List inspectable targets and exit.
    List {
        #[arg(default_value = "127.0.0.1:2999")]
        address: String,
    },

    /// Replay a recorded trace instead of attaching.
    ///
    /// Runs the whole application against `fixtures/*.jsonl` with no debuggee,
    /// which is how the UI is developed and demonstrated offline.
    Replay {
        /// Path to a `.jsonl` trace.
        fixture: std::path::PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mjx_webkit_debugger=info,mjx_wk=info".into()),
        )
        .init();

    match Cli::parse().command {
        Some(Command::List { address }) => attach::list(&address),
        Some(Command::Attach { address, target }) => {
            app::run(app::Startup::Attach { address, target })
        }
        Some(Command::Replay { fixture }) => app::run(app::Startup::Replay { fixture }),
        // No arguments: open the window with the target picker showing.
        None => app::run(app::Startup::Picker),
    }
}
