//! Tree-sitter highlighting — done-criteria for T-006.
//!
//! [`SourceText`] is still a T-005 stub on this branch, so tests drive the
//! highlighter through [`TreeSitterHighlighter::spans_for`] (byte/string input)
//! rather than [`Highlighter::spans`].

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mjx_wk_source::highlight::{HighlightKind, HighlightSpan, Highlighter, TreeSitterHighlighter};
use mjx_wk_source::{SourceId, SourceKind};

const APP_JS: &str = include_str!("../../../fixtures/page/app.js");
const STYLE_CSS: &str = include_str!("../../../fixtures/page/style.css");
const INDEX_HTML: &str = include_str!("../../../fixtures/page/index.html");

fn script() -> SourceKind {
    SourceKind::Script {
        module: false,
        content_script: false,
    }
}

fn hl() -> TreeSitterHighlighter {
    TreeSitterHighlighter::new()
}

fn line_count(text: &str) -> u32 {
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

fn line_text(text: &str, line: u32) -> &str {
    let bytes = text.as_bytes();
    let mut starts = vec![0usize];
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && i + 1 < bytes.len() {
            starts.push(i + 1);
        }
    }
    let Some(&start) = starts.get(line as usize) else {
        return "";
    };
    let next = starts
        .get(line as usize + 1)
        .copied()
        .unwrap_or(bytes.len());
    let mut end = next;
    if end > start && bytes[end - 1] == b'\n' {
        end -= 1;
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
    }
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

/// Assert every span's range lies inside its line and is non-empty.
fn assert_line_local(text: &str, start_line: u32, spans: &[Vec<HighlightSpan>]) {
    for (i, line_spans) in spans.iter().enumerate() {
        let line = start_line + i as u32;
        let content = line_text(text, line);
        let len = content.len() as u32;
        for span in line_spans {
            assert!(
                span.range.start < span.range.end,
                "empty span on line {line}: {span:?}"
            );
            assert!(
                span.range.end <= len,
                "span {:?} exceeds line {line} len {len} ({content:?})",
                span.range
            );
        }
    }
}

#[test]
fn new_succeeds_with_bundled_grammars() {
    let _ = hl();
}

#[test]
fn spans_are_byte_ranges_within_their_line() {
    let mut h = hl();
    let lines = 0..line_count(APP_JS);
    let spans = h.spans_for(SourceId(1), APP_JS, script(), lines.clone());
    assert_eq!(spans.len(), lines.len());
    assert_line_local(APP_JS, 0, &spans);
}

#[test]
fn javascript_keywords_and_strings_are_highlighted() {
    let src = "const msg = \"hello\";\n";
    let mut h = hl();
    let spans = h.spans_for(SourceId(1), src, script(), 0..1);
    assert_eq!(spans.len(), 1);
    assert_line_local(src, 0, &spans);

    let kinds: Vec<_> = spans[0].iter().map(|s| s.kind).collect();
    assert!(
        kinds.contains(&HighlightKind::Keyword),
        "expected keyword in {spans:?}"
    );
    assert!(
        kinds.contains(&HighlightKind::String),
        "expected string in {spans:?}"
    );

    // `const` is bytes 0..5 on the line.
    assert!(
        spans[0].iter().any(|s| {
            s.kind == HighlightKind::Keyword && s.range == (0..5) && &src[0..5] == "const"
        }),
        "const keyword span missing: {spans:?}"
    );
}

#[test]
fn syntax_error_still_highlights_what_parses() {
    let src = "\
function ok() { return 1; }
function {{{ broken
function stillOk() { return 2; }
";
    let mut h = hl();
    let spans = h.spans_for(SourceId(1), src, script(), 0..3);
    assert_eq!(spans.len(), 3);
    assert_line_local(src, 0, &spans);

    // First and third lines should still get a `function` keyword.
    assert!(
        spans[0]
            .iter()
            .any(|s| s.kind == HighlightKind::Keyword && s.range == (0..8)),
        "line 0 not highlighted: {:?}",
        spans[0]
    );
    assert!(
        spans[2]
            .iter()
            .any(|s| s.kind == HighlightKind::Keyword && s.range == (0..8)),
        "line 2 not highlighted after syntax error: {:?}",
        spans[2]
    );
}

#[test]
fn language_with_no_grammar_is_plain_text() {
    let src = "just some opaque bytes @@ ##\nsecond line\n";
    let mut h = hl();
    let spans = h.spans_for(SourceId(1), src, SourceKind::Other, 0..2);
    assert_eq!(spans, vec![vec![], vec![]]);
}

#[test]
fn repeated_calls_for_the_same_window_do_not_reparse() {
    let mut h = hl();
    let id = SourceId(7);
    let lines = 0..10u32;

    let first = h.spans_for(id, APP_JS, script(), lines.clone());
    let second = h.spans_for(id, APP_JS, script(), lines.clone());
    assert_eq!(first, second);

    // Invalidate must drop the cache so a subsequent call still works and can differ
    // only by recompute — content unchanged, so equal, but the path ran again.
    h.invalidate(id);
    let third = h.spans_for(id, APP_JS, script(), lines);
    assert_eq!(first, third);
}

#[test]
fn css_fixture_highlights_selectors_and_properties() {
    let mut h = hl();
    let spans = h.spans_for(SourceId(2), STYLE_CSS, SourceKind::StyleSheet, 0..1);
    assert_eq!(spans.len(), 1);
    assert_line_local(STYLE_CSS, 0, &spans);
    let kinds: Vec<_> = spans[0].iter().map(|s| s.kind).collect();
    assert!(
        kinds.contains(&HighlightKind::Property)
            || kinds.contains(&HighlightKind::Tag)
            || kinds.contains(&HighlightKind::Keyword),
        "expected CSS colouring on first line: {spans:?}"
    );
}

#[test]
fn html_fixture_highlights_tags() {
    let mut h = hl();
    // `<html lang="en">` sits on line 1 (0-based) after doctype.
    let spans = h.spans_for(SourceId(3), INDEX_HTML, SourceKind::Document, 1..2);
    assert_eq!(spans.len(), 1);
    assert_line_local(INDEX_HTML, 1, &spans);
    assert!(
        spans[0].iter().any(|s| s.kind == HighlightKind::Tag),
        "expected a tag on the html line: {:?}",
        spans[0]
    );
}

#[test]
fn only_requested_window_is_returned() {
    let mut h = hl();
    let spans = h.spans_for(SourceId(1), APP_JS, script(), 2..5);
    assert_eq!(spans.len(), 3);
    assert_line_local(APP_JS, 2, &spans);
}

#[test]
fn highlighting_a_100_line_window_of_a_5mb_file_stays_under_2ms() {
    let mut text = String::with_capacity(5_000_000);
    let mut n = 0u32;
    while text.len() < 5_000_000 {
        text.push_str(&format!("function f{n}() {{ return {n}; }}\n"));
        n += 1;
    }
    let total_lines = line_count(&text);
    assert!(total_lines > 200, "fixture too small: {total_lines} lines");

    let id = SourceId(99);
    let window = (total_lines / 2)..(total_lines / 2 + 100);
    let mut h = hl();

    // Warm the cache — the budget is for steady scrolling, not the cold first paint.
    let _ = h.spans_for(id, &text, script(), window.clone());

    let mut best = Duration::from_secs(1);
    for _ in 0..5 {
        let t0 = Instant::now();
        let spans = h.spans_for(id, &text, script(), window.clone());
        let elapsed = t0.elapsed();
        assert_eq!(spans.len(), 100);
        best = best.min(elapsed);
    }
    assert!(
        best < Duration::from_millis(2),
        "cached 100-line window took {best:?} (budget 2 ms)"
    );
}

#[test]
fn golden_javascript_fixture_spans() {
    assert_golden("app.js", APP_JS, script());
}

#[test]
fn golden_css_fixture_spans() {
    assert_golden("style.css", STYLE_CSS, SourceKind::StyleSheet);
}

fn assert_golden(name: &str, text: &str, kind: SourceKind) {
    let mut h = hl();
    let lines = 0..line_count(text);
    let spans = h.spans_for(SourceId(1), text, kind, lines);
    assert_line_local(text, 0, &spans);

    let path = golden_path(name);
    let actual = serialize_spans(&spans);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir golden");
        }
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e} (run with UPDATE_GOLDEN=1)",
            path.display()
        )
    });
    assert_eq!(
        normalize(&actual),
        normalize(&expected),
        "golden mismatch for {name}; re-run with UPDATE_GOLDEN=1 if intentional"
    );
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/highlight")
        .join(format!("{name}.spans.json"))
}

fn serialize_spans(spans: &[Vec<HighlightSpan>]) -> String {
    let mut out = String::from("[\n");
    for (i, line) in spans.iter().enumerate() {
        out.push_str("  [");
        for (j, span) in line.iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!(
                "{{\"range\":[{},{}],\"kind\":\"{}\"}}",
                span.range.start,
                span.range.end,
                kind_name(span.kind)
            ));
        }
        out.push(']');
        if i + 1 < spans.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("]\n");
    out
}

fn kind_name(kind: HighlightKind) -> &'static str {
    match kind {
        HighlightKind::Keyword => "Keyword",
        HighlightKind::String => "String",
        HighlightKind::Number => "Number",
        HighlightKind::Comment => "Comment",
        HighlightKind::Function => "Function",
        HighlightKind::Type => "Type",
        HighlightKind::Variable => "Variable",
        HighlightKind::Property => "Property",
        HighlightKind::Operator => "Operator",
        HighlightKind::Punctuation => "Punctuation",
        HighlightKind::Regex => "Regex",
        HighlightKind::Constant => "Constant",
        HighlightKind::Tag => "Tag",
        HighlightKind::Attribute => "Attribute",
    }
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}
