//! What sources exist, and how they are grouped for display.
//!
//! **Owned by `docs/tasks/T-004-source-inventory.md`.**
//!
//! Merges two feeds that arrive at different times and in different shapes:
//!
//! | Feed | Carries | Arrives |
//! |---|---|---|
//! | `Debugger.scriptParsed` | scripts, by `scriptId` | streamed, continuously |
//! | `Page.getResourceTree` | documents, stylesheets, images, by URL | once, on request |
//!
//! A script and a resource can describe the same file. They must appear once.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::{FrameId, SourceId, SourceKind};

/// One known source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry {
    pub id: SourceId,
    /// The debuggee's `scriptId`, when this came from `scriptParsed`. Needed to
    /// fetch text, and invalid after a reload.
    pub script_id: Option<String>,
    /// The frame this belongs to, needed by `Page.getResourceContent`.
    pub frame: Option<FrameId>,
    pub url: String,
    pub kind: SourceKind,
    /// From the `sourceMappingURL` comment or the `scriptParsed` field. May be
    /// a `data:` URI carrying the whole map inline.
    pub source_map_url: Option<String>,
    /// True for a source reconstructed from a source map rather than served.
    pub is_original: bool,
}

impl SourceEntry {
    /// The name to show in a file tree — the last path segment, or something
    /// sensible for the many sources that have no useful URL.
    ///
    /// `eval` and injected scripts arrive with an empty URL, and a tree full of
    /// blank rows is useless.
    pub fn display_name(&self) -> String {
        if let Some(name) = file_name_from_url(&self.url) {
            return name;
        }
        match self.kind {
            SourceKind::Script {
                content_script: true,
                ..
            } => "(content script)".to_owned(),
            SourceKind::Script { .. } => match &self.script_id {
                // Include the debuggee id so two evals are not identical blank rows.
                Some(script_id) => format!("(eval) #{script_id}"),
                None => "(eval)".to_owned(),
            },
            SourceKind::Document => "(document)".to_owned(),
            SourceKind::StyleSheet => "(stylesheet)".to_owned(),
            SourceKind::Other => "(other)".to_owned(),
        }
    }
}

/// The file tree, grouped by origin then by path.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceTreeNode {
    /// An origin (`https://example.com`) or a synthetic group.
    Group {
        label: String,
        children: Vec<SourceTreeNode>,
    },
    /// A source.
    Leaf { id: SourceId, label: String },
}

/// Every source the debuggee has told us about.
#[derive(Debug)]
pub struct SourceInventory {
    entries: Vec<SourceEntry>,
    /// Non-empty URLs only — empty-URL evals must not collapse onto one id.
    by_url: HashMap<String, SourceId>,
    by_script_id: HashMap<String, SourceId>,
    /// Rebuilt on mutation so the UI can read a pointer, not walk entries.
    cached_tree: SourceTreeNode,
}

impl Default for SourceInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceInventory {
    /// An empty inventory.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_url: HashMap::new(),
            by_script_id: HashMap::new(),
            cached_tree: empty_root(),
        }
    }

    /// Record a `Debugger.scriptParsed`.
    ///
    /// Returns the id, which is stable across reloads for a given URL: the user
    /// keeps their editor tab and their breakpoints when the page reloads.
    pub fn on_script_parsed(
        &mut self,
        event: &mjx_wk_protocol::generated::debugger::events::ScriptParsed,
    ) -> SourceId {
        let url = effective_script_url(event);
        let kind = SourceKind::Script {
            module: event.module.unwrap_or(false),
            content_script: event.is_content_script.unwrap_or(false),
        };
        let source_map_url = event.source_map_url.clone().filter(|s| !s.is_empty());

        let id = if !url.is_empty() {
            if let Some(&existing) = self.by_url.get(&url) {
                self.update_script_entry(existing, &event.script_id, kind, source_map_url);
                existing
            } else {
                self.insert_entry(SourceEntry {
                    id: SourceId(0), // filled by insert_entry
                    script_id: Some(event.script_id.clone()),
                    frame: None,
                    url,
                    kind,
                    source_map_url,
                    is_original: false,
                })
            }
        } else {
            // No URL key — each parse is its own entry (eval / anonymous).
            self.insert_entry(SourceEntry {
                id: SourceId(0),
                script_id: Some(event.script_id.clone()),
                frame: None,
                url,
                kind,
                source_map_url,
                is_original: false,
            })
        };

        self.rebuild_tree();
        id
    }

    /// Record a `Page.getResourceTree` reply.
    ///
    /// Must not duplicate a source already known from `scriptParsed` — the
    /// resource tree lists the page's own scripts too.
    pub fn on_resource_tree(&mut self, tree: &mjx_wk_protocol::generated::page::FrameResourceTree) {
        self.ingest_frame_tree(tree);
        self.rebuild_tree();
    }

    /// Look up by the debuggee's script id.
    pub fn by_script_id(&self, script_id: &str) -> Option<SourceId> {
        self.by_script_id.get(script_id).copied()
    }

    /// Look up by URL.
    pub fn by_url(&self, url: &str) -> Option<SourceId> {
        if url.is_empty() {
            return None;
        }
        self.by_url.get(url).copied()
    }

    /// One entry.
    pub fn get(&self, id: SourceId) -> Option<&SourceEntry> {
        self.entries.get(id.0 as usize)
    }

    /// Every entry, in discovery order.
    pub fn entries(&self) -> &[SourceEntry] {
        &self.entries
    }

    /// The display tree.
    ///
    /// Rebuilt when the inventory changes, not per frame.
    pub fn tree(&self) -> SourceTreeNode {
        self.cached_tree.clone()
    }

    /// Handle a navigation.
    ///
    /// Script ids are invalidated but entries are kept, so an open editor tab
    /// and its breakpoints survive a reload. Keeping stale script ids would
    /// make the next `getScriptSource` fail with a confusing error.
    pub fn on_navigated(&mut self) {
        for entry in &mut self.entries {
            entry.script_id = None;
        }
        self.by_script_id.clear();
        // Labels may change for eval rows that lost their script id.
        self.rebuild_tree();
    }

    fn update_script_entry(
        &mut self,
        id: SourceId,
        script_id: &str,
        kind: SourceKind,
        source_map_url: Option<String>,
    ) {
        let prev = self.entries[id.0 as usize].script_id.take();
        if let Some(prev) = prev {
            self.by_script_id.remove(&prev);
        }
        let entry = &mut self.entries[id.0 as usize];
        entry.script_id = Some(script_id.to_owned());
        entry.kind = kind;
        if source_map_url.is_some() {
            entry.source_map_url = source_map_url;
        }
        self.by_script_id.insert(script_id.to_owned(), id);
    }

    fn insert_entry(&mut self, mut entry: SourceEntry) -> SourceId {
        let id = SourceId(self.entries.len() as u32);
        entry.id = id;
        if let Some(script_id) = entry.script_id.clone() {
            self.by_script_id.insert(script_id, id);
        }
        if !entry.url.is_empty() {
            self.by_url.insert(entry.url.clone(), id);
        }
        self.entries.push(entry);
        id
    }

    fn ingest_frame_tree(&mut self, tree: &mjx_wk_protocol::generated::page::FrameResourceTree) {
        let frame_id = FrameId(tree.frame.id.clone());

        // The frame document is not always listed in `resources`; ensure it exists.
        if !tree.frame.url.is_empty() {
            self.upsert_resource(
                &tree.frame.url,
                SourceKind::Document,
                Some(frame_id.clone()),
                None,
            );
        }

        for resource in &tree.resources {
            let Some(kind) = resource_kind(resource.r#type) else {
                continue;
            };
            let source_map_url = resource.source_map_url.clone().filter(|s| !s.is_empty());
            self.upsert_resource(&resource.url, kind, Some(frame_id.clone()), source_map_url);
        }

        if let Some(children) = &tree.child_frames {
            for child in children {
                self.ingest_frame_tree(child);
            }
        }
    }

    fn upsert_resource(
        &mut self,
        url: &str,
        kind: SourceKind,
        frame: Option<FrameId>,
        source_map_url: Option<String>,
    ) {
        if url.is_empty() {
            return;
        }
        if let Some(&existing) = self.by_url.get(url) {
            let entry = &mut self.entries[existing.0 as usize];
            if frame.is_some() {
                entry.frame = frame;
            }
            if source_map_url.is_some() && entry.source_map_url.is_none() {
                entry.source_map_url = source_map_url;
            }
            // Prefer scriptParsed's richer Script flags over a bare resource-tree Script.
            if !matches!(entry.kind, SourceKind::Script { .. }) {
                entry.kind = kind;
            }
            return;
        }
        self.insert_entry(SourceEntry {
            id: SourceId(0),
            script_id: None,
            frame,
            url: url.to_owned(),
            kind,
            source_map_url,
            is_original: false,
        });
    }

    fn rebuild_tree(&mut self) {
        self.cached_tree = build_tree(&self.entries);
    }
}

fn empty_root() -> SourceTreeNode {
    SourceTreeNode::Group {
        label: String::new(),
        children: Vec::new(),
    }
}

/// Prefer `url`, then `sourceURL` (from `//# sourceURL=`), else empty.
fn effective_script_url(
    event: &mjx_wk_protocol::generated::debugger::events::ScriptParsed,
) -> String {
    if !event.url.is_empty() {
        return event.url.clone();
    }
    event
        .source_url
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn resource_kind(t: mjx_wk_protocol::generated::page::ResourceType) -> Option<SourceKind> {
    use mjx_wk_protocol::generated::page::ResourceType;
    match t {
        ResourceType::Document => Some(SourceKind::Document),
        ResourceType::StyleSheet => Some(SourceKind::StyleSheet),
        ResourceType::Script => Some(SourceKind::Script {
            module: false,
            content_script: false,
        }),
        ResourceType::Image | ResourceType::Font | ResourceType::Other => Some(SourceKind::Other),
        // Network-ish types do not belong in the Sources tree.
        ResourceType::Xhr
        | ResourceType::Fetch
        | ResourceType::Ping
        | ResourceType::Beacon
        | ResourceType::WebSocket
        | ResourceType::EventSource => None,
    }
}

fn file_name_from_url(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("data:") {
        return Some("(data)".to_owned());
    }
    if let Ok(parsed) = url::Url::parse(raw) {
        let path = parsed.path();
        let name = path.rsplit('/').next().unwrap_or("");
        if !name.is_empty() {
            return Some(name.to_owned());
        }
        // `https://example.com/` — show the host rather than a blank leaf.
        if let Some(host) = parsed.host_str() {
            return Some(host.to_owned());
        }
        return Some("/".to_owned());
    }
    let name = raw.rsplit('/').next().unwrap_or(raw);
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn origin_label(raw: &str) -> String {
    if raw.is_empty() {
        return "(no URL)".to_owned();
    }
    if raw.starts_with("data:") {
        return "(data)".to_owned();
    }
    match url::Url::parse(raw) {
        Ok(parsed) => match parsed.origin() {
            url::Origin::Opaque(_) => {
                // `file:`, `about:`, blob without host, etc.
                let mut label = parsed.scheme().to_owned();
                if let Some(host) = parsed.host_str() {
                    let _ = write!(label, "://{host}");
                }
                label
            }
            tuple => tuple.ascii_serialization(),
        },
        Err(_) => "(other)".to_owned(),
    }
}

/// Path segments under the origin, excluding the final file name.
fn path_dirs(raw: &str) -> Vec<String> {
    let Ok(parsed) = url::Url::parse(raw) else {
        return Vec::new();
    };
    let path = parsed.path();
    if path.is_empty() || path == "/" {
        return Vec::new();
    }
    let mut parts: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if !parts.is_empty() {
        parts.pop(); // file name lives on the leaf
    }
    parts
}

fn build_tree(entries: &[SourceEntry]) -> SourceTreeNode {
    #[derive(Default)]
    struct Dir {
        dirs: HashMap<String, Dir>,
        leaves: Vec<(SourceId, String)>,
    }

    impl Dir {
        fn into_nodes(mut self) -> Vec<SourceTreeNode> {
            let mut children = Vec::new();
            let mut dir_keys: Vec<String> = self.dirs.keys().cloned().collect();
            dir_keys.sort_unstable();
            for key in dir_keys {
                if let Some(dir) = self.dirs.remove(&key) {
                    children.push(SourceTreeNode::Group {
                        label: key,
                        children: dir.into_nodes(),
                    });
                }
            }
            self.leaves
                .sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
            for (id, label) in self.leaves {
                children.push(SourceTreeNode::Leaf { id, label });
            }
            children
        }
    }

    let mut origins: HashMap<String, Dir> = HashMap::new();

    for entry in entries {
        let origin = origin_label(&entry.url);
        let label = entry.display_name();
        let root = origins.entry(origin).or_default();
        let mut node = root;
        for seg in path_dirs(&entry.url) {
            node = node.dirs.entry(seg).or_default();
        }
        node.leaves.push((entry.id, label));
    }

    let mut origin_keys: Vec<String> = origins.keys().cloned().collect();
    origin_keys.sort_unstable();
    let mut children = Vec::with_capacity(origin_keys.len());
    for key in origin_keys {
        if let Some(dir) = origins.remove(&key) {
            children.push(SourceTreeNode::Group {
                label: key,
                children: dir.into_nodes(),
            });
        }
    }

    SourceTreeNode::Group {
        label: String::new(),
        children,
    }
}
