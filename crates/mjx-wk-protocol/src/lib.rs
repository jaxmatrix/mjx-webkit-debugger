//! L0 — the WebKit Remote Inspector protocol, as Rust types.
//!
//! This crate is the bottom of the stack. It depends on `serde` and nothing
//! else in the workspace, and it performs no I/O: it knows how a frame is
//! shaped, not how one travels.
//!
//! # Layout
//!
//! - [`frame`] — the JSON-RPC-ish envelope every message rides in.
//! - [`domain`] — the domain / debuggable / target vocabularies.
//! - [`error`] — the protocol error type.
//! - [`generated`] — one module per domain, produced by `xtask codegen` from
//!   the descriptions in `reference/webkit-protocol/`. **Committed**, so a
//!   clean clone builds without that directory present.
//!
//! # The two traits everything else is written against
//!
//! [`Command`] and [`Event`] tie a Rust type to a wire method name. A caller
//! never types a method string:
//!
//! ```ignore
//! let reply = session.call(debugger::GetScriptSource { script_id }).await?;
//! //  reply: debugger::GetScriptSourceReturns — the type comes from the command.
//! ```
//!
//! # Note on fidelity
//!
//! WebKit's protocol is *not* the Chrome DevTools Protocol, and the differences
//! are not cosmetic — see `docs/PROTOCOL-NOTES.md`. Types here follow WebKit.
//! Chromium is reached by translating into these types in `mjx-wk-dialect`,
//! never by loosening them.

pub mod domain;
pub mod error;
pub mod frame;
pub mod generated;

pub use domain::{DebuggableType, Domain, TargetType};
pub use error::{ProtocolError, Result};
pub use frame::{Frame, RequestId};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A request that can be sent to a debuggee, paired with the type of its reply.
///
/// Implementations are generated. `METHOD` is the bare member name; the wire
/// method is `"{DOMAIN}.{METHOD}"`, which [`Command::qualified_method`] builds.
pub trait Command: Serialize + Send + Sync {
    /// The domain this command belongs to.
    const DOMAIN: Domain;
    /// The member name, without the domain prefix (e.g. `"setBreakpointByUrl"`).
    const METHOD: &'static str;
    /// What the debuggee sends back on success.
    type Returns: DeserializeOwned + Send;

    /// The full wire method name, e.g. `"Debugger.setBreakpointByUrl"`.
    fn qualified_method() -> String {
        format!("{}.{}", Self::DOMAIN.as_str(), Self::METHOD)
    }
}

/// An unsolicited message from the debuggee.
pub trait Event: DeserializeOwned + Send + Sync {
    /// The domain this event belongs to.
    const DOMAIN: Domain;
    /// The member name, without the domain prefix (e.g. `"scriptParsed"`).
    const METHOD: &'static str;

    /// The full wire method name, e.g. `"Debugger.scriptParsed"`.
    fn qualified_method() -> String {
        format!("{}.{}", Self::DOMAIN.as_str(), Self::METHOD)
    }
}
