//! Source maps.
//!
//! **Phase 2 — owned by `docs/tasks/T-205-source-maps.md`.**
//!
//! The seam exists in Phase 1 so the code view and breakpoint model can be
//! written against [`SourceLocation`] indirection from the start. Retrofitting
//! a mapping step under a UI that assumed generated positions means touching
//! every panel.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use mjx_wk_protocol::generated::{network, page};
use mjx_wk_session::SessionHandle;
use sourcemap::{DecodedMap, SourceMap};

use crate::{SourceEntry, SourceError, SourceId, SourceKind, SourceLocation};

/// Resolve a possibly-relative `sourceMapURL` against the script that named it.
///
/// `data:` URIs and absolute URLs are returned unchanged. Relative references
/// join against `script_url` (the script's own URL, not the page URL) so a map
/// next to `…/js/bundle.js` resolves to `…/js/bundle.js.map`.
pub fn resolve_source_map_url(script_url: &str, source_map_url: &str) -> String {
    let source_map_url = source_map_url.trim();
    if source_map_url.is_empty() {
        return String::new();
    }
    if source_map_url.starts_with("data:") {
        return source_map_url.to_owned();
    }
    if is_absolute_url(source_map_url) {
        return source_map_url.to_owned();
    }
    join_url(script_url, source_map_url)
}

/// Resolves between generated and original positions.
#[derive(Debug, Default)]
pub struct SourceMapResolver {
    /// Successfully loaded maps, keyed by the generated [`SourceId`].
    maps: HashMap<SourceId, LoadedMap>,
    /// Authored URL → dense local id (shared when several bundles name one file).
    by_original_url: HashMap<String, SourceId>,
    /// Authored entries reconstructed from maps (`is_original = true`).
    originals: HashMap<SourceId, SourceEntry>,
    /// `(original, line)` → every generated location that line was inlined into.
    reverse: HashMap<(SourceId, u32), Vec<SourceLocation>>,
    /// Next id to hand to an authored source. Skips ids already used as keys.
    next_id: u32,
}

#[derive(Debug)]
struct LoadedMap {
    map: SourceMap,
    /// Parallel to the map's `sources` list.
    original_ids: Vec<SourceId>,
}

impl SourceMapResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the map named by a script's `sourceMapURL`.
    ///
    /// The URL may be a `data:` URI with the whole map inline, a relative URL
    /// to resolve against the script, or absent. A map that fails to load is
    /// not an error the user should see — the generated source is still
    /// perfectly debuggable.
    ///
    /// Callers that have the script URL should absolutize with
    /// [`resolve_source_map_url`] before calling. Relative URLs that reach
    /// here fall back to joining against the session target URL.
    pub async fn load(
        &mut self,
        session: &SessionHandle,
        generated: SourceId,
        source_map_url: &str,
    ) -> Result<(), SourceError> {
        let raw = source_map_url.trim();
        if raw.is_empty() {
            return Ok(());
        }

        // Prefer an already-absolute / data: URL; otherwise join against the
        // page URL as a last resort when the caller did not absolutize.
        let map_url = if raw.starts_with("data:") || is_absolute_url(raw) {
            raw.to_owned()
        } else {
            join_url(&session.target().url, raw)
        };

        let Some(bytes) = fetch_map_bytes(session, &map_url).await else {
            // Fetch failed, empty body, non-OK status — generated source stays
            // debuggable; do not surface an error.
            return Ok(());
        };

        let Some(map) = parse_map(&bytes) else {
            return Ok(());
        };

        self.install_map(generated, &map_url, map);
        Ok(())
    }

    /// Authored sources reconstructed from the map for `generated`.
    ///
    /// Each entry has [`SourceEntry::is_original`] set so a caller can merge
    /// them into the source tree. Ids are allocated by this resolver and are
    /// the ones [`to_original`] / [`to_generated`] speak.
    pub fn original_entries(&self, generated: SourceId) -> Vec<SourceEntry> {
        let Some(loaded) = self.maps.get(&generated) else {
            return Vec::new();
        };
        loaded
            .original_ids
            .iter()
            .filter_map(|id| self.originals.get(id).cloned())
            .collect()
    }

    /// Generated position → authored position.
    pub fn to_original(&self, location: SourceLocation) -> Option<SourceLocation> {
        let loaded = self.maps.get(&location.source)?;
        let token = loaded.map.lookup_token(location.line, location.column)?;
        if !token.has_source() {
            return None;
        }
        let src_id = *loaded.original_ids.get(token.get_src_id() as usize)?;
        Some(SourceLocation {
            source: src_id,
            line: token.get_src_line(),
            column: token.get_src_col(),
        })
    }

    /// Authored position → generated positions.
    ///
    /// Returns several: one authored line can be inlined in many places, and a
    /// breakpoint on it must be set at each.
    pub fn to_generated(&self, location: SourceLocation) -> Vec<SourceLocation> {
        let Some(all) = self.reverse.get(&(location.source, location.line)) else {
            return Vec::new();
        };
        if location.column == 0 {
            // Line-level breakpoint: every inline of that authored line.
            return all.clone();
        }
        // Column-precise: keep generated sites whose original column matches,
        // falling back to the whole line if the map has no exact column.
        let exact: Vec<SourceLocation> = all
            .iter()
            .copied()
            .filter(|dst| {
                self.to_original(*dst)
                    .is_some_and(|o| o.column == location.column)
            })
            .collect();
        if exact.is_empty() { all.clone() } else { exact }
    }

    /// Whether a source has a usable map.
    pub fn has_map(&self, generated: SourceId) -> bool {
        self.maps.contains_key(&generated)
    }

    fn install_map(&mut self, generated: SourceId, map_url: &str, map: SourceMap) {
        // Drop a previous map for this generated id so reverse entries do not
        // accumulate stale locations across reloads of the same SourceId.
        if self.maps.contains_key(&generated) {
            self.revoke_generated(generated);
        }

        let source_root = map.get_source_root().map(str::to_owned);
        let mut original_ids = Vec::with_capacity(map.get_source_count() as usize);

        for idx in 0..map.get_source_count() {
            let raw = map.get_source(idx).unwrap_or("");
            let url = resolve_source_path(map_url, source_root.as_deref(), raw);
            let id = self.ensure_original(&url);
            original_ids.push(id);
        }

        for token in map.tokens() {
            if !token.has_source() {
                continue;
            }
            let Some(&orig_id) = original_ids.get(token.get_src_id() as usize) else {
                continue;
            };
            let dst = SourceLocation {
                source: generated,
                line: token.get_dst_line(),
                column: token.get_dst_col(),
            };
            let key = (orig_id, token.get_src_line());
            match self.reverse.entry(key) {
                Entry::Vacant(v) => {
                    v.insert(vec![dst]);
                }
                Entry::Occupied(mut o) => {
                    let list = o.get_mut();
                    if !list.contains(&dst) {
                        list.push(dst);
                    }
                }
            }
        }

        self.maps.insert(generated, LoadedMap { map, original_ids });
    }

    fn revoke_generated(&mut self, generated: SourceId) {
        let Some(loaded) = self.maps.remove(&generated) else {
            return;
        };
        // Drop reverse entries that pointed only at this generated source.
        self.reverse.retain(|_, locs| {
            locs.retain(|loc| loc.source != generated);
            !locs.is_empty()
        });
        // Originals that no other map still references can stay — a reload of
        // the same URL reuses the id via `by_original_url`. Keep the entries.
        let _ = loaded;
    }

    fn ensure_original(&mut self, url: &str) -> SourceId {
        if let Some(&id) = self.by_original_url.get(url) {
            return id;
        }
        let id = self.alloc_id();
        let entry = SourceEntry {
            id,
            script_id: None,
            frame: None,
            url: url.to_owned(),
            kind: kind_for_original_url(url),
            source_map_url: None,
            is_original: true,
        };
        self.by_original_url.insert(url.to_owned(), id);
        self.originals.insert(id, entry);
        id
    }

    fn alloc_id(&mut self) -> SourceId {
        loop {
            let id = SourceId(self.next_id);
            self.next_id = self.next_id.wrapping_add(1);
            if self.maps.contains_key(&id) || self.originals.contains_key(&id) {
                continue;
            }
            return id;
        }
    }
}

fn is_absolute_url(s: &str) -> bool {
    // `Url::parse` accepts some non-absolute forms; require a scheme:// shape
    // (or scheme: for webpack/file-style ids that maps already emit absolute).
    match url::Url::parse(s) {
        Ok(u) => {
            !u.scheme().is_empty()
                && (u.has_host() || matches!(u.scheme(), "file" | "webpack" | "webpack-internal"))
        }
        Err(_) => false,
    }
}

fn join_url(base: &str, relative: &str) -> String {
    match url::Url::parse(base) {
        Ok(base_url) => match base_url.join(relative) {
            Ok(joined) => joined.to_string(),
            Err(_) => relative.to_owned(),
        },
        Err(_) => {
            // Non-URL script ids (eval, bare paths): best-effort path join.
            if base.is_empty() {
                return relative.to_owned();
            }
            if base.ends_with('/') {
                format!("{base}{relative}")
            } else if let Some(idx) = base.rfind('/') {
                format!("{}{relative}", &base[..=idx])
            } else {
                relative.to_owned()
            }
        }
    }
}

fn resolve_source_path(map_url: &str, source_root: Option<&str>, source: &str) -> String {
    let source = source.trim();
    if source.is_empty() {
        return String::new();
    }
    if is_absolute_url(source) || source.starts_with("data:") {
        return source.to_owned();
    }
    let base = match source_root {
        Some(root) if !root.is_empty() => {
            if is_absolute_url(root) {
                root.to_owned()
            } else if map_url.starts_with("data:") {
                // data: maps have no directory; leave source_root-relative as-is
                // unless root itself joins sensibly against an empty base.
                root.to_owned()
            } else {
                join_url(map_url, root)
            }
        }
        _ if map_url.starts_with("data:") => {
            // Inline maps often use bare paths (`src/app.ts`); keep them stable
            // so the tree label is the path the author wrote.
            return source.to_owned();
        }
        _ => map_url.to_owned(),
    };
    if base.is_empty() {
        source.to_owned()
    } else {
        join_url(&base, source)
    }
}

fn kind_for_original_url(url: &str) -> SourceKind {
    let path = url::Url::parse(url)
        .ok()
        .and_then(|u| u.path().rsplit('/').next().map(str::to_owned))
        .unwrap_or_else(|| url.rsplit('/').next().unwrap_or(url).to_owned());
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".css") || lower.ends_with(".scss") || lower.ends_with(".less") {
        SourceKind::StyleSheet
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        SourceKind::Document
    } else if lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        SourceKind::Script {
            module: lower.ends_with(".mjs")
                || lower.ends_with(".ts")
                || lower.ends_with(".tsx")
                || lower.ends_with(".jsx"),
            content_script: false,
        }
    } else {
        SourceKind::Script {
            module: false,
            content_script: false,
        }
    }
}

fn parse_map(bytes: &[u8]) -> Option<SourceMap> {
    let decoded = sourcemap::decode_slice(bytes).ok()?;
    match decoded {
        DecodedMap::Regular(map) => Some(map),
        DecodedMap::Index(index) => index.flatten().ok(),
        // React Native Hermes maps are out of scope for WebKit debugging; a
        // failed load stays silent so the generated script remains usable.
        DecodedMap::Hermes(_) => None,
    }
}

/// Fetch map bytes. Returns `None` on any failure — callers treat that as
/// "no map", never as a user-visible error.
async fn fetch_map_bytes(session: &SessionHandle, map_url: &str) -> Option<Vec<u8>> {
    if map_url.starts_with("data:") {
        return decode_data_url_bytes(map_url);
    }

    let frame_id = main_frame_id(session).await?;
    let reply = session
        .call(network::commands::LoadResource {
            frame_id,
            url: map_url.to_owned(),
        })
        .await
        .ok()?;

    // Non-OK HTTP is a failed load, not a protocol error.
    if reply.status < 200 || reply.status >= 300 {
        return None;
    }
    Some(reply.content.into_bytes())
}

fn decode_data_url_bytes(url: &str) -> Option<Vec<u8>> {
    // Prefer the sourcemap crate's decoder (handles charset + base64); fall
    // back to a minimal raw `data:application/json,...` form some tools emit.
    match sourcemap::decode_data_url(url) {
        Ok(decoded) => {
            let mut buf = Vec::new();
            decoded.to_writer(&mut buf).ok()?;
            Some(buf)
        }
        Err(_) => decode_raw_json_data_url(url),
    }
}

fn decode_raw_json_data_url(url: &str) -> Option<Vec<u8>> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.contains(";base64") {
        return None;
    }
    // Percent-decoded JSON payload.
    let decoded = percent_decode(data)?;
    Some(decoded.into_bytes())
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(h, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

async fn main_frame_id(session: &SessionHandle) -> Option<String> {
    let tree = session
        .call(page::commands::GetResourceTree {})
        .await
        .ok()?;
    Some(tree.frame_tree.frame.id)
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    #[test]
    fn data_uri_unchanged() {
        let data = "data:application/json;base64,e30=";
        assert_eq!(
            resolve_source_map_url("https://example.test/a.js", data),
            data
        );
    }

    #[test]
    fn absolute_unchanged() {
        assert_eq!(
            resolve_source_map_url(
                "https://example.test/js/a.js",
                "https://cdn.example.test/a.js.map"
            ),
            "https://cdn.example.test/a.js.map"
        );
    }

    #[test]
    fn relative_joins_against_script_directory() {
        assert_eq!(
            resolve_source_map_url("https://example.test/js/bundle.js", "bundle.js.map"),
            "https://example.test/js/bundle.js.map"
        );
        assert_eq!(
            resolve_source_map_url("https://example.test/js/bundle.js", "./bundle.js.map"),
            "https://example.test/js/bundle.js.map"
        );
        assert_eq!(
            resolve_source_map_url("https://example.test/js/bundle.js", "../maps/b.js.map"),
            "https://example.test/maps/b.js.map"
        );
    }
}
