//! Source text, and finding lines in it quickly.
//!
//! **Owned by `docs/tasks/T-005-source-text.md`.**
//!
//! The hard case is not a 200-line module — it is a 5 MB minified bundle on one
//! line, or a 200 000-line vendor file the user scrolls through. The virtualised
//! code view asks "what is on line 48 217?" sixty times a second, so that lookup
//! must be O(1) and must not allocate.

use std::ops::Range;
use std::sync::Arc;

use crate::SourceId;

/// Mean line length (bytes) at or above which we treat the file as minified.
///
/// Chosen so ordinary multi-megabyte sources stay un-minified while a short
/// two-line bundle still trips the heuristic — size alone is the wrong signal.
const MINIFIED_MEAN_LINE_BYTES: usize = 200;

/// Byte offsets of every line start, so a line number becomes a slice.
///
/// Built once when text arrives, on the session thread, never on the UI thread.
#[derive(Debug, Clone, Default)]
pub struct LineIndex {
    /// Byte offset of the first character of each line.
    starts: Vec<usize>,
    /// Exclusive byte offset of each line's content (terminator excluded).
    ///
    /// Parallel to `starts`. Storing ends avoids re-examining the text on every
    /// `line_range` call — the UI asks for this sixty times a second.
    ends: Vec<usize>,
}

impl LineIndex {
    /// Index a string. O(n) once; every later lookup is O(1).
    ///
    /// Must handle `\n`, `\r\n`, and a final line with no terminator, and must
    /// count a trailing newline as ending the last line rather than starting an
    /// empty one — off by one here shows up as a phantom line in every file.
    pub fn build(text: &str) -> Self {
        let bytes = text.as_bytes();
        // Reserve from a typical line length so a 5 MB minified file (few lines)
        // and a 200k-line vendor file both avoid pathological realloc churn
        // without a second scan.
        let mut starts = Vec::with_capacity(1 + bytes.len() / 32);
        let mut ends = Vec::with_capacity(1 + bytes.len() / 32);

        let mut line_start = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    starts.push(line_start);
                    ends.push(i);
                    i += 1;
                    line_start = i;
                }
                b'\r' => {
                    starts.push(line_start);
                    ends.push(i);
                    i += 1;
                    if i < bytes.len() && bytes[i] == b'\n' {
                        i += 1;
                    }
                    line_start = i;
                }
                _ => i += 1,
            }
        }

        // Final line with no terminator — but not a phantom line after a
        // trailing newline (`line_start == len` means the terminator already
        // closed the last real line).
        if line_start < bytes.len() || starts.is_empty() {
            starts.push(line_start);
            ends.push(bytes.len());
        }

        Self { starts, ends }
    }

    /// How many lines the text has.
    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    /// The byte range of one line, excluding its terminator.
    pub fn line_range(&self, line: u32) -> Option<Range<usize>> {
        let i = line as usize;
        let start = *self.starts.get(i)?;
        let end = *self.ends.get(i)?;
        Some(start..end)
    }

    /// Which line a byte offset falls on. Binary search.
    pub fn line_of_offset(&self, offset: usize) -> u32 {
        if self.starts.is_empty() {
            return 0;
        }
        match self.starts.binary_search(&offset) {
            Ok(i) => i as u32,
            Err(0) => 0,
            Err(i) => (i - 1) as u32,
        }
    }

    /// Convert a protocol line/column into a byte offset.
    ///
    /// Columns are UTF-16 code units on the wire — JavaScript's string model —
    /// and byte offsets here. Conflating them puts breakpoints on the wrong
    /// character in any file containing an emoji or a non-Latin identifier.
    pub fn offset_of(&self, text: &str, line: u32, utf16_column: u32) -> Option<usize> {
        let range = self.line_range(line)?;
        let line_text = text.get(range.clone())?;

        let mut col = 0u32;
        for (byte_idx, ch) in line_text.char_indices() {
            if col == utf16_column {
                return Some(range.start + byte_idx);
            }
            let width = ch.len_utf16() as u32;
            // A column that lands inside a surrogate pair is not a valid
            // protocol position; refuse rather than invent a byte offset.
            if col + width > utf16_column {
                return None;
            }
            col += width;
        }
        if col == utf16_column {
            Some(range.end)
        } else {
            None
        }
    }
}

/// Immutable text plus its line index.
///
/// Cloning is cheap: the text is shared, not copied.
#[derive(Debug, Clone)]
pub struct SourceText {
    id: SourceId,
    text: Arc<str>,
    index: LineIndex,
}

impl SourceText {
    /// Take ownership of some text and index it.
    pub fn new(id: SourceId, text: String) -> Self {
        let index = LineIndex::build(&text);
        Self {
            id,
            text: Arc::from(text),
            index,
        }
    }

    /// Which source this is.
    pub fn id(&self) -> SourceId {
        self.id
    }

    /// The whole text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// One line, without its terminator.
    pub fn line(&self, line: u32) -> Option<&str> {
        let range = self.index.line_range(line)?;
        self.text.get(range)
    }

    /// The line index.
    pub fn index(&self) -> &LineIndex {
        &self.index
    }

    /// Whether this looks like generated or minified output.
    ///
    /// Used to offer pretty-printing without being asked. The signal is mean
    /// line length, not file size: a 5 MB file of ordinary code is not
    /// minified, and a 40 kB bundle on two lines is.
    pub fn looks_minified(&self) -> bool {
        let lines = self.index.line_count() as usize;
        if lines == 0 {
            return false;
        }
        self.text.len() / lines >= MINIFIED_MEAN_LINE_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(text: &str) -> Vec<&str> {
        let index = LineIndex::build(text);
        (0..index.line_count())
            .map(|i| {
                let range = index.line_range(i).expect("line in range");
                &text[range]
            })
            .collect()
    }

    #[test]
    fn lf_crlf_and_unterminated_final_line() {
        assert_eq!(lines_of("a\nb\nc"), vec!["a", "b", "c"]);
        assert_eq!(lines_of("a\r\nb\r\nc"), vec!["a", "b", "c"]);
        assert_eq!(lines_of("solo"), vec!["solo"]);
        assert_eq!(lines_of(""), vec![""]);
    }

    #[test]
    fn trailing_newline_does_not_create_phantom_line() {
        assert_eq!(lines_of("a\nb\n"), vec!["a", "b"]);
        assert_eq!(lines_of("a\r\nb\r\n"), vec!["a", "b"]);
        assert_eq!(lines_of("\n"), vec![""]);
        assert_eq!(lines_of("\r\n"), vec![""]);
        assert_eq!(lines_of("\n\n"), vec!["", ""]);
    }

    #[test]
    fn line_of_offset_binary_searches_starts() {
        let text = "ab\ncde\nf";
        let index = LineIndex::build(text);
        assert_eq!(index.line_of_offset(0), 0);
        assert_eq!(index.line_of_offset(2), 0); // '\n'
        assert_eq!(index.line_of_offset(3), 1); // 'c'
        assert_eq!(index.line_of_offset(7), 2); // 'f'
    }

    #[test]
    fn offset_of_counts_utf16_columns_for_emoji() {
        // "ok😀x" — grinning face is one scalar, two UTF-16 code units, four UTF-8 bytes.
        let text = "ok😀x";
        let index = LineIndex::build(text);
        assert_eq!(index.offset_of(text, 0, 0), Some(0));
        assert_eq!(index.offset_of(text, 0, 2), Some(2)); // start of emoji
        assert_eq!(index.offset_of(text, 0, 3), None); // inside surrogate pair
        assert_eq!(index.offset_of(text, 0, 4), Some(6)); // 'x'
        assert_eq!(index.offset_of(text, 0, 5), Some(7)); // end of line
    }

    #[test]
    fn offset_of_counts_utf16_columns_for_non_latin() {
        // Each hiragana is one UTF-16 code unit and three UTF-8 bytes.
        let text = "変数 = 1";
        let index = LineIndex::build(text);
        assert_eq!(index.offset_of(text, 0, 0), Some(0));
        assert_eq!(index.offset_of(text, 0, 1), Some(3)); // second char
        assert_eq!(index.offset_of(text, 0, 2), Some(6)); // space after identifier
        assert_eq!(&text[index.offset_of(text, 0, 2).unwrap()..], " = 1");
    }

    #[test]
    fn source_text_shares_and_exposes_lines() {
        let src = SourceText::new(SourceId(7), "one\ntwo\n".into());
        assert_eq!(src.id(), SourceId(7));
        assert_eq!(src.line(0), Some("one"));
        assert_eq!(src.line(1), Some("two"));
        assert_eq!(src.line(2), None);
        assert_eq!(src.index().line_count(), 2);
        let clone = src.clone();
        // Clone must share the text allocation — the UI holds many snapshots.
        assert_eq!(src.as_str().as_ptr(), clone.as_str().as_ptr());
    }

    #[test]
    fn looks_minified_keys_on_mean_line_length_not_size() {
        // ~5 MB of ordinary short lines must not look minified.
        let ordinary = "fn f() {}\n".repeat(500_000);
        assert!(ordinary.len() > 4_000_000);
        let ordinary_src = SourceText::new(SourceId(1), ordinary);
        assert!(!ordinary_src.looks_minified());

        // A 40 kB bundle on two lines must look minified.
        let chunk = "x".repeat(20_000);
        let tiny_bundle = format!("{chunk}\n{chunk}");
        assert!(tiny_bundle.len() < 50_000);
        let mini = SourceText::new(SourceId(2), tiny_bundle);
        assert!(mini.looks_minified());
    }

    #[test]
    fn indexes_large_bundle_fixture_script_source() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/large-bundle.jsonl"
        );
        let raw = std::fs::read_to_string(path).expect("large-bundle fixture");
        let script = extract_script_source(&raw).expect("scriptSource in fixture");
        assert!(script.len() > 1_000_000, "fixture should be multi-MB");

        let src = SourceText::new(SourceId(1), script);
        assert!(src.index().line_count() > 1);
        let mid = src.index().line_count() / 2;
        let line = src.line(mid).expect("mid line");
        assert!(!line.is_empty() || mid == 0);
        // Random-access must be O(1) and allocation-free beyond the &str.
        let range = src.index().line_range(mid).unwrap();
        assert_eq!(&src.as_str()[range], src.line(mid).unwrap());
    }

    fn extract_script_source(jsonl: &str) -> Option<String> {
        for line in jsonl.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(message) = v.pointer("/frame/params/message").and_then(|m| m.as_str()) else {
                continue;
            };
            let Ok(inner) = serde_json::from_str::<serde_json::Value>(message) else {
                continue;
            };
            if let Some(src) = inner
                .pointer("/result/scriptSource")
                .and_then(|s| s.as_str())
            {
                return Some(src.to_owned());
            }
        }
        None
    }
}
