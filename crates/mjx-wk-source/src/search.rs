//! Searching across sources.
//!
//! **Owned by `docs/tasks/T-012-search.md`.**
//!
//! Two strategies, and the choice matters. `Page.searchInResources` searches
//! everything the debuggee knows without transferring it — the right answer for
//! a large site. Local search over the cache is instant and works offline, and
//! is the right answer for what the user already has open. Do both: local
//! first for immediate feedback, remote to fill in the rest.

use crate::{SourceId, SourceLocation};

/// What to look for.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub is_regex: bool,
    /// Restrict to one source; `None` searches everything.
    pub within: Option<SourceId>,
}

/// One match.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub location: SourceLocation,
    /// The whole line, for display. Truncated for minified sources, where the
    /// "line" can be megabytes.
    pub line_text: String,
    /// Byte range of the match within `line_text`.
    pub match_range: std::ops::Range<u32>,
}

/// Runs searches over local and remote sources.
#[derive(Debug, Default)]
pub struct SearchIndex {
    _private: (),
}

impl SearchIndex {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Search cached sources. Synchronous and immediate.
    pub fn search_local(&self, _query: &SearchQuery) -> Vec<SearchHit> {
        todo!("T-012")
    }

    /// Search everything the debuggee has, via `Page.searchInResources`.
    pub async fn search_remote(
        &self,
        _session: &mjx_wk_session::SessionHandle,
        _query: &SearchQuery,
    ) -> Result<Vec<SearchHit>, crate::SourceError> {
        todo!("T-012")
    }
}
