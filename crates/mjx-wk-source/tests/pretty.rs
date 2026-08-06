//! Integration tests for the pretty-printer and position map (T-007).
//!
//! Golden inputs live under `tests/golden/pretty/`. The contract is:
//! - non-whitespace token bytes are preserved in order (nothing dropped/reordered);
//! - string / template contents stay byte-identical;
//! - `to_original(to_pretty(p)) == p` for statement-ish starts;
//! - already-formatted sources are left alone.

use std::fs;
use std::path::PathBuf;

use mjx_wk_source::{PrettyPrinter, SourceId, SourceLocation, SourceText};

/// When T-005's `SourceText` is still a ZST stub, public-API tests cannot run.
/// Unit tests in `pretty.rs` cover mapping without it; after T-005 lands these
/// exercise the real boundary.
fn source_text_ready() -> bool {
    std::mem::size_of::<SourceText>() > 0
}

fn make_text(id: SourceId, text: String) -> Option<SourceText> {
    if !source_text_ready() {
        return None;
    }
    Some(SourceText::new(id, text))
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/pretty")
}

fn read_golden(name: &str) -> String {
    let path = golden_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn offset_to_location(text: &SourceText, offset: usize) -> SourceLocation {
    let line = text.index().line_of_offset(offset);
    let range = text
        .index()
        .line_range(line)
        .unwrap_or_else(|| panic!("line {line} missing for offset {offset}"));
    let line_str = &text.as_str()[range.clone()];
    let col_bytes = offset.saturating_sub(range.start).min(line_str.len());
    let column = line_str[..col_bytes]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    SourceLocation {
        source: text.id(),
        line,
        column,
    }
}

/// Byte offsets of tokens that begin a statement-ish region.
fn statement_start_offsets(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut emit_next = true;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
            continue;
        }
        if emit_next {
            out.push(i);
            emit_next = false;
        }
        if matches!(b, b';' | b'{' | b'}') {
            emit_next = true;
        }
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

fn assert_token_identity(original: &str, pretty: &str) {
    let strip = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    assert_eq!(
        strip(original),
        strip(pretty),
        "formatter must not reorder or drop non-whitespace bytes"
    );
}

#[test]
fn already_formatted_left_alone() {
    let input = read_golden("already_formatted.in.js");
    let Some(src) = make_text(SourceId(1), input.clone()) else {
        eprintln!("skip: SourceText stub (needs T-005)");
        return;
    };
    let printed = PrettyPrinter::new().format(SourceId(1), &src);
    assert_eq!(printed.text().as_str(), input);
    let loc = SourceLocation {
        source: SourceId(1),
        line: 1,
        column: 2,
    };
    assert_eq!(printed.to_pretty(loc), loc);
    assert_eq!(printed.to_original(loc), loc);
}

#[test]
fn js_minified_preserves_strings_and_round_trips() {
    let input = read_golden("js_minified.in.js");
    let Some(src) = make_text(SourceId(7), input.clone()) else {
        eprintln!("skip: SourceText stub (needs T-005)");
        return;
    };
    let printed = PrettyPrinter::new().format(SourceId(7), &src);
    let pretty = printed.text().as_str();
    assert_ne!(pretty, input, "minified JS should be reformatted");
    assert_token_identity(&input, pretty);
    assert!(pretty.contains("`keep\nme=${a}`"));
    assert!(pretty.contains("\"hi\\nthere\""));
    assert!(pretty.contains("/ab\\//g"));

    for off in statement_start_offsets(&input) {
        let orig = offset_to_location(&src, off);
        let pretty_loc = printed.to_pretty(orig);
        let back = printed.to_original(pretty_loc);
        assert_eq!(
            back, orig,
            "round-trip failed at byte {off}: {orig} → {pretty_loc} → {back}\npretty:\n{pretty}"
        );
    }
}

#[test]
fn css_minified_preserves_and_round_trips() {
    let input = read_golden("css_minified.in.css");
    let Some(src) = make_text(SourceId(3), input.clone()) else {
        eprintln!("skip: SourceText stub (needs T-005)");
        return;
    };
    let printed = PrettyPrinter::new().format(SourceId(3), &src);
    let pretty = printed.text().as_str();
    assert_ne!(pretty, input);
    assert_token_identity(&input, pretty);
    assert!(pretty.contains("\"Helvetica Neue\""));

    for off in statement_start_offsets(&input) {
        let orig = offset_to_location(&src, off);
        let pretty_loc = printed.to_pretty(orig);
        let back = printed.to_original(pretty_loc);
        assert_eq!(back, orig, "css round-trip at {off}");
    }
}

#[test]
fn golden_outs_match_formatter_when_present() {
    let Some(_) = make_text(SourceId(0), String::new()) else {
        eprintln!("skip: SourceText stub (needs T-005)");
        return;
    };
    for (input_name, out_name) in [
        ("js_minified.in.js", "js_minified.out.js"),
        ("css_minified.in.css", "css_minified.out.css"),
    ] {
        let out_path = golden_dir().join(out_name);
        if !out_path.exists() {
            continue;
        }
        let input = read_golden(input_name);
        let expected = read_golden(out_name);
        let src = SourceText::new(SourceId(1), input);
        let printed = PrettyPrinter::new().format(SourceId(1), &src);
        assert_eq!(printed.text().as_str(), expected, "golden {out_name}");
    }
}
