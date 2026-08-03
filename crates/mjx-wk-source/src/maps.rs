//! Source maps.
//!
//! **Phase 2 — owned by `docs/tasks/T-205-source-maps.md`.**
//!
//! The seam exists in Phase 1 so the code view and breakpoint model can be
//! written against [`SourceLocation`] indirection from the start. Retrofitting
//! a mapping step under a UI that assumed generated positions means touching
//! every panel.

use crate::{SourceError, SourceId, SourceLocation};

/// Resolves between generated and original positions.
#[derive(Debug, Default)]
pub struct SourceMapResolver {
    _private: (),
}

impl SourceMapResolver {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Load the map named by a script's `sourceMapURL`.
    ///
    /// The URL may be a `data:` URI with the whole map inline, a relative URL
    /// to resolve against the script, or absent. A map that fails to load is
    /// not an error the user should see — the generated source is still
    /// perfectly debuggable.
    pub async fn load(
        &mut self,
        _session: &mjx_wk_session::SessionHandle,
        _generated: SourceId,
        _source_map_url: &str,
    ) -> Result<(), SourceError> {
        todo!("T-205")
    }

    /// Generated position → authored position.
    pub fn to_original(&self, _location: SourceLocation) -> Option<SourceLocation> {
        todo!("T-205")
    }

    /// Authored position → generated positions.
    ///
    /// Returns several: one authored line can be inlined in many places, and a
    /// breakpoint on it must be set at each.
    pub fn to_generated(&self, _location: SourceLocation) -> Vec<SourceLocation> {
        todo!("T-205")
    }

    /// Whether a source has a usable map.
    pub fn has_map(&self, _generated: SourceId) -> bool {
        todo!("T-205")
    }
}
