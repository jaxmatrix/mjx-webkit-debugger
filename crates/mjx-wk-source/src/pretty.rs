//! Reformatting minified source.
//!
//! **Owned by `docs/tasks/T-007-pretty-printer.md`.**
//!
//! Chrome calls this the `{}` button. The output is only half the job: **every
//! position must be mappable both ways**, or a breakpoint set on pretty-printed
//! line 40 is sent to the debuggee as line 40 of a file that has one line.

use crate::{SourceId, SourceLocation, SourceText};

/// Pretty-printed text plus the mapping back to the original.
#[derive(Debug)]
pub struct PrettyPrinted {
    _private: (),
}

impl PrettyPrinted {
    /// The reformatted text.
    pub fn text(&self) -> &SourceText {
        todo!("T-007")
    }

    /// Pretty position → original position. Used when setting a breakpoint.
    pub fn to_original(&self, _location: SourceLocation) -> SourceLocation {
        todo!("T-007")
    }

    /// Original position → pretty position. Used when showing where execution
    /// paused.
    pub fn to_pretty(&self, _location: SourceLocation) -> SourceLocation {
        todo!("T-007")
    }
}

/// Reformats minified JavaScript and CSS.
#[derive(Debug, Default)]
pub struct PrettyPrinter {
    _private: (),
}

impl PrettyPrinter {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Reformat a source, building the position map as it goes.
    ///
    /// Must not reorder or drop anything: this is a formatter, not a
    /// transformer. Must preserve string and template-literal contents exactly,
    /// including newlines inside them.
    pub fn format(&self, _id: SourceId, _text: &SourceText) -> PrettyPrinted {
        todo!("T-007")
    }
}
