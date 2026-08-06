//! Searching across sources.
//!
//! **Owned by `docs/tasks/T-012-search.md`.**
//!
//! Two strategies, and the choice matters. `Page.searchInResources` searches
//! everything the debuggee knows without transferring it — the right answer for
//! a large site. Local search over the cache is instant and works offline, and
//! is the right answer for what the user already has open. Do both: local
//! first for immediate feedback, remote to fill in the rest.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use regex::{Regex, RegexBuilder};

use crate::{SourceError, SourceId, SourceLocation};

/// Cap on `SearchHit::line_text`. A minified "line" can be the whole file.
const MAX_LINE_DISPLAY: usize = 240;

/// What to look for.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub is_regex: bool,
    /// Restrict to one source; `None` searches everything.
    pub within: Option<SourceId>,
}

/// One match.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub location: SourceLocation,
    /// The whole line, for display. Truncated for minified sources, where the
    /// "line" can be megabytes.
    pub line_text: String,
    /// Byte range of the match within `line_text`.
    pub match_range: Range<u32>,
}

/// Why a query cannot be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    /// `is_regex` was set but the pattern does not compile.
    #[error("invalid regular expression: {0}")]
    InvalidRegex(String),
}

/// One cached source the local index can scan.
#[derive(Debug, Clone)]
struct IndexedSource {
    text: Arc<str>,
    /// Resource URL when known — used to join remote `searchInResources` hits.
    url: Option<String>,
}

/// Runs searches over local and remote sources.
#[derive(Debug, Default)]
pub struct SearchIndex {
    sources: HashMap<SourceId, IndexedSource>,
    /// Reverse map for remote results keyed by URL.
    by_url: HashMap<String, SourceId>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace cached source text for local search.
    ///
    /// The source store (and tests) call this when text becomes available. Local
    /// search never awaits and never talks to the debuggee.
    pub fn upsert(&mut self, id: SourceId, text: impl Into<Arc<str>>) {
        let text = text.into();
        let url = self.sources.get(&id).and_then(|s| s.url.clone());
        if let Some(ref u) = url {
            self.by_url.insert(u.clone(), id);
        }
        self.sources.insert(id, IndexedSource { text, url });
    }

    /// Associate a resource URL with a source so remote hits can be resolved.
    pub fn set_url(&mut self, id: SourceId, url: impl Into<String>) {
        let url = url.into();
        if let Some(prev) = self.sources.get_mut(&id) {
            if let Some(old) = prev.url.take() {
                self.by_url.remove(&old);
            }
            prev.url = Some(url.clone());
        } else {
            self.sources.insert(
                id,
                IndexedSource {
                    text: Arc::from(""),
                    url: Some(url.clone()),
                },
            );
        }
        self.by_url.insert(url, id);
    }

    /// Drop one source from the local cache.
    pub fn remove(&mut self, id: SourceId) {
        if let Some(prev) = self.sources.remove(&id)
            && let Some(url) = prev.url
        {
            self.by_url.remove(&url);
        }
    }

    /// Drop everything — call on navigation when script ids are reissued.
    pub fn clear(&mut self) {
        self.sources.clear();
        self.by_url.clear();
    }

    /// Validate a query before searching. Invalid regex is reported here rather
    /// than panicking inside [`Self::search_local`].
    pub fn check_query(query: &SearchQuery) -> Result<(), SearchError> {
        compile_needle(query).map(|_| ())
    }

    /// Search cached sources. Synchronous and immediate.
    ///
    /// An invalid regex yields an empty list — call [`Self::check_query`] to
    /// surface the error to the user.
    pub fn search_local(&self, query: &SearchQuery) -> Vec<SearchHit> {
        let Ok(needle) = compile_needle(query) else {
            return Vec::new();
        };
        if needle.is_empty() {
            return Vec::new();
        }

        let mut hits = Vec::new();
        let mut ids: Vec<SourceId> = self.sources.keys().copied().collect();
        ids.sort_unstable();

        for id in ids {
            if let Some(within) = query.within
                && within != id
            {
                continue;
            }
            let Some(src) = self.sources.get(&id) else {
                continue;
            };
            search_text(id, &src.text, &needle, &mut hits);
        }
        hits
    }

    /// Search everything the debuggee has, via `Page.searchInResources`.
    ///
    /// For each matching resource, fetches line-level hits with
    /// `Page.searchInResource` and resolves URLs through the local URL map.
    /// Resources the index has never seen are skipped — they cannot become a
    /// [`SourceLocation`] without a [`SourceId`].
    pub async fn search_remote(
        &self,
        session: &mjx_wk_session::SessionHandle,
        query: &SearchQuery,
    ) -> Result<Vec<SearchHit>, SourceError> {
        let needle = match compile_needle(query) {
            Ok(n) => n,
            Err(_) => return Ok(Vec::new()),
        };
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        use mjx_wk_protocol::generated::page::commands::{SearchInResource, SearchInResources};

        let overview = session
            .call(SearchInResources {
                text: query.text.clone(),
                case_sensitive: Some(query.case_sensitive),
                is_regex: Some(query.is_regex),
            })
            .await?;

        let mut hits = Vec::new();
        for result in overview.result {
            let Some(&id) = self.by_url.get(&result.url) else {
                continue;
            };
            if let Some(within) = query.within
                && within != id
            {
                continue;
            }

            // Prefer scanning cached text when we have it — same match positions
            // without another round trip, and truncation stays consistent.
            if let Some(src) = self.sources.get(&id)
                && !src.text.is_empty()
            {
                search_text(id, &src.text, &needle, &mut hits);
                continue;
            }

            let matches = session
                .call(SearchInResource {
                    frame_id: result.frame_id,
                    url: result.url,
                    query: query.text.clone(),
                    case_sensitive: Some(query.case_sensitive),
                    is_regex: Some(query.is_regex),
                    request_id: result.request_id,
                })
                .await?;

            for m in matches.result {
                let line = f64_line(m.line_number);
                let line_content = m.line_content;
                for (start, end) in find_on_line(&line_content, &needle) {
                    let column = utf16_len(&line_content[..start]);
                    let (line_text, match_range) = truncate_line(&line_content, start, end);
                    hits.push(SearchHit {
                        location: SourceLocation {
                            source: id,
                            line,
                            column,
                        },
                        line_text,
                        match_range,
                    });
                }
            }
        }
        Ok(hits)
    }

    /// Merge remote hits into a local-first list without reordering what is
    /// already shown. Hits already present (same location + match range) are
    /// skipped; new ones append.
    pub fn merge_remote(local: Vec<SearchHit>, remote: Vec<SearchHit>) -> Vec<SearchHit> {
        let mut out = local;
        for hit in remote {
            if !out.iter().any(|existing| same_hit(existing, &hit)) {
                out.push(hit);
            }
        }
        out
    }
}

fn same_hit(a: &SearchHit, b: &SearchHit) -> bool {
    a.location == b.location && a.match_range == b.match_range
}

/// Compiled needle. Literals are regex-escaped so one matcher covers both modes.
struct Needle {
    re: Regex,
    /// True when the original query text was empty — never match.
    empty: bool,
}

impl Needle {
    fn is_empty(&self) -> bool {
        self.empty
    }
}

fn compile_needle(query: &SearchQuery) -> Result<Needle, SearchError> {
    if query.text.is_empty() {
        // A never-matching pattern; callers short-circuit on `is_empty`.
        let re = Regex::new("a^").map_err(|e| SearchError::InvalidRegex(e.to_string()))?;
        return Ok(Needle { re, empty: true });
    }
    let pattern = if query.is_regex {
        query.text.clone()
    } else {
        regex::escape(&query.text)
    };
    let re = RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .build()
        .map_err(|e| SearchError::InvalidRegex(e.to_string()))?;
    Ok(Needle { re, empty: false })
}

fn search_text(id: SourceId, text: &str, needle: &Needle, hits: &mut Vec<SearchHit>) {
    let mut line_no = 0u32;
    let mut rest = text;
    loop {
        let (line, next) = split_first_line(rest);
        for (start, end) in find_on_line(line, needle) {
            let column = utf16_len(&line[..start]);
            let (line_text, match_range) = truncate_line(line, start, end);
            hits.push(SearchHit {
                location: SourceLocation {
                    source: id,
                    line: line_no,
                    column,
                },
                line_text,
                match_range,
            });
        }
        match next {
            Some(n) => {
                rest = n;
                line_no = line_no.saturating_add(1);
            }
            None => break,
        }
    }
}

/// Split off the first line (without terminator). Returns `(line, Some(rest))`
/// when a terminator was present, or `(whole, None)` at EOF. A trailing
/// newline does not produce an empty phantom line.
fn split_first_line(text: &str) -> (&str, Option<&str>) {
    if let Some(i) = text.find('\n') {
        let line = if i > 0 && text.as_bytes()[i - 1] == b'\r' {
            &text[..i - 1]
        } else {
            &text[..i]
        };
        let rest = &text[i + 1..];
        if rest.is_empty() {
            (line, None)
        } else {
            (line, Some(rest))
        }
    } else if let Some(i) = text.find('\r') {
        let line = &text[..i];
        let rest = &text[i + 1..];
        if rest.is_empty() {
            (line, None)
        } else {
            (line, Some(rest))
        }
    } else {
        (text, None)
    }
}

fn find_on_line(line: &str, needle: &Needle) -> Vec<(usize, usize)> {
    if needle.empty {
        return Vec::new();
    }
    needle
        .re
        .find_iter(line)
        .map(|m| (m.start(), m.end()))
        .collect()
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn f64_line(n: f64) -> u32 {
    if !n.is_finite() || n < 0.0 {
        0
    } else if n >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        n as u32
    }
}

/// Build display text for a hit, truncating around the match when the line is
/// longer than [`MAX_LINE_DISPLAY`].
fn truncate_line(line: &str, match_start: usize, match_end: usize) -> (String, Range<u32>) {
    let match_start = match_start.min(line.len());
    let match_end = match_end.min(line.len()).max(match_start);

    if line.len() <= MAX_LINE_DISPLAY {
        return (
            line.to_owned(),
            match_start as u32..match_end as u32,
        );
    }

    let match_len = match_end - match_start;
    let (win_start, win_end) = if match_len >= MAX_LINE_DISPLAY {
        let end = floor_char_boundary(line, match_start + MAX_LINE_DISPLAY);
        (match_start, end)
    } else {
        let pad = (MAX_LINE_DISPLAY - match_len) / 2;
        let mut start = match_start.saturating_sub(pad);
        start = floor_char_boundary(line, start);
        let mut end = ceil_char_boundary(line, (start + MAX_LINE_DISPLAY).min(line.len()));
        if end < match_end {
            end = ceil_char_boundary(line, match_end);
            start = floor_char_boundary(line, end.saturating_sub(MAX_LINE_DISPLAY));
        }
        if start > match_start {
            start = floor_char_boundary(line, match_start);
        }
        (start, end.min(line.len()))
    };

    let mut out = String::with_capacity(MAX_LINE_DISPLAY + 6);
    let mut adjust = 0u32;
    if win_start > 0 {
        out.push('…');
        adjust = '…'.len_utf8() as u32;
    }
    out.push_str(&line[win_start..win_end]);
    if win_end < line.len() {
        out.push('…');
    }

    let rel_start = (match_start - win_start) as u32 + adjust;
    let rel_end = (match_end - win_start) as u32 + adjust;
    (out, rel_start..rel_end)
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use mjx_wk_dialect::{DialectKind, WebKitDialect};
    use mjx_wk_protocol::TargetType;
    use mjx_wk_session::Session;
    use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};

    fn q(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            case_sensitive: false,
            is_regex: false,
            within: None,
        }
    }

    #[test]
    fn local_literal_hits_are_immediate() {
        let mut index = SearchIndex::new();
        index.upsert(SourceId(1), "alpha\nbeta foo\ngamma");
        index.upsert(SourceId(2), "foo at start");

        let hits = index.search_local(&q("foo"));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].location.source, SourceId(1));
        assert_eq!(hits[0].location.line, 1);
        assert!(hits[0].line_text.contains("foo"));
        assert_eq!(hits[1].location.source, SourceId(2));
    }

    #[test]
    fn case_sensitive_mode_distinguishes_case() {
        let mut index = SearchIndex::new();
        index.upsert(SourceId(1), "Foo foo FOO");

        let mut query = q("foo");
        query.case_sensitive = true;
        let hits = index.search_local(&query);
        assert_eq!(hits.len(), 1);
        assert_eq!(&hits[0].line_text[hits[0].match_range.start as usize..hits[0].match_range.end as usize], "foo");
    }

    #[test]
    fn regex_mode_matches_and_invalid_regex_is_reported() {
        let mut index = SearchIndex::new();
        index.upsert(SourceId(1), "a1 b22 c3");

        let mut query = q(r"b\d+");
        query.is_regex = true;
        let hits = index.search_local(&query);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &hits[0].line_text[hits[0].match_range.start as usize..hits[0].match_range.end as usize],
            "b22"
        );

        let mut bad = q("[unterminated");
        bad.is_regex = true;
        let err = SearchIndex::check_query(&bad).expect_err("must report");
        assert!(matches!(err, SearchError::InvalidRegex(_)));
        assert!(index.search_local(&bad).is_empty(), "must not panic");
    }

    #[test]
    fn minified_line_is_truncated_around_the_match() {
        let mut index = SearchIndex::new();
        let mut huge = "x".repeat(50_000);
        huge.push_str("NEEDLE");
        huge.push_str(&"y".repeat(50_000));
        index.upsert(SourceId(7), huge);

        let hits = index.search_local(&q("NEEDLE"));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].line_text.len() <= MAX_LINE_DISPLAY + 6);
        assert!(hits[0].line_text.contains("NEEDLE"));
        assert!(hits[0].line_text.starts_with('…') || hits[0].line_text.ends_with('…'));
        let slice = &hits[0].line_text
            [hits[0].match_range.start as usize..hits[0].match_range.end as usize];
        assert_eq!(slice, "NEEDLE");
    }

    #[test]
    fn within_restricts_to_one_source() {
        let mut index = SearchIndex::new();
        index.upsert(SourceId(1), "hit");
        index.upsert(SourceId(2), "hit");
        let mut query = q("hit");
        query.within = Some(SourceId(2));
        let hits = index.search_local(&query);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].location.source, SourceId(2));
    }

    #[test]
    fn merge_remote_preserves_local_order() {
        let local = vec![
            SearchHit {
                location: SourceLocation {
                    source: SourceId(1),
                    line: 0,
                    column: 0,
                },
                line_text: "first".into(),
                match_range: 0..5,
            },
            SearchHit {
                location: SourceLocation {
                    source: SourceId(2),
                    line: 0,
                    column: 0,
                },
                line_text: "second".into(),
                match_range: 0..6,
            },
        ];
        let remote = vec![
            SearchHit {
                location: SourceLocation {
                    source: SourceId(2),
                    line: 0,
                    column: 0,
                },
                line_text: "second-dup".into(),
                match_range: 0..6,
            },
            SearchHit {
                location: SourceLocation {
                    source: SourceId(3),
                    line: 1,
                    column: 2,
                },
                line_text: "third".into(),
                match_range: 0..5,
            },
            SearchHit {
                location: SourceLocation {
                    source: SourceId(0),
                    line: 0,
                    column: 0,
                },
                line_text: "should-not-lead".into(),
                match_range: 0..4,
            },
        ];

        let merged = SearchIndex::merge_remote(local.clone(), remote);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].location.source, SourceId(1));
        assert_eq!(merged[1].location.source, SourceId(2));
        assert_eq!(merged[1].line_text, "second");
        assert_eq!(merged[2].location.source, SourceId(3));
        assert_eq!(merged[3].location.source, SourceId(0));
    }

    fn page_target() -> Target {
        Target {
            key: TargetKey("test/page".into()),
            name: "fixture".into(),
            url: "https://example.test/".into(),
            kind: TargetType::WebPage,
            dialect: DialectKind::WebKitRwi,
            origin: TransportOrigin::Replay {
                fixture: "inline".into(),
            },
        }
    }

    async fn attach(trace: &str) -> mjx_wk_session::SessionHandle {
        let transport = ReplayTransport::from_str(trace, "inline-trace").expect("trace parses");
        Session::attach(Box::new(transport), Box::new(WebKitDialect), page_target())
            .await
            .expect("attach")
    }

    #[tokio::test]
    async fn search_remote_resolves_urls_via_replay() {
        let trace = r#"
{"dir":"send","frame":{"id":1,"method":"Inspector.enable","params":{}}}
{"dir":"recv","frame":{"id":1,"result":{}}}
{"dir":"send","frame":{"id":2,"method":"Page.searchInResources","params":{"text":"hello","caseSensitive":false,"isRegex":false}}}
{"dir":"recv","frame":{"id":2,"result":{"result":[{"url":"https://example.test/app.js","frameId":"frame-1","matchesCount":1}]}}}
"#;
        let session = attach(trace).await;
        let mut index = SearchIndex::new();
        index.upsert(SourceId(9), "const x = 1;\nhello world;\n");
        index.set_url(SourceId(9), "https://example.test/app.js");

        let hits = index
            .search_remote(&session, &q("hello"))
            .await
            .expect("remote");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].location.source, SourceId(9));
        assert_eq!(hits[0].location.line, 1);
        assert!(hits[0].line_text.contains("hello"));
    }
}
