//! Reformatting minified source.
//!
//! **Owned by `docs/tasks/T-007-pretty-printer.md`.**
//!
//! Chrome calls this the `{}` button. The output is only half the job: **every
//! position must be mappable both ways**, or a breakpoint set on pretty-printed
//! line 40 is sent to the debuggee as line 40 of a file that has one line.
//!
//! # Design (Plan-Optimization)
//!
//! - Copy every non-whitespace token **verbatim** — strings, templates, regex
//!   literals, comments — so nothing is reordered, dropped, or reinterpreted.
//! - Insert only whitespace. Mapping points are recorded when inserted
//!   whitespace makes original and pretty offsets diverge (Chrome's sparse
//!   `FormattedContentBuilder` scheme).
//! - Own a tiny line-start index here so mapping does not depend on T-005's
//!   `LineIndex`. `SourceText` is only used at the public boundary.
//! - Columns are UTF-16 code units, matching the wire / [`SourceLocation`].

use crate::{SourceId, SourceLocation, SourceText};

/// Pretty-printed text plus the mapping back to the original.
#[derive(Debug)]
pub struct PrettyPrinted {
    text: SourceText,
    source: SourceId,
    /// Original input bytes (needed for UTF-16 column conversion on the way back).
    original: String,
    /// Line starts (byte offsets) for the original input.
    original_lines: Vec<usize>,
    /// Line starts (byte offsets) for the pretty output.
    pretty_lines: Vec<usize>,
    /// Parallel original/pretty UTF-8 byte offsets. Monotonic; always starts at 0.
    map_original: Vec<usize>,
    map_pretty: Vec<usize>,
}

impl PrettyPrinted {
    /// The reformatted text.
    pub fn text(&self) -> &SourceText {
        &self.text
    }

    /// Pretty position → original position. Used when setting a breakpoint.
    pub fn to_original(&self, location: SourceLocation) -> SourceLocation {
        let pretty_off = location_to_offset(
            self.text.as_str(),
            &self.pretty_lines,
            location.line,
            location.column,
        );
        let orig_off = map_pretty_to_original(&self.map_original, &self.map_pretty, pretty_off);
        let (line, column) = offset_to_location(&self.original_lines, &self.original, orig_off);
        SourceLocation {
            source: self.source,
            line,
            column,
        }
    }

    /// Original position → pretty position. Used when showing where execution
    /// paused.
    pub fn to_pretty(&self, location: SourceLocation) -> SourceLocation {
        let orig_off = location_to_offset(
            &self.original,
            &self.original_lines,
            location.line,
            location.column,
        );
        let pretty_off = map_original_to_pretty(&self.map_original, &self.map_pretty, orig_off);
        let (line, column) = offset_to_location(&self.pretty_lines, self.text.as_str(), pretty_off);
        SourceLocation {
            source: self.source,
            line,
            column,
        }
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
    pub fn format(&self, id: SourceId, text: &SourceText) -> PrettyPrinted {
        let input = text.as_str();
        let result = format_source(input);
        PrettyPrinted {
            text: SourceText::new(id, result.formatted),
            source: id,
            original: result.original,
            original_lines: result.original_lines,
            pretty_lines: result.pretty_lines,
            map_original: result.map_original,
            map_pretty: result.map_pretty,
        }
    }
}

// ---------------------------------------------------------------------------
// Format result (string-level; unit-tested without `SourceText`)
// ---------------------------------------------------------------------------

struct FormatResult {
    original: String,
    formatted: String,
    original_lines: Vec<usize>,
    pretty_lines: Vec<usize>,
    map_original: Vec<usize>,
    map_pretty: Vec<usize>,
}

fn format_source(input: &str) -> FormatResult {
    let original_lines = build_line_starts(input);
    if !looks_minified(input, &original_lines) {
        return FormatResult {
            original: input.to_owned(),
            formatted: input.to_owned(),
            original_lines: original_lines.clone(),
            pretty_lines: original_lines,
            map_original: vec![0],
            map_pretty: vec![0],
        };
    }

    let mut builder = ContentBuilder::new();
    let lang = detect_lang(input);
    emit_pretty(input, lang, &mut builder);
    let formatted = builder.content;
    let pretty_lines = build_line_starts(&formatted);
    FormatResult {
        original: input.to_owned(),
        formatted,
        original_lines,
        pretty_lines,
        map_original: builder.map_original,
        map_pretty: builder.map_pretty,
    }
}

// ---------------------------------------------------------------------------
// Minified heuristic & language sniff (no `SourceKind` on the seam)
// ---------------------------------------------------------------------------

/// Mean line length above this → offer / apply pretty-print.
const MINIFIED_MEAN_LINE_LEN: usize = 120;

fn looks_minified(text: &str, lines: &[usize]) -> bool {
    if text.is_empty() {
        return false;
    }
    let line_count = lines.len().max(1);
    let mean = text.len() / line_count;
    if mean >= MINIFIED_MEAN_LINE_LEN {
        return true;
    }
    // Short one-liners (or two-liners) that still look packed: several
    // statement/brace markers and almost no line breaks. Keeps tiny golden
    // fixtures exercising the formatter without waiting for a 5 MB bundle.
    if line_count <= 2 && text.len() >= 24 {
        let markers = text
            .bytes()
            .filter(|b| matches!(b, b';' | b'{' | b'}'))
            .count();
        if markers >= 2 {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    JavaScript,
    Css,
}

fn detect_lang(text: &str) -> Lang {
    // Cheap sniff: CSS-looking constructs without JS keywords → CSS.
    let head: String = text.chars().take(512).collect();
    let lower = head.to_ascii_lowercase();
    let js_signal = [
        "function",
        "=>",
        "var ",
        "let ",
        "const ",
        "return ",
        "typeof ",
        "new ",
        "class ",
        "import ",
        "export ",
        "async ",
        "await ",
    ]
    .iter()
    .any(|s| lower.contains(s));
    if js_signal {
        return Lang::JavaScript;
    }
    let css_signal = lower.contains("@media")
        || lower.contains("@keyframes")
        || lower.contains("@import")
        || (lower.contains('{') && lower.contains(':') && lower.contains(';'));
    if css_signal {
        Lang::Css
    } else {
        Lang::JavaScript
    }
}

// ---------------------------------------------------------------------------
// Line index + UTF-16 columns (local copy; does not touch T-005)
// ---------------------------------------------------------------------------

fn build_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                i += 1;
                if i < bytes.len() {
                    starts.push(i);
                }
            }
            b'\r' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    starts.push(i);
                }
            }
            _ => i += 1,
        }
    }
    starts
}

fn line_of_offset(lines: &[usize], offset: usize) -> u32 {
    match lines.binary_search(&offset) {
        Ok(i) => i as u32,
        Err(i) => i.saturating_sub(1) as u32,
    }
}

fn line_byte_range<'a>(text: &'a str, lines: &[usize], line: u32) -> Option<&'a str> {
    let i = line as usize;
    if i >= lines.len() {
        return None;
    }
    let start = lines[i];
    let end = if i + 1 < lines.len() {
        // Exclude the terminator that precedes the next line start.
        let mut e = lines[i + 1];
        let b = text.as_bytes();
        if e > start && b[e - 1] == b'\n' {
            e -= 1;
            if e > start && b[e - 1] == b'\r' {
                e -= 1;
            }
        } else if e > start && b[e - 1] == b'\r' {
            e -= 1;
        }
        e
    } else {
        text.len()
    };
    Some(&text[start..end])
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| if c.len_utf16() == 2 { 2 } else { 1 }).sum()
}

fn byte_index_at_utf16_column(line: &str, utf16_column: u32) -> usize {
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if units >= utf16_column {
            return byte_idx;
        }
        units += if ch.len_utf16() == 2 { 2 } else { 1 };
    }
    line.len()
}

fn utf16_column_at_byte(line: &str, byte_in_line: usize) -> u32 {
    let byte_in_line = byte_in_line.min(line.len());
    utf16_len(&line[..byte_in_line])
}

fn location_to_offset(text: &str, lines: &[usize], line: u32, utf16_column: u32) -> usize {
    let Some(line_str) = line_byte_range(text, lines, line) else {
        return text.len();
    };
    let start = lines[line as usize];
    start + byte_index_at_utf16_column(line_str, utf16_column)
}

fn offset_to_location(lines: &[usize], text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let line = line_of_offset(lines, offset);
    let start = lines[line as usize];
    let line_str = line_byte_range(text, lines, line).unwrap_or("");
    let col_bytes = offset.saturating_sub(start).min(line_str.len());
    let column = utf16_column_at_byte(line_str, col_bytes);
    (line, column)
}

// ---------------------------------------------------------------------------
// Bidirectional offset map (Chrome FormattedContentBuilder)
// ---------------------------------------------------------------------------

fn map_original_to_pretty(map_original: &[usize], map_pretty: &[usize], orig_off: usize) -> usize {
    debug_assert_eq!(map_original.len(), map_pretty.len());
    let i = match map_original.binary_search(&orig_off) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let orig_base = map_original[i];
    let pretty_base = map_pretty[i];
    let delta = orig_off - orig_base;
    let candidate = pretty_base + delta;
    if i + 1 < map_pretty.len() {
        candidate.min(map_pretty[i + 1])
    } else {
        candidate
    }
}

fn map_pretty_to_original(map_original: &[usize], map_pretty: &[usize], pretty_off: usize) -> usize {
    debug_assert_eq!(map_original.len(), map_pretty.len());
    let i = match map_pretty.binary_search(&pretty_off) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let orig_base = map_original[i];
    let pretty_base = map_pretty[i];
    let delta = pretty_off.saturating_sub(pretty_base);

    if i + 1 < map_original.len() {
        let orig_span = map_original[i + 1] - map_original[i];
        // 1:1 region covers `orig_span` bytes after this mapping point. Anything
        // further is inserted pretty whitespace before the next mapped token.
        if delta <= orig_span {
            (orig_base + delta).min(map_original[i + 1])
        } else {
            // Inside padding — snap to the next original token start so a
            // breakpoint on a pretty blank/indent line still lands on code.
            map_original[i + 1]
        }
    } else {
        orig_base + delta
    }
}

// ---------------------------------------------------------------------------
// Content builder
// ---------------------------------------------------------------------------

struct ContentBuilder {
    content: String,
    map_original: Vec<usize>,
    map_pretty: Vec<usize>,
    last_original: usize,
    last_pretty: usize,
    nesting: u32,
    new_lines: u32,
    soft_space: bool,
    hard_spaces: u32,
}

impl ContentBuilder {
    fn new() -> Self {
        Self {
            content: String::new(),
            map_original: vec![0],
            map_pretty: vec![0],
            last_original: 0,
            last_pretty: 0,
            nesting: 0,
            new_lines: 0,
            soft_space: false,
            hard_spaces: 0,
        }
    }

    fn add_token(&mut self, token: &str, original_offset: usize) {
        self.flush_formatting();
        self.add_mapping_if_needed(original_offset);
        self.content.push_str(token);
    }

    fn add_soft_space(&mut self) {
        if self.hard_spaces == 0 {
            self.soft_space = true;
        }
    }

    fn add_new_line(&mut self) {
        if self.content.is_empty() && self.new_lines == 0 && !self.soft_space && self.hard_spaces == 0
        {
            // Avoid leading newlines, matching Chrome.
            return;
        }
        self.new_lines = self.new_lines.max(1);
    }

    fn increase_nesting(&mut self) {
        self.nesting += 1;
    }

    fn decrease_nesting(&mut self) {
        self.nesting = self.nesting.saturating_sub(1);
    }

    fn flush_formatting(&mut self) {
        if self.new_lines > 0 {
            for _ in 0..self.new_lines {
                self.content.push('\n');
            }
            for _ in 0..self.nesting {
                self.content.push_str("  ");
            }
        } else if self.soft_space {
            self.content.push(' ');
        }
        for _ in 0..self.hard_spaces {
            self.content.push(' ');
        }
        self.new_lines = 0;
        self.soft_space = false;
        self.hard_spaces = 0;
    }

    fn add_mapping_if_needed(&mut self, original_position: usize) {
        let pretty_position = self.content.len();
        if original_position.wrapping_sub(self.last_original)
            == pretty_position.wrapping_sub(self.last_pretty)
        {
            return;
        }
        self.map_original.push(original_position);
        self.last_original = original_position;
        self.map_pretty.push(pretty_position);
        self.last_pretty = pretty_position;
    }
}

// ---------------------------------------------------------------------------
// Lexer — tokens copied verbatim; whitespace skipped
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Ident,
    Number,
    String,
    Template,
    Regex,
    Comment,
    Punct,
    Keyword,
}

struct Token<'a> {
    text: &'a str,
    start: usize,
    kind: TokenKind,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c == '$' || c == '\\' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn keyword_kind(text: &str) -> TokenKind {
    match text {
        "break" | "case" | "catch" | "class" | "const" | "continue" | "debugger" | "default"
        | "delete" | "do" | "else" | "export" | "extends" | "finally" | "for" | "function"
        | "if" | "import" | "in" | "instanceof" | "let" | "new" | "return" | "super" | "switch"
        | "this" | "throw" | "try" | "typeof" | "var" | "void" | "while" | "with" | "yield"
        | "await" | "async" | "of" | "static" | "get" | "set" | "from" | "as" => TokenKind::Keyword,
        _ => TokenKind::Ident,
    }
}

fn lex_string(input: &str, start: usize, quote: char) -> usize {
    let mut chars = input[start..].char_indices();
    chars.next(); // consume opening quote
    while let Some((rel, c)) = chars.next() {
        if c == '\\' {
            // Skip escaped char (including escaped newline sequences).
            let _ = chars.next();
            continue;
        }
        if c == quote {
            return start + rel + c.len_utf8();
        }
    }
    input.len()
}

/// Lex a template literal starting at `` ` ``, including nested `${ ... }`.
fn lex_template(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'`' => return i + 1,
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                i += 2;
                i = lex_template_expression(input, i);
            }
            _ => {
                // Advance one UTF-8 char.
                i += utf8_char_len(bytes, i);
            }
        }
    }
    input.len()
}

fn lex_template_expression(input: &str, mut i: usize) -> usize {
    let bytes = input.as_bytes();
    let mut depth = 1i32;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' | b'"' => {
                let quote = if c == b'\'' { '\'' } else { '"' };
                i = lex_string(input, i, quote);
            }
            b'`' => i = lex_template(input, i),
            b'/' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    i = skip_line_comment(bytes, i);
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    i = skip_block_comment(bytes, i);
                } else {
                    i += 1;
                }
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += utf8_char_len(bytes, i),
        }
    }
    input.len()
}

fn utf8_char_len(bytes: &[u8], i: usize) -> usize {
    match bytes[i] {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
    .min(bytes.len() - i)
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}

fn looks_like_regex(prev: Option<&Token<'_>>) -> bool {
    match prev.map(|t| (t.kind, t.text)) {
        None => true,
        Some((TokenKind::Ident | TokenKind::Keyword | TokenKind::Number | TokenKind::String, _)) => {
            false
        }
        Some((TokenKind::Template, _)) => false,
        Some((TokenKind::Regex, _)) => false,
        Some((TokenKind::Punct, t)) => !matches!(t, ")" | "]" | "}"),
        Some((TokenKind::Comment, _)) => true,
    }
}

fn lex_regex(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'[' if !in_class => {
                in_class = true;
                i += 1;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
            }
            b'/' if !in_class => {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                    i += 1;
                }
                return i;
            }
            b'\n' | b'\r' => return i, // invalid; stop
            _ => i += utf8_char_len(bytes, i),
        }
    }
    input.len()
}

fn next_token<'a>(input: &'a str, mut pos: usize, prev: Option<&Token<'a>>) -> Option<Token<'a>> {
    let bytes = input.as_bytes();
    // Skip whitespace.
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c') {
        pos += 1;
    }
    if pos >= bytes.len() {
        return None;
    }

    let start = pos;
    let c = input[pos..].chars().next()?;

    // Comments
    if c == '/' && pos + 1 < bytes.len() {
        if bytes[pos + 1] == b'/' {
            let end = skip_line_comment(bytes, pos);
            return Some(Token {
                text: &input[start..end],
                start,
                kind: TokenKind::Comment,
            });
        }
        if bytes[pos + 1] == b'*' {
            let end = skip_block_comment(bytes, pos);
            return Some(Token {
                text: &input[start..end],
                start,
                kind: TokenKind::Comment,
            });
        }
        if looks_like_regex(prev) {
            let end = lex_regex(input, pos);
            return Some(Token {
                text: &input[start..end],
                start,
                kind: TokenKind::Regex,
            });
        }
    }

    // Strings
    if c == '\'' || c == '"' {
        let end = lex_string(input, pos, c);
        return Some(Token {
            text: &input[start..end],
            start,
            kind: TokenKind::String,
        });
    }

    // Template
    if c == '`' {
        let end = lex_template(input, pos);
        return Some(Token {
            text: &input[start..end],
            start,
            kind: TokenKind::Template,
        });
    }

    // Number
    if c.is_ascii_digit() || (c == '.' && pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit()) {
        let end = lex_number(input, pos);
        return Some(Token {
            text: &input[start..end],
            start,
            kind: TokenKind::Number,
        });
    }

    // Identifier / keyword / private name / CSS hash
    if c == '#' {
        let mut end = pos + c.len_utf8();
        while end < input.len() {
            let ch = input[end..].chars().next()?;
            if ch.is_ascii_hexdigit() || is_ident_continue(ch) || ch == '-' {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        return Some(Token {
            text: &input[start..end],
            start,
            kind: TokenKind::Ident,
        });
    }

    if is_ident_start(c) {
        let mut end = pos + c.len_utf8();
        while end < input.len() {
            let ch = input[end..].chars().next()?;
            if is_ident_continue(ch) {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        let text = &input[start..end];
        return Some(Token {
            text,
            start,
            kind: keyword_kind(text),
        });
    }

    // Multi-char punctuators first
    let rest = &input[pos..];
    for p in [
        ">>>=", "===", "!==", ">>>", ">>=", "<<=", "**=", "&&", "||", "??", "==", "!=", "<=", ">=",
        "<<", ">>", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "=>", "**", "++", "--", "?.",
        "...",
    ] {
        if rest.starts_with(p) {
            return Some(Token {
                text: &input[start..start + p.len()],
                start,
                kind: TokenKind::Punct,
            });
        }
    }

    // Single punctuator
    let end = pos + c.len_utf8();
    Some(Token {
        text: &input[start..end],
        start,
        kind: TokenKind::Punct,
    })
}

fn lex_number(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut i = start;

    // 0x / 0b / 0o prefixes.
    if i + 1 < bytes.len()
        && bytes[i] == b'0'
        && matches!(bytes[i + 1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
    {
        i += 2;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch.is_ascii_hexdigit() || ch == '_' {
                i += 1;
            } else {
                break;
            }
        }
        return i;
    }

    let mut seen_dot = false;
    let mut seen_exp = false;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch.is_ascii_digit() || ch == '_' {
            i += 1;
        } else if ch == '.' && !seen_dot && !seen_exp {
            seen_dot = true;
            i += 1;
        } else if (ch == 'e' || ch == 'E') && !seen_exp {
            seen_exp = true;
            i += 1;
            if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
                i += 1;
            }
        } else {
            break;
        }
    }
    i
}

fn tokenize<'a>(input: &'a str) -> Vec<Token<'a>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while let Some(tok) = next_token(input, pos, tokens.last()) {
        pos = tok.start + tok.text.len();
        tokens.push(tok);
    }
    tokens
}

// ---------------------------------------------------------------------------
// Emit pretty output from tokens
// ---------------------------------------------------------------------------

fn emit_pretty(input: &str, lang: Lang, builder: &mut ContentBuilder) {
    let tokens = tokenize(input);
    match lang {
        Lang::Css => emit_css(&tokens, builder),
        Lang::JavaScript => emit_js(&tokens, builder),
    }
}

fn emit_js(tokens: &[Token<'_>], builder: &mut ContentBuilder) {
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    for (idx, tok) in tokens.iter().enumerate() {
        let next = tokens.get(idx + 1);
        match tok.kind {
            TokenKind::Comment => {
                builder.add_token(tok.text, tok.start);
                builder.add_new_line();
                continue;
            }
            TokenKind::Punct => match tok.text {
                "{" => {
                    builder.add_soft_space();
                    builder.add_token(tok.text, tok.start);
                    builder.add_new_line();
                    builder.increase_nesting();
                }
                "}" => {
                    builder.decrease_nesting();
                    builder.add_new_line();
                    builder.add_token(tok.text, tok.start);
                    let followed_by_else_catch_while = next.is_some_and(|n| {
                        matches!(n.text, "else" | "catch" | "finally" | "while")
                    });
                    if followed_by_else_catch_while {
                        builder.add_soft_space();
                    } else if next.is_some_and(|n| n.text == ",") {
                        // object / destructure
                    } else if next.is_some_and(|n| n.text == ";") {
                        // fall through; semicolon handler adds newline
                    } else if next.is_some_and(|n| n.text == ")") {
                        // do-while / export
                    } else {
                        builder.add_new_line();
                    }
                }
                ";" => {
                    builder.add_token(tok.text, tok.start);
                    if paren_depth == 0 {
                        builder.add_new_line();
                    } else {
                        builder.add_soft_space();
                    }
                }
                "," => {
                    builder.add_token(tok.text, tok.start);
                    if paren_depth == 0 && bracket_depth == 0 {
                        // Keep object/array property lists readable when nested.
                        if builder.nesting > 0 {
                            builder.add_new_line();
                        } else {
                            builder.add_soft_space();
                        }
                    } else {
                        builder.add_soft_space();
                    }
                }
                "(" => {
                    paren_depth += 1;
                    // Space after keyword before '('.
                    if idx > 0 && tokens[idx - 1].kind == TokenKind::Keyword {
                        builder.add_soft_space();
                    }
                    builder.add_token(tok.text, tok.start);
                }
                ")" => {
                    paren_depth = paren_depth.saturating_sub(1);
                    builder.add_token(tok.text, tok.start);
                    if next.is_some_and(|n| n.text == "{") {
                        builder.add_soft_space();
                    }
                }
                "[" => {
                    bracket_depth += 1;
                    builder.add_token(tok.text, tok.start);
                }
                "]" => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    builder.add_token(tok.text, tok.start);
                }
                ":" => {
                    builder.add_token(tok.text, tok.start);
                    builder.add_soft_space();
                }
                "?" => {
                    builder.add_soft_space();
                    builder.add_token(tok.text, tok.start);
                    builder.add_soft_space();
                }
                "=>" | "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "==" | "===" | "!=" | "!=="
                | "<" | ">" | "<=" | ">=" | "+" | "-" | "*" | "/" | "%" | "&&" | "||" | "??" => {
                    // Binary / assignment operators get spaces on both sides.
                    builder.add_soft_space();
                    builder.add_token(tok.text, tok.start);
                    builder.add_soft_space();
                }
                "!" | "~" | "++" | "--" | "..." | "." | "?." => {
                    builder.add_token(tok.text, tok.start);
                }
                _ => {
                    builder.add_token(tok.text, tok.start);
                }
            },
            TokenKind::Keyword => {
                // Space before keyword if previous was an ident/number/string close.
                if idx > 0 {
                    let prev = &tokens[idx - 1];
                    let needs_space = (matches!(
                        prev.kind,
                        TokenKind::Ident
                            | TokenKind::Keyword
                            | TokenKind::Number
                            | TokenKind::String
                            | TokenKind::Template
                            | TokenKind::Regex
                    ) || matches!(prev.text, ")" | "]" | "}"))
                        && prev.text != "."
                        && prev.text != "?.";
                    if needs_space {
                        builder.add_soft_space();
                    }
                }
                builder.add_token(tok.text, tok.start);
                // Space after keyword before ident / string / number / punct that needs it.
                if next.is_some_and(|n| {
                    matches!(
                        n.kind,
                        TokenKind::Ident
                            | TokenKind::Keyword
                            | TokenKind::Number
                            | TokenKind::String
                            | TokenKind::Template
                    ) || n.text == "{"
                        || n.text == "*"
                }) {
                    builder.add_soft_space();
                }
            }
            TokenKind::Ident | TokenKind::Number | TokenKind::String | TokenKind::Template
            | TokenKind::Regex => {
                if idx > 0 {
                    let prev = &tokens[idx - 1];
                    if matches!(
                        prev.kind,
                        TokenKind::Ident
                            | TokenKind::Keyword
                            | TokenKind::Number
                            | TokenKind::String
                            | TokenKind::Template
                            | TokenKind::Regex
                    ) {
                        builder.add_soft_space();
                    }
                }
                builder.add_token(tok.text, tok.start);
            }
        }
    }
    // Trailing newline for non-empty pretty output.
    if !builder.content.is_empty() && !builder.content.ends_with('\n') {
        builder.add_new_line();
        builder.flush_formatting();
    }
}

fn emit_css(tokens: &[Token<'_>], builder: &mut ContentBuilder) {
    let mut in_value = false;
    for (idx, tok) in tokens.iter().enumerate() {
        let next = tokens.get(idx + 1);
        match tok.text {
            "{" => {
                builder.add_soft_space();
                builder.add_token(tok.text, tok.start);
                builder.add_new_line();
                builder.increase_nesting();
                in_value = false;
            }
            "}" => {
                builder.decrease_nesting();
                builder.add_new_line();
                builder.add_token(tok.text, tok.start);
                builder.add_new_line();
                in_value = false;
            }
            ":" => {
                builder.add_token(tok.text, tok.start);
                // Property values get a space after `:`. Pseudo-classes in
                // selectors (`a:hover`) must stay glued — those appear at
                // nesting 0 (or after `}` before the next rule's `{`).
                let space = match next {
                    Some(n) if builder.nesting > 0 => {
                        matches!(
                            n.kind,
                            TokenKind::Ident
                                | TokenKind::Keyword
                                | TokenKind::Number
                                | TokenKind::String
                        ) || n.text.starts_with('#')
                    }
                    _ => false,
                };
                if space {
                    builder.add_soft_space();
                    in_value = true;
                } else {
                    in_value = false;
                }
            }
            ";" => {
                builder.add_token(tok.text, tok.start);
                builder.add_new_line();
                in_value = false;
            }
            "," => {
                builder.add_token(tok.text, tok.start);
                if in_value {
                    builder.add_soft_space();
                } else {
                    builder.add_new_line();
                }
            }
            _ => {
                if tok.kind == TokenKind::Comment {
                    builder.add_token(tok.text, tok.start);
                    builder.add_new_line();
                    continue;
                }
                if idx > 0 {
                    let prev = &tokens[idx - 1];
                    if matches!(
                        prev.kind,
                        TokenKind::Ident
                            | TokenKind::Keyword
                            | TokenKind::Number
                            | TokenKind::String
                    ) && matches!(
                        tok.kind,
                        TokenKind::Ident
                            | TokenKind::Keyword
                            | TokenKind::Number
                            | TokenKind::String
                    ) {
                        builder.add_soft_space();
                    }
                }
                builder.add_token(tok.text, tok.start);
                let _ = next;
            }
        }
    }
    if !builder.content.is_empty() && !builder.content.ends_with('\n') {
        builder.add_new_line();
        builder.flush_formatting();
    }
}

// ---------------------------------------------------------------------------
// Unit tests — string-level; do not require T-005 `SourceText`
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn statement_starts(text: &str) -> Vec<usize> {
        // Byte offsets of each non-whitespace token that begins a "statement-ish"
        // region: start of file, or token following `;` / `{` / `}` at nesting 0/1.
        let tokens = tokenize(text);
        let mut out = Vec::new();
        if let Some(t) = tokens.first() {
            out.push(t.start);
        }
        for w in tokens.windows(2) {
            if matches!(w[0].text, ";" | "{" | "}") {
                out.push(w[1].start);
            }
        }
        out
    }

    #[test]
    fn identity_when_already_formatted() {
        let src = "function hello() {\n  return 1;\n}\n";
        let result = format_source(src);
        assert_eq!(result.formatted, src);
        assert_eq!(result.map_original, vec![0]);
        assert_eq!(result.map_pretty, vec![0]);
    }

    #[test]
    fn preserves_string_contents_byte_identical() {
        let src = r#"var x="hello\nworld";var y='a\\b';"#;
        let result = format_source(src);
        assert!(result.formatted.contains("\"hello\\nworld\""));
        assert!(result.formatted.contains("'a\\\\b'"));
        // Every original non-ws token appears in order.
        let orig_toks = tokenize(src);
        let pretty_toks = tokenize(&result.formatted);
        let orig_sig: Vec<&str> = orig_toks.iter().map(|t| t.text).collect();
        let pretty_sig: Vec<&str> = pretty_toks.iter().map(|t| t.text).collect();
        assert_eq!(orig_sig, pretty_sig);
    }

    #[test]
    fn preserves_template_literal_newlines() {
        let src = "var t=`line1\nline2=${foo}`;";
        let result = format_source(src);
        assert!(result.formatted.contains("`line1\nline2=${foo}`"));
    }

    #[test]
    fn bidirectional_round_trip_on_statement_starts() {
        let src = "function f(a,b){if(a){return b+1;}else{return a;}}";
        let result = format_source(src);
        assert_ne!(result.formatted, src, "minified input should change");

        for orig_off in statement_starts(src) {
            let pretty_off =
                map_original_to_pretty(&result.map_original, &result.map_pretty, orig_off);
            let back =
                map_pretty_to_original(&result.map_original, &result.map_pretty, pretty_off);
            assert_eq!(
                back, orig_off,
                "round-trip failed at orig {orig_off}: pretty={pretty_off}, back={back}\npretty:\n{}",
                result.formatted
            );
        }
    }

    #[test]
    fn css_round_trip_and_preserve() {
        let src = "body{margin:0;color:red}a{text-decoration:none}";
        let result = format_source(src);
        assert!(result.formatted.contains("margin"));
        assert!(result.formatted.contains(':'));
        let orig_toks: Vec<&str> = tokenize(src).iter().map(|t| t.text).collect();
        let pretty_toks: Vec<&str> = tokenize(&result.formatted).iter().map(|t| t.text).collect();
        assert_eq!(orig_toks, pretty_toks);

        for orig_off in statement_starts(src) {
            let pretty_off =
                map_original_to_pretty(&result.map_original, &result.map_pretty, orig_off);
            let back =
                map_pretty_to_original(&result.map_original, &result.map_pretty, pretty_off);
            assert_eq!(back, orig_off);
        }
    }

    #[test]
    fn location_round_trip_via_line_column() {
        let src = "function f(){return 1;}";
        let result = format_source(src);
        // Walk every token start through line/col conversion.
        for tok in tokenize(src) {
            let (line, col) = offset_to_location(&result.original_lines, &result.original, tok.start);
            let off = location_to_offset(&result.original, &result.original_lines, line, col);
            assert_eq!(off, tok.start);

            let pretty_off =
                map_original_to_pretty(&result.map_original, &result.map_pretty, tok.start);
            let (pl, pc) =
                offset_to_location(&result.pretty_lines, &result.formatted, pretty_off);
            let back_pretty =
                location_to_offset(&result.formatted, &result.pretty_lines, pl, pc);
            assert_eq!(back_pretty, pretty_off);

            let back_orig =
                map_pretty_to_original(&result.map_original, &result.map_pretty, pretty_off);
            assert_eq!(back_orig, tok.start, "token {:?}", tok.text);
        }
    }

    #[test]
    fn nothing_dropped_or_reordered() {
        let src = "const x=1;const y=`keep ${a+b}`;foo(/ab\\/c/g);";
        let result = format_source(src);
        let a: Vec<&str> = tokenize(src).iter().map(|t| t.text).collect();
        let b: Vec<&str> = tokenize(&result.formatted).iter().map(|t| t.text).collect();
        assert_eq!(a, b);
    }


}
