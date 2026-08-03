//! What sources exist, and how they are grouped for display.
//!
//! **Owned by `docs/tasks/T-004-source-inventory.md`.**
//!
//! Merges two feeds that arrive at different times and in different shapes:
//!
//! | Feed | Carries | Arrives |
//! |---|---|---|
//! | `Debugger.scriptParsed` | scripts, by `scriptId` | streamed, continuously |
//! | `Page.getResourceTree` | documents, stylesheets, images, by URL | once, on request |
//!
//! A script and a resource can describe the same file. They must appear once.

use crate::{FrameId, SourceId, SourceKind};

/// One known source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry {
    pub id: SourceId,
    /// The debuggee's `scriptId`, when this came from `scriptParsed`. Needed to
    /// fetch text, and invalid after a reload.
    pub script_id: Option<String>,
    /// The frame this belongs to, needed by `Page.getResourceContent`.
    pub frame: Option<FrameId>,
    pub url: String,
    pub kind: SourceKind,
    /// From the `sourceMappingURL` comment or the `scriptParsed` field. May be
    /// a `data:` URI carrying the whole map inline.
    pub source_map_url: Option<String>,
    /// True for a source reconstructed from a source map rather than served.
    pub is_original: bool,
}

impl SourceEntry {
    /// The name to show in a file tree — the last path segment, or something
    /// sensible for the many sources that have no useful URL.
    ///
    /// `eval` and injected scripts arrive with an empty URL, and a tree full of
    /// blank rows is useless.
    pub fn display_name(&self) -> String {
        todo!("T-004")
    }
}

/// The file tree, grouped by origin then by path.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceTreeNode {
    /// An origin (`https://example.com`) or a synthetic group.
    Group {
        label: String,
        children: Vec<SourceTreeNode>,
    },
    /// A source.
    Leaf { id: SourceId, label: String },
}

/// Every source the debuggee has told us about.
#[derive(Debug, Default)]
pub struct SourceInventory {
    _private: (),
}

impl SourceInventory {
    /// An empty inventory.
    pub fn new() -> Self {
        todo!("T-004")
    }

    /// Record a `Debugger.scriptParsed`.
    ///
    /// Returns the id, which is stable across reloads for a given URL: the user
    /// keeps their editor tab and their breakpoints when the page reloads.
    pub fn on_script_parsed(
        &mut self,
        _event: &mjx_wk_protocol::generated::debugger::events::ScriptParsed,
    ) -> SourceId {
        todo!("T-004")
    }

    /// Record a `Page.getResourceTree` reply.
    ///
    /// Must not duplicate a source already known from `scriptParsed` — the
    /// resource tree lists the page's own scripts too.
    pub fn on_resource_tree(
        &mut self,
        _tree: &mjx_wk_protocol::generated::page::FrameResourceTree,
    ) {
        todo!("T-004")
    }

    /// Look up by the debuggee's script id.
    pub fn by_script_id(&self, _script_id: &str) -> Option<SourceId> {
        todo!("T-004")
    }

    /// Look up by URL.
    pub fn by_url(&self, _url: &str) -> Option<SourceId> {
        todo!("T-004")
    }

    /// One entry.
    pub fn get(&self, _id: SourceId) -> Option<&SourceEntry> {
        todo!("T-004")
    }

    /// Every entry, in discovery order.
    pub fn entries(&self) -> &[SourceEntry] {
        todo!("T-004")
    }

    /// The display tree.
    ///
    /// Rebuilt when the inventory changes, not per frame.
    pub fn tree(&self) -> SourceTreeNode {
        todo!("T-004")
    }

    /// Handle a navigation.
    ///
    /// Script ids are invalidated but entries are kept, so an open editor tab
    /// and its breakpoints survive a reload. Keeping stale script ids would
    /// make the next `getScriptSource` fail with a confusing error.
    pub fn on_navigated(&mut self) {
        todo!("T-004")
    }
}
