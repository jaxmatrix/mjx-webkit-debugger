//! Source text, and finding lines in it quickly.
//!
//! **Owned by `docs/tasks/T-005-source-text-store.md`.**
//!
//! The hard case is not a 200-line module — it is a 5 MB minified bundle on one
//! line, or a 200 000-line vendor file the user scrolls through. The virtualised
//! code view asks "what is on line 48 217?" sixty times a second, so that lookup
//! must be O(1) and must not allocate.

use crate::SourceId;

/// Byte offsets of every line start, so a line number becomes a slice.
///
/// Built once when text arrives, on the session thread, never on the UI thread.
#[derive(Debug, Clone, Default)]
pub struct LineIndex {
    _private: (),
}

impl LineIndex {
    /// Index a string. O(n) once; every later lookup is O(1).
    ///
    /// Must handle `\n`, `\r\n`, and a final line with no terminator, and must
    /// count a trailing newline as ending the last line rather than starting an
    /// empty one — off by one here shows up as a phantom line in every file.
    pub fn build(_text: &str) -> Self {
        todo!("T-005")
    }

    /// How many lines the text has.
    pub fn line_count(&self) -> u32 {
        todo!("T-005")
    }

    /// The byte range of one line, excluding its terminator.
    pub fn line_range(&self, _line: u32) -> Option<std::ops::Range<usize>> {
        todo!("T-005")
    }

    /// Which line a byte offset falls on. Binary search.
    pub fn line_of_offset(&self, _offset: usize) -> u32 {
        todo!("T-005")
    }

    /// Convert a protocol line/column into a byte offset.
    ///
    /// Columns are UTF-16 code units on the wire — JavaScript's string model —
    /// and byte offsets here. Conflating them puts breakpoints on the wrong
    /// character in any file containing an emoji or a non-Latin identifier.
    pub fn offset_of(&self, _text: &str, _line: u32, _utf16_column: u32) -> Option<usize> {
        todo!("T-005")
    }
}

/// Immutable text plus its line index.
///
/// Cloning is cheap: the text is shared, not copied.
#[derive(Debug, Clone)]
pub struct SourceText {
    _private: (),
}

impl SourceText {
    /// Take ownership of some text and index it.
    pub fn new(_id: SourceId, _text: String) -> Self {
        todo!("T-005")
    }

    /// Which source this is.
    pub fn id(&self) -> SourceId {
        todo!("T-005")
    }

    /// The whole text.
    pub fn as_str(&self) -> &str {
        todo!("T-005")
    }

    /// One line, without its terminator.
    pub fn line(&self, _line: u32) -> Option<&str> {
        todo!("T-005")
    }

    /// The line index.
    pub fn index(&self) -> &LineIndex {
        todo!("T-005")
    }

    /// Whether this looks like generated or minified output.
    ///
    /// Used to offer pretty-printing without being asked. The signal is mean
    /// line length, not file size: a 5 MB file of ordinary code is not
    /// minified, and a 40 kB bundle on two lines is.
    pub fn looks_minified(&self) -> bool {
        todo!("T-005")
    }
}
