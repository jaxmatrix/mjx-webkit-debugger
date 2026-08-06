//! Fetching and caching source text.
//!
//! **Owned by `docs/tasks/T-011-source-store.md`.**
//!
//! Two protocol paths, chosen by [`crate::SourceKind`]:
//!
//! - scripts → `Debugger.getScriptSource { scriptId }`
//! - documents and stylesheets → `Page.getResourceContent { frameId, url }`,
//!   which may return base64 and must be decoded
//!
//! Both replies can be megabytes. Neither may be awaited on the UI thread.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mjx_wk_protocol::generated::{debugger, page};
use mjx_wk_session::{SessionError, SessionHandle};
use tokio::sync::broadcast;

use crate::{SourceEntry, SourceError, SourceId, SourceKind, SourceText};

/// An LRU cache over fetched source text.
#[derive(Debug)]
pub struct SourceStore {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    budget_bytes: usize,
    bytes_held: usize,
    /// Bumped on [`SourceStore::clear`] so a fetch that races with navigation
    /// cannot repopulate the cache under a reissued script id.
    generation: u64,
    map: HashMap<SourceId, CacheEntry>,
    /// Front = least recently used; back = most recently used.
    lru: VecDeque<SourceId>,
    /// In-flight fetches, so concurrent `text` calls for one id share a request.
    inflight: HashMap<SourceId, broadcast::Sender<SharedFetch>>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    text: Arc<SourceText>,
    bytes: usize,
}

/// Cloneable result shared with waiters on a single in-flight fetch.
#[derive(Debug, Clone)]
enum SharedFetch {
    Ok(Arc<SourceText>),
    Err(SharedError),
}

#[derive(Debug, Clone)]
enum SharedError {
    UnknownSource(SourceId),
    Unavailable { id: SourceId, reason: String },
    NotText(SourceId),
    /// Typed [`SourceError::Session`] is not `Clone`; waiters get the message.
    Session { id: SourceId, reason: String },
}

impl SharedError {
    fn from_source_error(id: SourceId, err: &SourceError) -> Self {
        match err {
            SourceError::UnknownSource(id) => Self::UnknownSource(*id),
            SourceError::Unavailable { id, reason } => Self::Unavailable {
                id: *id,
                reason: reason.clone(),
            },
            SourceError::NotText(id) => Self::NotText(*id),
            SourceError::Session(err) => Self::Session {
                id,
                reason: err.to_string(),
            },
        }
    }
}

impl From<SharedError> for SourceError {
    fn from(err: SharedError) -> Self {
        match err {
            SharedError::UnknownSource(id) => Self::UnknownSource(id),
            SharedError::Unavailable { id, reason } => Self::Unavailable { id, reason },
            SharedError::NotText(id) => Self::NotText(id),
            // Waiters lose the typed session error; the message is enough to
            // surface, and the leader already returned the precise value.
            SharedError::Session { id, reason } => Self::Unavailable { id, reason },
        }
    }
}

impl SourceStore {
    /// A store with a byte budget.
    ///
    /// Bounded by total bytes rather than entry count, because entry sizes here
    /// differ by four orders of magnitude.
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                budget_bytes,
                bytes_held: 0,
                generation: 0,
                map: HashMap::new(),
                lru: VecDeque::new(),
                inflight: HashMap::new(),
            }),
        }
    }

    /// Fetch for a source, fetching it if it is not cached.
    ///
    /// Takes a [`SourceEntry`] rather than a bare [`SourceId`]: choosing between
    /// `Debugger.getScriptSource` and `Page.getResourceContent` needs
    /// `kind` / `script_id` / `frame` / `url`. The cache remains keyed by
    /// `entry.id`. Concurrent calls for the same id must share one request —
    /// the source tree and the editor routinely ask at the same moment, and
    /// fetching a 5 MB bundle twice is a visible stall.
    pub async fn text(
        &self,
        session: &SessionHandle,
        entry: &SourceEntry,
    ) -> Result<Arc<SourceText>, SourceError> {
        let id = entry.id;

        if let Some(hit) = self.cached(id) {
            return Ok(hit);
        }

        // Reject without touching the wire when the entry cannot be fetched.
        if let Some(err) = preparatory_error(entry) {
            return Err(err);
        }

        // Singleflight: either lead the fetch or wait for the leader.
        enum Role {
            Leader { generation: u64 },
            Follower(broadcast::Receiver<SharedFetch>),
        }

        let role = {
            let mut g = lock(&self.inner);
            if let Some(hit) = g.map.get(&id) {
                let text = Arc::clone(&hit.text);
                g.touch(id);
                return Ok(text);
            }
            if let Some(tx) = g.inflight.get(&id) {
                Role::Follower(tx.subscribe())
            } else {
                let (tx, _rx) = broadcast::channel(1);
                g.inflight.insert(id, tx);
                Role::Leader {
                    generation: g.generation,
                }
            }
        };

        match role {
            Role::Follower(mut rx) => match rx.recv().await {
                Ok(SharedFetch::Ok(text)) => Ok(text),
                Ok(SharedFetch::Err(err)) => Err(err.into()),
                // Leader dropped without publishing (clear, panic, task cancel).
                // Retry — cache / a new leader may already exist.
                Err(broadcast::error::RecvError::Closed)
                | Err(broadcast::error::RecvError::Lagged(_)) => {
                    Box::pin(self.text(session, entry)).await
                }
            },
            Role::Leader { generation } => {
                let result = fetch_text(session, entry).await;
                let shared = match &result {
                    Ok((text, _)) => SharedFetch::Ok(Arc::clone(text)),
                    Err(err) => SharedFetch::Err(SharedError::from_source_error(id, err)),
                };

                {
                    let mut g = lock(&self.inner);
                    if let Some(tx) = g.inflight.remove(&id) {
                        let _ = tx.send(shared);
                    }
                    if let Ok((text, bytes)) = &result
                        && g.generation == generation
                    {
                        // Byte size is the UTF-8 length of the fetched body,
                        // measured before wrapping so eviction stays accurate
                        // without a second pass over a multi-megabyte buffer.
                        g.insert(id, Arc::clone(text), *bytes);
                    }
                }

                result.map(|(text, _)| text)
            }
        }
    }

    /// Cached text, if present. Never blocks — safe from the UI thread.
    pub fn cached(&self, id: SourceId) -> Option<Arc<SourceText>> {
        let mut g = lock(&self.inner);
        let hit = g.map.get(&id).map(|e| Arc::clone(&e.text))?;
        g.touch(id);
        Some(hit)
    }

    /// Drop everything.
    ///
    /// Called on navigation: script ids are reissued, so stale text would be
    /// served under a new script's id.
    pub fn clear(&self) {
        let mut g = lock(&self.inner);
        g.map.clear();
        g.lru.clear();
        g.bytes_held = 0;
        g.generation = g.generation.wrapping_add(1);
        // Dropping senders wakes followers with `Closed`; they retry against
        // the empty cache rather than observing a pre-navigation body.
        g.inflight.clear();
    }

    /// Bytes currently held.
    pub fn bytes_held(&self) -> usize {
        lock(&self.inner).bytes_held
    }
}

impl Inner {
    fn touch(&mut self, id: SourceId) {
        if let Some(pos) = self.lru.iter().position(|x| *x == id) {
            self.lru.remove(pos);
            self.lru.push_back(id);
        }
    }

    fn insert(&mut self, id: SourceId, text: Arc<SourceText>, bytes: usize) {
        if let Some(old) = self.map.remove(&id) {
            self.bytes_held = self.bytes_held.saturating_sub(old.bytes);
            if let Some(pos) = self.lru.iter().position(|x| *x == id) {
                self.lru.remove(pos);
            }
        }

        // Evict LRU entries until the new body fits. A single entry larger
        // than the budget is still kept — otherwise a 5 MB bundle could never
        // be cached under a tighter multi-file budget.
        while self.bytes_held.saturating_add(bytes) > self.budget_bytes && !self.lru.is_empty() {
            self.evict_lru();
        }

        self.bytes_held = self.bytes_held.saturating_add(bytes);
        self.map.insert(id, CacheEntry { text, bytes });
        self.lru.push_back(id);
    }

    fn evict_lru(&mut self) {
        let Some(id) = self.lru.pop_front() else {
            return;
        };
        if let Some(entry) = self.map.remove(&id) {
            self.bytes_held = self.bytes_held.saturating_sub(entry.bytes);
        }
    }
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Errors that are known before any protocol call.
fn preparatory_error(entry: &SourceEntry) -> Option<SourceError> {
    let id = entry.id;
    match entry.kind {
        SourceKind::Other => Some(SourceError::NotText(id)),
        SourceKind::Script { .. } if entry.script_id.is_none() => Some(SourceError::Unavailable {
            id,
            reason: "script id invalidated".into(),
        }),
        SourceKind::Document | SourceKind::StyleSheet if entry.frame.is_none() => {
            Some(SourceError::Unavailable {
                id,
                reason: "missing frame id for getResourceContent".into(),
            })
        }
        SourceKind::Document | SourceKind::StyleSheet if entry.url.is_empty() => {
            Some(SourceError::Unavailable {
                id,
                reason: "missing url for getResourceContent".into(),
            })
        }
        _ => None,
    }
}

async fn fetch_text(
    session: &SessionHandle,
    entry: &SourceEntry,
) -> Result<(Arc<SourceText>, usize), SourceError> {
    let id = entry.id;
    let content = match entry.kind {
        SourceKind::Script { .. } => {
            let script_id = entry.script_id.clone().ok_or_else(|| SourceError::Unavailable {
                id,
                reason: "script id invalidated".into(),
            })?;
            let ret = session
                .call(debugger::commands::GetScriptSource { script_id })
                .await
                .map_err(|err| map_session_error(id, err))?;
            ret.script_source
        }
        SourceKind::Document | SourceKind::StyleSheet => {
            let frame_id = entry
                .frame
                .as_ref()
                .ok_or_else(|| SourceError::Unavailable {
                    id,
                    reason: "missing frame id for getResourceContent".into(),
                })?
                .0
                .clone();
            let ret = session
                .call(page::commands::GetResourceContent {
                    frame_id,
                    url: entry.url.clone(),
                })
                .await
                .map_err(|err| map_session_error(id, err))?;
            decode_resource_content(id, ret.content, ret.base64_encoded)?
        }
        SourceKind::Other => return Err(SourceError::NotText(id)),
    };

    let bytes = content.len();
    Ok((Arc::new(SourceText::new(id, content)), bytes))
}

fn map_session_error(id: SourceId, err: SessionError) -> SourceError {
    match err {
        SessionError::Protocol(protocol) => SourceError::Unavailable {
            id,
            reason: protocol.message,
        },
        other => SourceError::Session(other),
    }
}

/// Decode a `Page.getResourceContent` payload into UTF-8 text.
fn decode_resource_content(
    id: SourceId,
    content: String,
    base64_encoded: bool,
) -> Result<String, SourceError> {
    if !base64_encoded {
        return Ok(content);
    }
    let bytes = decode_base64(&content).map_err(|()| SourceError::Unavailable {
        id,
        reason: "invalid base64 in getResourceContent".into(),
    })?;
    String::from_utf8(bytes).map_err(|_| SourceError::NotText(id))
}

/// Standard base64 (RFC 4648) as returned by WebKit's `getResourceContent`.
fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    fn digit(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let chars: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if !chars.len().is_multiple_of(4) {
        return Err(());
    }

    let mut out = Vec::with_capacity(chars.len() / 4 * 3);
    for chunk in chars.chunks_exact(4) {
        let (a, b, c, d) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        let pad = u8::from(c == b'=') + u8::from(d == b'=');
        if pad > 2 || (c == b'=' && d != b'=') {
            return Err(());
        }
        if (a == b'=') || (b == b'=') {
            return Err(());
        }

        let av = digit(a).ok_or(())?;
        let bv = digit(b).ok_or(())?;
        let cv = if c == b'=' { 0 } else { digit(c).ok_or(())? };
        let dv = if d == b'=' { 0 } else { digit(d).ok_or(())? };

        out.push((av << 2) | (bv >> 4));
        if pad < 2 {
            out.push((bv << 4) | (cv >> 2));
        }
        if pad < 1 {
            out.push((cv << 6) | dv);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::FrameId;

    #[test]
    fn decode_base64_round_trip_ascii() {
        // "hello" → aGVsbG8=
        let bytes = decode_base64("aGVsbG8=").expect("base64");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn decode_resource_plain_passes_through() {
        let text = decode_resource_content(SourceId(1), "body {}".into(), false).unwrap();
        assert_eq!(text, "body {}");
    }

    #[test]
    fn decode_resource_base64_text() {
        let text = decode_resource_content(SourceId(1), "Ym9keSB7fQ==".into(), true).unwrap();
        assert_eq!(text, "body {}");
    }

    #[test]
    fn decode_resource_base64_binary_is_not_text() {
        // PNG signature bytes, base64-encoded — valid base64, not UTF-8.
        let png = decode_base64("iVBORw0KGgo=").expect("fixture b64");
        assert!(String::from_utf8(png).is_err());
        let err = decode_resource_content(SourceId(7), "iVBORw0KGgo=".into(), true).unwrap_err();
        assert!(matches!(err, SourceError::NotText(SourceId(7))));
    }

    #[test]
    fn preparatory_other_is_not_text() {
        let entry = SourceEntry {
            id: SourceId(3),
            script_id: None,
            frame: None,
            url: "https://example.test/x.png".into(),
            kind: SourceKind::Other,
            source_map_url: None,
            is_original: false,
        };
        assert!(matches!(
            preparatory_error(&entry),
            Some(SourceError::NotText(SourceId(3)))
        ));
    }

    #[test]
    fn preparatory_script_without_id_is_unavailable() {
        let entry = SourceEntry {
            id: SourceId(4),
            script_id: None,
            frame: Some(FrameId("f".into())),
            url: "https://example.test/a.js".into(),
            kind: SourceKind::Script {
                module: false,
                content_script: false,
            },
            source_map_url: None,
            is_original: false,
        };
        assert!(matches!(
            preparatory_error(&entry),
            Some(SourceError::Unavailable { id: SourceId(4), .. })
        ));
    }
}
