//! Syntax highlighting.
//!
//! **Owned by `docs/tasks/T-006-syntax-highlighting.md`.**
//!
//! Tree-sitter, incremental, and computed **only for the visible window plus a
//! small margin**. Highlighting a 5 MB bundle up front would cost seconds and
//! be thrown away as soon as the user scrolled.

use std::ops::Range;

use crate::{SourceId, SourceText};

/// What a span is, semantically. Mapped to colours by the theme, so the
/// highlighter never knows about colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Type,
    Variable,
    Property,
    Operator,
    Punctuation,
    Regex,
    Constant,
    /// Markup tag or CSS selector.
    Tag,
    Attribute,
}

/// A highlighted run of bytes within one line.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightSpan {
    /// Byte range within the line, not the file.
    pub range: Range<u32>,
    pub kind: HighlightKind,
}

/// Produces highlight spans for a window of lines.
pub trait Highlighter: Send + std::fmt::Debug {
    /// Spans for `lines`, one `Vec` per line.
    ///
    /// Called every frame for the visible window, so it must be cached: parse
    /// once, re-highlight only what changed. Must degrade to plain text rather
    /// than failing — a syntax error mid-edit is normal, and a file whose
    /// language has no grammar must still be readable.
    fn spans(&mut self, source: &SourceText, lines: Range<u32>) -> Vec<Vec<HighlightSpan>>;

    /// Drop cached state for a source.
    fn invalidate(&mut self, source: SourceId);
}

/// A tree-sitter highlighter for JavaScript, CSS, and HTML.
#[derive(Debug)]
pub struct TreeSitterHighlighter {
    _private: (),
}

impl TreeSitterHighlighter {
    /// Build a highlighter with the bundled grammars.
    pub fn new() -> Self {
        todo!("T-006")
    }
}

impl Default for TreeSitterHighlighter {
    fn default() -> Self {
        Self::new()
    }
}
