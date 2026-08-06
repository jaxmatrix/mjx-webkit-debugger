//! Syntax highlighting.
//!
//! **Owned by `docs/tasks/T-006-syntax-highlighting.md`.**
//!
//! Tree-sitter, incremental, and computed **only for the visible window plus a
//! small margin**. Highlighting a 5 MB bundle up front would cost seconds and
//! be thrown away as soon as the user scrolled.

use std::collections::HashMap;
use std::ops::Range;

use tree_sitter::{Language, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::{SourceId, SourceKind, SourceText};

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

/// Lines of context parsed beyond the visible window so a comment or string
/// that started just above still colours correctly.
const PARSE_MARGIN_LINES: u32 = 32;

/// Bundled grammar + its highlight query.
struct Grammar {
    language: Language,
    query: Query,
}

/// Which bundled grammar a [`SourceKind`] maps to, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GrammarId {
    Javascript,
    Css,
    Html,
}

/// Cached parse + spans for one source.
struct SourceCache {
    /// Identity of the text last seen — length plus a cheap content fingerprint.
    text_fp: TextFingerprint,
    kind: SourceKind,
    line_starts: Vec<usize>,
    /// Last included byte range the tree was built for.
    included: Option<Range<usize>>,
    tree: Option<Tree>,
    /// Last line window whose spans we still hold.
    spans_lines: Option<Range<u32>>,
    spans: Vec<Vec<HighlightSpan>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextFingerprint {
    len: usize,
    hash: u64,
}

impl TextFingerprint {
    fn of(text: &str) -> Self {
        // Sample head/mid/tail so a 5 MB bundle is not hashed end-to-end on every frame.
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut hash = len as u64;
        let chunk = 4096.min(len);
        if chunk > 0 {
            hash_bytes(&mut hash, &bytes[..chunk]);
            if len > chunk {
                let mid = len / 2;
                let start = mid.saturating_sub(chunk / 2);
                hash_bytes(&mut hash, &bytes[start..start + chunk.min(len - start)]);
                hash_bytes(&mut hash, &bytes[len - chunk..]);
            }
        }
        Self { len, hash }
    }
}

fn hash_bytes(state: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *state = state
            .wrapping_mul(0x0100_0000_01b3)
            .wrapping_add(u64::from(b));
    }
}

/// A tree-sitter highlighter for JavaScript, CSS, and HTML.
pub struct TreeSitterHighlighter {
    js: Grammar,
    css: Grammar,
    html: Grammar,
    parser: Parser,
    cursor: QueryCursor,
    sources: HashMap<SourceId, SourceCache>,
    /// Kind overrides for the [`Highlighter::spans`] path (no kind on [`SourceText`]).
    kinds: HashMap<SourceId, SourceKind>,
}

impl std::fmt::Debug for TreeSitterHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterHighlighter")
            .field("cached_sources", &self.sources.len())
            .field("kinds", &self.kinds.len())
            .finish_non_exhaustive()
    }
}

impl TreeSitterHighlighter {
    /// Build a highlighter with the bundled grammars.
    pub fn new() -> Self {
        Self {
            js: Grammar::javascript(),
            css: Grammar::css(),
            html: Grammar::html(),
            parser: Parser::new(),
            cursor: QueryCursor::new(),
            sources: HashMap::new(),
            kinds: HashMap::new(),
        }
    }

    /// Remember the language for `id` so [`Highlighter::spans`] can pick a grammar.
    ///
    /// [`SourceText`] does not carry [`SourceKind`]; the session layer sets this
    /// when a source is opened. Without it, [`Self::spans_for`] must be used, or
    /// `spans` falls back to a content sniff.
    pub fn set_kind(&mut self, id: SourceId, kind: SourceKind) {
        if let Some(cache) = self.sources.get_mut(&id)
            && cache.kind != kind
        {
            cache.kind = kind;
            cache.tree = None;
            cache.included = None;
            cache.spans_lines = None;
            cache.spans.clear();
        }
        self.kinds.insert(id, kind);
    }

    /// Highlight a window of `text` for `kind`.
    ///
    /// Byte ranges in the returned spans are **relative to each line**, not the
    /// file. This is the engine behind [`Highlighter::spans`]; tests (and any
    /// caller blocked on a live [`SourceText`]) use it directly.
    pub fn spans_for(
        &mut self,
        id: SourceId,
        text: &str,
        kind: SourceKind,
        lines: Range<u32>,
    ) -> Vec<Vec<HighlightSpan>> {
        self.kinds.insert(id, kind);
        self.highlight_window(id, text, kind, lines)
    }

    fn highlight_window(
        &mut self,
        id: SourceId,
        text: &str,
        kind: SourceKind,
        lines: Range<u32>,
    ) -> Vec<Vec<HighlightSpan>> {
        // Fingerprint is O(1) samples — never scan the whole file before the cache check.
        // A 5 MB `count_lines` here would blow the 2 ms scrolling budget on every frame.
        let fp = TextFingerprint::of(text);

        // Fast path: same source, same text, same caller window → clone cached spans.
        // Compare against the unclamped request so we skip the line-index walk entirely.
        if let Some(cache) = self.sources.get(&id)
            && cache.text_fp == fp
            && cache.kind == kind
            && cache.spans_lines.as_ref() == Some(&lines)
        {
            return cache.spans.clone();
        }

        let line_count = if let Some(cache) = self.sources.get(&id) {
            if cache.text_fp == fp {
                cache.line_starts.len() as u32
            } else {
                count_lines(text)
            }
        } else {
            count_lines(text)
        };
        let start = lines.start.min(line_count);
        let end = lines.end.min(line_count).max(start);
        // We key the span cache on the caller's `lines` (not the clamped range) so the
        // next identical request hits before any O(n) work.
        let requested = lines.clone();

        if start == end {
            return Vec::new();
        }

        let grammar_id = grammar_for(kind);

        let Some(grammar_id) = grammar_id else {
            let plain = vec![Vec::new(); (end - start) as usize];
            self.sources.insert(
                id,
                SourceCache {
                    text_fp: fp,
                    kind,
                    line_starts: line_starts(text),
                    included: None,
                    tree: None,
                    spans_lines: Some(requested),
                    spans: plain.clone(),
                },
            );
            return plain;
        };

        let needs_reindex = self
            .sources
            .get(&id)
            .is_none_or(|c| c.text_fp != fp || c.kind != kind);
        if needs_reindex {
            self.sources.insert(
                id,
                SourceCache {
                    text_fp: fp,
                    kind,
                    line_starts: line_starts(text),
                    included: None,
                    tree: None,
                    spans_lines: None,
                    spans: Vec::new(),
                },
            );
        }

        // Clone the index once per cold fill — the hot path returned above.
        let line_starts = match self.sources.get(&id) {
            Some(c) => c.line_starts.clone(),
            None => return vec![Vec::new(); (end - start) as usize],
        };
        let parse_start = start.saturating_sub(PARSE_MARGIN_LINES);
        let parse_end = end.saturating_add(PARSE_MARGIN_LINES).min(line_count);
        let byte_start = line_byte_start(&line_starts, parse_start);
        let byte_end = line_byte_end(&line_starts, text.len(), parse_end);
        let included = byte_start..byte_end;

        let reuse_tree = self.sources.get(&id).and_then(|c| {
            if c.included.as_ref() == Some(&included) {
                c.tree.clone()
            } else {
                None
            }
        });

        let range = tree_sitter::Range {
            start_byte: included.start,
            end_byte: included.end,
            start_point: Point::new(parse_start as usize, 0),
            end_point: point_at_offset(&line_starts, included.end, parse_end),
        };

        let window_bytes =
            line_byte_start(&line_starts, start)..line_byte_end(&line_starts, text.len(), end);

        // Destructure so parser/cursor/grammar can be borrowed together without aliasing
        // the sources map we still need to update afterward.
        let TreeSitterHighlighter {
            js,
            css,
            html,
            parser,
            cursor,
            sources,
            ..
        } = self;

        let grammar = match grammar_id {
            GrammarId::Javascript => js,
            GrammarId::Css => css,
            GrammarId::Html => html,
        };

        if parser.set_language(&grammar.language).is_err() {
            return vec![Vec::new(); (end - start) as usize];
        }
        if parser.set_included_ranges(&[range]).is_err() {
            return vec![Vec::new(); (end - start) as usize];
        }

        let tree = match parser.parse(text.as_bytes(), reuse_tree.as_ref()) {
            Some(tree) => tree,
            None => return vec![Vec::new(); (end - start) as usize],
        };

        cursor.set_byte_range(window_bytes.clone());

        let mut captures: Vec<(usize, usize, HighlightKind)> = Vec::new();
        {
            let mut iter = cursor.captures(&grammar.query, tree.root_node(), text.as_bytes());
            while let Some((m, cap_idx)) = iter.next() {
                let cap = m.captures[*cap_idx];
                let name = grammar.query.capture_names()[cap.index as usize];
                let Some(hl) = kind_from_capture(name) else {
                    continue;
                };
                let s = cap.node.start_byte();
                let e = cap.node.end_byte();
                if e <= window_bytes.start || s >= window_bytes.end || s >= e {
                    continue;
                }
                captures.push((s.max(window_bytes.start), e.min(window_bytes.end), hl));
            }
        }

        // Document order; later captures for the same range replace earlier ones
        // (specific patterns follow the general identifier catch-alls).
        captures.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let spans = project_to_lines(&captures, &line_starts, text.as_bytes(), start, end);

        if let Some(cache) = sources.get_mut(&id) {
            cache.included = Some(included);
            cache.tree = Some(tree);
            cache.spans_lines = Some(requested);
            cache.spans = spans.clone();
        }

        spans
    }
}

impl Default for TreeSitterHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for TreeSitterHighlighter {
    fn spans(&mut self, source: &SourceText, lines: Range<u32>) -> Vec<Vec<HighlightSpan>> {
        let id = source.id();
        let text = source.as_str();
        let kind = self
            .kinds
            .get(&id)
            .copied()
            .unwrap_or_else(|| sniff_kind(text));
        self.highlight_window(id, text, kind, lines)
    }

    fn invalidate(&mut self, source: SourceId) {
        self.sources.remove(&source);
        self.kinds.remove(&source);
    }
}

impl Grammar {
    fn javascript() -> Self {
        let language: Language = tree_sitter_javascript::LANGUAGE.into();
        // Bundled query strings are compile-time fixtures, not debuggee input.
        #[allow(clippy::expect_used)]
        let query = Query::new(&language, tree_sitter_javascript::HIGHLIGHT_QUERY)
            .expect("bundled JS highlight query");
        Self { language, query }
    }

    fn css() -> Self {
        let language: Language = tree_sitter_css::LANGUAGE.into();
        #[allow(clippy::expect_used)]
        let query = Query::new(&language, tree_sitter_css::HIGHLIGHTS_QUERY)
            .expect("bundled CSS highlight query");
        Self { language, query }
    }

    fn html() -> Self {
        let language: Language = tree_sitter_html::LANGUAGE.into();
        #[allow(clippy::expect_used)]
        let query = Query::new(&language, tree_sitter_html::HIGHLIGHTS_QUERY)
            .expect("bundled HTML highlight query");
        Self { language, query }
    }
}

fn grammar_for(kind: SourceKind) -> Option<GrammarId> {
    match kind {
        SourceKind::Script { .. } => Some(GrammarId::Javascript),
        SourceKind::StyleSheet => Some(GrammarId::Css),
        SourceKind::Document => Some(GrammarId::Html),
        SourceKind::Other => None,
    }
}

fn sniff_kind(text: &str) -> SourceKind {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return SourceKind::Other;
    }
    if trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
        || trimmed.starts_with("<!--")
    {
        return SourceKind::Document;
    }
    // Stylesheets in the wild often start with a rule, `@import`, or `:root`.
    if trimmed.starts_with('@')
        || trimmed.starts_with(":root")
        || trimmed.starts_with("html {")
        || trimmed.starts_with("body {")
    {
        return SourceKind::StyleSheet;
    }
    SourceKind::Script {
        module: false,
        content_script: false,
    }
}

fn kind_from_capture(name: &str) -> Option<HighlightKind> {
    // JS highlights regex as `string.special`.
    if name == "string.special" {
        return Some(HighlightKind::Regex);
    }
    let primary = name.split('.').next().unwrap_or(name);
    match primary {
        "keyword" => Some(HighlightKind::Keyword),
        "string" => Some(HighlightKind::String),
        "number" => Some(HighlightKind::Number),
        "comment" => Some(HighlightKind::Comment),
        "function" | "constructor" => Some(HighlightKind::Function),
        "type" => Some(HighlightKind::Type),
        "variable" => Some(HighlightKind::Variable),
        "property" => Some(HighlightKind::Property),
        "operator" => Some(HighlightKind::Operator),
        "punctuation" | "embedded" => Some(HighlightKind::Punctuation),
        "constant" => Some(HighlightKind::Constant),
        "tag" => Some(HighlightKind::Tag),
        "attribute" => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut n = 1u32;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && i + 1 < bytes.len() {
            n += 1;
        }
    }
    n
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(text.len() / 32 + 1);
    starts.push(0);
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && i + 1 < bytes.len() {
            starts.push(i + 1);
        }
    }
    starts
}

fn line_byte_start(starts: &[usize], line: u32) -> usize {
    starts
        .get(line as usize)
        .copied()
        .unwrap_or_else(|| starts.last().copied().unwrap_or(0))
}

fn line_byte_end(starts: &[usize], text_len: usize, line_end_exclusive: u32) -> usize {
    if line_end_exclusive as usize >= starts.len() {
        text_len
    } else {
        starts[line_end_exclusive as usize]
    }
}

fn point_at_offset(starts: &[usize], offset: usize, approx_row: u32) -> Point {
    let row = if (approx_row as usize) < starts.len() {
        approx_row as usize
    } else {
        starts.len().saturating_sub(1)
    };
    let row_start = starts.get(row).copied().unwrap_or(0);
    Point::new(row, offset.saturating_sub(row_start))
}

/// Byte end of line content, excluding `\n` or `\r\n`.
fn line_content_end(starts: &[usize], text: &[u8], line: u32) -> usize {
    let start = line_byte_start(starts, line);
    let next = if (line as usize + 1) < starts.len() {
        starts[line as usize + 1]
    } else {
        text.len()
    };
    if next == start {
        return start;
    }
    // next points at the first byte of the following line (after `\n`).
    let mut end = next;
    if end > start && text.get(end - 1) == Some(&b'\n') {
        end -= 1;
        if end > start && text.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
    }
    end
}

/// Project file-absolute captures into per-line, line-relative spans.
fn project_to_lines(
    captures: &[(usize, usize, HighlightKind)],
    line_starts: &[usize],
    text: &[u8],
    start_line: u32,
    end_line: u32,
) -> Vec<Vec<HighlightSpan>> {
    let n = (end_line - start_line) as usize;
    let mut out = vec![Vec::new(); n];

    for &(cap_start, cap_end, kind) in captures {
        let first = line_of_offset(line_starts, cap_start).max(start_line);
        let last_byte = cap_end.saturating_sub(1);
        let last = line_of_offset(line_starts, last_byte).min(end_line.saturating_sub(1));
        if first >= end_line || last < start_line {
            continue;
        }
        for line in first..=last {
            let ls = line_byte_start(line_starts, line);
            let le = line_content_end(line_starts, text, line);
            let s = cap_start.max(ls);
            let e = cap_end.min(le);
            if s < e {
                let rel = (s - ls) as u32..(e - ls) as u32;
                let idx = (line - start_line) as usize;
                push_span(&mut out[idx], HighlightSpan { range: rel, kind });
            }
        }
    }

    out
}

fn push_span(line_spans: &mut Vec<HighlightSpan>, span: HighlightSpan) {
    if span.range.start >= span.range.end {
        return;
    }
    if let Some(last) = line_spans.last_mut() {
        if last.range.start == span.range.start && last.range.end == span.range.end {
            *last = span;
            return;
        }
        if last.range.start < span.range.end && span.range.start < last.range.end {
            if span.range.start <= last.range.start && span.range.end >= last.range.end {
                *last = span;
                return;
            }
            if last.range.start < span.range.start && last.range.end > span.range.start {
                last.range.end = span.range.start;
                if last.range.start >= last.range.end {
                    line_spans.pop();
                }
            }
        }
    }
    line_spans.push(span);
}

fn line_of_offset(starts: &[usize], offset: usize) -> u32 {
    match starts.binary_search(&offset) {
        Ok(i) => i as u32,
        Err(i) => i.saturating_sub(1) as u32,
    }
}
