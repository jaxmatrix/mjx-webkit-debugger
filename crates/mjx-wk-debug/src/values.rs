//! Inspecting values in a paused frame.
//!
//! **Owned by `docs/tasks/T-203-variable-tree.md`.**
//!
//! # Two constraints shape this entirely
//!
//! **Expansion is lazy.** A global scope has thousands of properties and any
//! object may have millions. Nothing is fetched until a row is opened.
//!
//! **`Runtime.getProperties` is paginated on WebKit** — `fetchStart` and
//! `fetchCount`, which CDP does not have. Ignoring that and asking for
//! everything is how a debugger hangs on a large array.

use std::ops::Range;

/// A node in the variable tree, local to this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueNodeId(pub u32);

/// A short rendering of a value, shown without expanding it.
///
/// Comes from the protocol's `generatePreview`, which is why previews are free:
/// the debuggee builds them while answering.
#[derive(Debug, Clone, PartialEq)]
pub struct ValuePreview {
    /// e.g. `"object"`, `"string"`, `"function"`.
    pub type_name: String,
    /// e.g. `"array"`, `"null"`, `"regexp"`.
    pub subtype: Option<String>,
    /// What to render: `"Array(3)"`, `"\"hello\""`, `"undefined"`.
    pub description: String,
    /// Whether opening this would show anything.
    pub has_children: bool,
}

/// One row.
#[derive(Debug, Clone)]
pub struct ValueNode {
    pub id: ValueNodeId,
    pub name: String,
    pub preview: ValuePreview,
    /// The remote handle, for expanding. **Invalid after resume.**
    pub object_id: Option<String>,
    /// `None` until expanded.
    pub children: Option<Vec<ValueNodeId>>,
    /// Which slice of the properties has been fetched so far.
    pub fetched: Range<u32>,
    /// A getter that has not been invoked. Shown as `(...)`, because invoking
    /// it could have side effects and the user must opt in.
    pub is_accessor: bool,
    /// An own property rather than an inherited one.
    pub is_own: bool,
}

/// A lazily expanded tree of values.
#[derive(Debug, Clone, Default)]
pub struct ValueTree {
    _private: (),
}

impl ValueTree {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// One node.
    pub fn get(&self, _id: ValueNodeId) -> Option<&ValueNode> {
        todo!("T-203")
    }

    /// The top-level rows.
    pub fn roots(&self) -> &[ValueNodeId] {
        todo!("T-203")
    }

    /// Whether a node needs fetching before it can be shown open.
    pub fn needs_fetch(&self, _id: ValueNodeId) -> bool {
        todo!("T-203")
    }

    /// How many more properties exist beyond what has been fetched.
    ///
    /// Drives the "Show more" row an object with 50 000 keys needs.
    pub fn remaining(&self, _id: ValueNodeId) -> Option<u32> {
        todo!("T-203")
    }
}
