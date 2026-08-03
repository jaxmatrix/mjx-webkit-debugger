//! L3 — everything the debuggee can show you as text, and where things are.
//!
//! This layer sits below every feature crate because they all need the same two
//! things: a way to name a piece of source, and a way to name a place in it.
//! A breakpoint is a location. A stack frame is a location. A CSS rule, a
//! network initiator, a profiler sample — all locations.
//!
//! Defining [`SourceId`], [`SourceLocation`] and the id vocabulary here is what
//! lets the nine L4 crates stay peers that never reference one another:
//! `mjx-wk-css` takes a [`NodeId`], not a `&DomTree` from `mjx-wk-dom`.
//!
//! # Sources come from two places and must look like one
//!
//! Scripts arrive as `Debugger.scriptParsed` events. Documents and stylesheets
//! do not — they come from `Page.getResourceTree` and are fetched with
//! `Page.getResourceContent`, which is keyed by **URL**, not by an id. The
//! [`inventory`] merges both into one tree so the UI has a single file list.

pub mod highlight;
pub mod inventory;
pub mod maps;
pub mod pretty;
pub mod search;
pub mod store;
pub mod text;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use highlight::{HighlightKind, HighlightSpan, Highlighter};
pub use inventory::{SourceEntry, SourceInventory, SourceTreeNode};
pub use maps::SourceMapResolver;
pub use pretty::PrettyPrinter;
pub use search::{SearchHit, SearchIndex, SearchQuery};
pub use store::SourceStore;
pub use text::{LineIndex, SourceText};

/// A source file, script, or stylesheet, as this process numbers them.
///
/// Deliberately *not* the debuggee's `scriptId`. A `scriptId` is a string, is
/// only valid for one page lifetime, and does not exist at all for documents
/// and stylesheets. A dense local id survives reloads, indexes a `Vec`, and is
/// `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(pub u32);

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source#{}", self.0)
    }
}

/// A DOM node, as the debuggee numbers them.
///
/// Lives here rather than in `mjx-wk-dom` so that `mjx-wk-css` can talk about
/// nodes without depending on `mjx-wk-dom` — the rule that keeps the L4 crates
/// parallel-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub i64);

/// A network request, as the debuggee names them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(pub String);

/// A frame within the page.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(pub String);

/// What a source is, which decides how its text is fetched and highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    /// JavaScript, from `Debugger.scriptParsed`.
    Script {
        /// An ES module rather than a classic script.
        module: bool,
        /// Injected by an extension rather than served by the page.
        content_script: bool,
    },
    /// An HTML document, from the resource tree.
    Document,
    /// A stylesheet.
    StyleSheet,
    /// An image, font, or anything else with no useful text.
    Other,
}

impl SourceKind {
    /// Whether this can carry breakpoints.
    pub fn is_debuggable(self) -> bool {
        // Documents count: inline `<script>` blocks live in the HTML, and
        // WebKit reports breakpoints in them against the document's URL.
        matches!(self, SourceKind::Script { .. } | SourceKind::Document)
    }
}

/// A position in a source.
///
/// Zero-based on both axes, matching the protocol. The UI adds one when it
/// displays a line number; nothing else should.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub source: SourceId,
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    /// A location at the start of a line.
    pub fn line_start(source: SourceId, line: u32) -> Self {
        Self {
            source,
            line,
            column: 0,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // One-based, because this is what a human reads.
        write!(f, "{}:{}:{}", self.source, self.line + 1, self.column + 1)
    }
}

/// Something went wrong resolving or fetching source.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// No source has that id.
    #[error("unknown source {0}")]
    UnknownSource(SourceId),

    /// The debuggee could not produce the text.
    ///
    /// Routine rather than exceptional: a script from a reloaded page, or a
    /// resource evicted from the memory cache, is simply gone.
    #[error("source {id} is no longer available: {reason}")]
    Unavailable { id: SourceId, reason: String },

    /// The content was binary and there is nothing to show.
    #[error("source {0} is not text")]
    NotText(SourceId),

    /// Talking to the debuggee failed.
    #[error(transparent)]
    Session(#[from] mjx_wk_session::SessionError),
}
