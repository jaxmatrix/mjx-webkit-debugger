//! Fetching and caching source text.
//!
//! **Owned by `docs/tasks/T-011-source-store.md`.**
//!
//! Two protocol paths, chosen by [`crate::SourceKind`]:
//!
//! - scripts → `Debugger.getScriptSource { scriptId }`
//! - documents and stylesheets → `Page.getResourceContent { frameId, url }`,
//!   which may return base64 and must be decoded
//!
//! Both replies can be megabytes. Neither may be awaited on the UI thread.

use std::sync::Arc;

use crate::{SourceError, SourceId, SourceText};

/// An LRU cache over fetched source text.
#[derive(Debug)]
pub struct SourceStore {
    _private: (),
}

impl SourceStore {
    /// A store with a byte budget.
    ///
    /// Bounded by total bytes rather than entry count, because entry sizes here
    /// differ by four orders of magnitude.
    pub fn new(_budget_bytes: usize) -> Self {
        todo!("T-011")
    }

    /// Text for a source, fetching it if it is not cached.
    ///
    /// Concurrent calls for the same id must share one request. The source tree
    /// and the editor routinely ask at the same moment, and fetching a 5 MB
    /// bundle twice is a visible stall.
    pub async fn text(
        &self,
        _session: &mjx_wk_session::SessionHandle,
        _id: SourceId,
    ) -> Result<Arc<SourceText>, SourceError> {
        todo!("T-011")
    }

    /// Cached text, if present. Never blocks — safe from the UI thread.
    pub fn cached(&self, _id: SourceId) -> Option<Arc<SourceText>> {
        todo!("T-011")
    }

    /// Drop everything.
    ///
    /// Called on navigation: script ids are reissued, so stale text would be
    /// served under a new script's id.
    pub fn clear(&self) {
        todo!("T-011")
    }

    /// Bytes currently held.
    pub fn bytes_held(&self) -> usize {
        todo!("T-011")
    }
}
