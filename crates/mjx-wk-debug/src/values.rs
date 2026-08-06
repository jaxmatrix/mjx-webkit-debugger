//! Inspecting values in a paused frame.
//!
//! **Owned by `docs/tasks/T-203-variable-tree.md`.**
//!
//! # Two constraints shape this entirely
//!
//! **Expansion is lazy.** A global scope has thousands of properties and any
//! object may have millions. Nothing is fetched until a row is opened.
//!
//! **`Runtime.getProperties` is paginated on WebKit** — `fetchStart` and
//! `fetchCount`, which CDP does not have. Ignoring that and asking for
//! everything is how a debugger hangs on a large array.

use std::ops::Range;

use mjx_wk_protocol::generated::runtime::{
    InternalPropertyDescriptor, PropertyDescriptor, RemoteObject, RemoteObjectSubtype,
    RemoteObjectType,
};

/// Default `fetchCount` for one expansion page.
///
/// Matches the WebKit inspector front-end and `fixtures/breakpoint-hit.jsonl`.
pub const PAGE_SIZE: u32 = 100;

/// A node in the variable tree, local to this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueNodeId(pub u32);

/// A short rendering of a value, shown without expanding it.
///
/// Comes from the protocol's `generatePreview`, which is why previews are free:
/// the debuggee builds them while answering.
#[derive(Debug, Clone, PartialEq)]
pub struct ValuePreview {
    /// e.g. `"object"`, `"string"`, `"function"`.
    pub type_name: String,
    /// e.g. `"array"`, `"null"`, `"regexp"`.
    pub subtype: Option<String>,
    /// What to render: `"Array(3)"`, `"\"hello\""`, `"undefined"`.
    pub description: String,
    /// Whether opening this would show anything.
    pub has_children: bool,
}

impl ValuePreview {
    /// Preview for a getter that has not been invoked.
    ///
    /// Shown as `(...)` so the user must opt in — invoking a getter can have
    /// side effects.
    pub fn accessor() -> Self {
        Self {
            type_name: "accessor".into(),
            subtype: None,
            description: "(...)".into(),
            has_children: false,
        }
    }

    /// Build a preview from a protocol mirror object.
    pub fn from_remote(obj: &RemoteObject) -> Self {
        let type_name = remote_type_name(obj.r#type).to_owned();
        let subtype = obj.subtype.map(remote_subtype_name).map(str::to_owned);
        let description = preview_description(obj);
        let has_children = remote_has_children(obj);
        Self {
            type_name,
            subtype,
            description,
            has_children,
        }
    }
}

/// One row.
#[derive(Debug, Clone)]
pub struct ValueNode {
    pub id: ValueNodeId,
    pub name: String,
    pub preview: ValuePreview,
    /// The remote handle, for expanding. **Invalid after resume.**
    pub object_id: Option<String>,
    /// `None` until expanded.
    pub children: Option<Vec<ValueNodeId>>,
    /// Which slice of the properties has been fetched so far.
    pub fetched: Range<u32>,
    /// A getter that has not been invoked. Shown as `(...)`, because invoking
    /// it could have side effects and the user must opt in.
    pub is_accessor: bool,
    /// An own property rather than an inherited one.
    pub is_own: bool,
}

/// Extra bookkeeping that must not clutter the public row.
#[derive(Debug, Clone, Default)]
struct NodeMeta {
    /// Last page returned a full `fetchCount` — more may exist.
    may_have_more: bool,
    /// From `RemoteObject.size` when the debuggee told us.
    known_total: Option<u32>,
    /// Containing object + getter handle, for opt-in accessor invocation.
    accessor: Option<AccessorRef>,
}

/// What the session needs to invoke a getter via `Runtime.callFunctionOn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessorRef {
    /// Object that owns the accessor property.
    pub holder_object_id: String,
    /// The getter function's remote object id.
    pub getter_object_id: String,
}

/// A lazily expanded tree of values.
#[derive(Debug, Clone, Default)]
pub struct ValueTree {
    nodes: Vec<ValueNode>,
    meta: Vec<NodeMeta>,
    roots: Vec<ValueNodeId>,
    /// Root ids that represent watch expressions (subset of `roots` order).
    watch_root_count: usize,
}

impl ValueTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            meta: Vec::new(),
            roots: Vec::new(),
            watch_root_count: 0,
        }
    }

    /// One node.
    pub fn get(&self, id: ValueNodeId) -> Option<&ValueNode> {
        self.nodes.get(id.0 as usize)
    }

    /// The top-level rows.
    pub fn roots(&self) -> &[ValueNodeId] {
        &self.roots
    }

    /// Roots that are watch expressions (always a prefix of [`Self::roots`]).
    pub fn watch_roots(&self) -> &[ValueNodeId] {
        self.roots.get(..self.watch_root_count).unwrap_or(&[])
    }

    /// Scope / object roots after the watch prefix.
    pub fn scope_roots(&self) -> &[ValueNodeId] {
        self.roots.get(self.watch_root_count..).unwrap_or(&[])
    }

    /// Whether a node needs fetching before it can be shown open.
    pub fn needs_fetch(&self, id: ValueNodeId) -> bool {
        let Some(node) = self.get(id) else {
            return false;
        };
        if node.is_accessor {
            // Still showing `(...)` — user has not opted in.
            return true;
        }
        node.preview.has_children && node.children.is_none()
    }

    /// How many more properties exist beyond what has been fetched.
    ///
    /// Drives the "Show more" row an object with 50 000 keys needs.
    pub fn remaining(&self, id: ValueNodeId) -> Option<u32> {
        let node = self.get(id)?;
        node.children.as_ref()?;
        let meta = self.meta.get(id.0 as usize)?;
        if let Some(total) = meta.known_total {
            let left = total.saturating_sub(node.fetched.end);
            return (left > 0).then_some(left);
        }
        if meta.may_have_more {
            // Unknown total — advertise at least another page.
            return Some(PAGE_SIZE);
        }
        None
    }

    /// Accessor invocation handles, when this row is still an uninvoked getter.
    pub fn accessor_ref(&self, id: ValueNodeId) -> Option<&AccessorRef> {
        self.meta.get(id.0 as usize)?.accessor.as_ref()
    }

    /// Drop every node. Call on `Debugger.resumed` and
    /// `Debugger.globalObjectCleared` — every `objectId` is dead.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.meta.clear();
        self.roots.clear();
        self.watch_root_count = 0;
    }

    /// Append an unexpanded object root (a scope, `this`, etc.).
    ///
    /// Nothing is fetched here — that waits for an expand.
    pub fn push_root(
        &mut self,
        name: impl Into<String>,
        object_id: Option<String>,
        preview: ValuePreview,
    ) -> ValueNodeId {
        let id = self.alloc(
            name.into(),
            preview,
            object_id,
            /*is_accessor*/ false,
            /*is_own*/ true,
            None,
        );
        self.roots.push(id);
        id
    }

    /// Replace watch-expression roots with freshly evaluated results.
    ///
    /// Call on every pause and step so watches never keep a stale `objectId`.
    /// Prior watch roots are detached (scope roots and their subgraphs stay).
    pub fn set_watch_roots<I>(&mut self, watches: I)
    where
        I: IntoIterator<Item = WatchResult>,
    {
        let scope_roots: Vec<ValueNodeId> = self.scope_roots().to_vec();
        self.roots.clear();
        self.watch_root_count = 0;

        for watch in watches {
            let (preview, object_id, is_accessor, accessor) = match watch.value {
                WatchValue::Ready(obj) => {
                    let preview = ValuePreview::from_remote(&obj);
                    let object_id = obj.object_id.clone();
                    (preview, object_id, false, None)
                }
                WatchValue::Accessor { holder, getter } => (
                    ValuePreview::accessor(),
                    None,
                    true,
                    Some(AccessorRef {
                        holder_object_id: holder,
                        getter_object_id: getter,
                    }),
                ),
                WatchValue::Unavailable(msg) => (
                    ValuePreview {
                        type_name: "error".into(),
                        subtype: None,
                        description: msg,
                        has_children: false,
                    },
                    None,
                    false,
                    None,
                ),
            };
            let id = self.alloc(
                watch.expression,
                preview,
                object_id,
                is_accessor,
                true,
                accessor,
            );
            self.roots.push(id);
        }
        self.watch_root_count = self.roots.len();
        self.roots.extend(scope_roots);
    }

    /// Apply one page of `Runtime.getProperties` to an expanded node.
    ///
    /// `start` must equal the node's current `fetched.end` (or `0` on first
    /// fetch). `page_size` is the `fetchCount` that was requested — a short
    /// page means the object is exhausted.
    pub fn apply_properties(
        &mut self,
        id: ValueNodeId,
        start: u32,
        page_size: u32,
        properties: &[PropertyDescriptor],
        internal: &[InternalPropertyDescriptor],
        holder_object_id: Option<&str>,
    ) {
        let Some(node) = self.nodes.get(id.0 as usize) else {
            return;
        };
        if node.is_accessor {
            return;
        }
        if start != node.fetched.end && !(start == 0 && node.children.is_none()) {
            return;
        }

        let mut child_ids = if start == 0 {
            Vec::new()
        } else {
            node.children.clone().unwrap_or_default()
        };

        if start == 0 {
            for prop in internal {
                let preview = prop
                    .value
                    .as_ref()
                    .map(ValuePreview::from_remote)
                    .unwrap_or_else(|| ValuePreview {
                        type_name: "undefined".into(),
                        subtype: None,
                        description: "undefined".into(),
                        has_children: false,
                    });
                let object_id = prop.value.as_ref().and_then(|v| v.object_id.clone());
                let child = self.alloc(prop.name.clone(), preview, object_id, false, true, None);
                child_ids.push(child);
            }
        }

        for prop in properties {
            let (preview, object_id, is_accessor, accessor) =
                descriptor_to_row(prop, holder_object_id);
            let child = self.alloc(
                prop.name.clone(),
                preview,
                object_id,
                is_accessor,
                prop.is_own.unwrap_or(true),
                accessor,
            );
            child_ids.push(child);
        }

        let fetched_end = start.saturating_add(properties.len() as u32);
        let may_have_more = page_size > 0 && (properties.len() as u32) >= page_size;

        if let Some(node) = self.nodes.get_mut(id.0 as usize) {
            node.children = Some(child_ids);
            node.fetched = 0..fetched_end;
            // Once expanded, disclosure follows remaining pages / children.
            node.preview.has_children = may_have_more || node.fetched.end > 0;
        }
        if let Some(meta) = self.meta.get_mut(id.0 as usize) {
            meta.may_have_more = may_have_more;
        }
    }

    /// Record a known property total (e.g. from `RemoteObject.size`).
    pub fn set_known_total(&mut self, id: ValueNodeId, total: u32) {
        if let Some(meta) = self.meta.get_mut(id.0 as usize) {
            meta.known_total = Some(total);
        }
    }

    /// Replace an accessor's `(...)` with the invoked value.
    pub fn apply_getter_result(&mut self, id: ValueNodeId, value: &RemoteObject) {
        let Some(node) = self.nodes.get_mut(id.0 as usize) else {
            return;
        };
        if !node.is_accessor {
            return;
        }
        node.preview = ValuePreview::from_remote(value);
        node.object_id = value.object_id.clone();
        node.is_accessor = false;
        node.children = None;
        node.fetched = 0..0;
        if let Some(meta) = self.meta.get_mut(id.0 as usize) {
            meta.accessor = None;
            meta.may_have_more = false;
            meta.known_total = value.size.and_then(|s| u32::try_from(s).ok());
        }
    }

    fn alloc(
        &mut self,
        name: String,
        preview: ValuePreview,
        object_id: Option<String>,
        is_accessor: bool,
        is_own: bool,
        accessor: Option<AccessorRef>,
    ) -> ValueNodeId {
        let id = ValueNodeId(self.nodes.len() as u32);
        self.nodes.push(ValueNode {
            id,
            name,
            preview,
            object_id,
            children: None,
            fetched: 0..0,
            is_accessor,
            is_own,
        });
        self.meta.push(NodeMeta {
            may_have_more: false,
            known_total: None,
            accessor,
        });
        id
    }
}

/// One evaluated watch expression.
#[derive(Debug, Clone)]
pub struct WatchResult {
    pub expression: String,
    pub value: WatchValue,
}

/// Outcome of evaluating a watch.
#[derive(Debug, Clone)]
pub enum WatchValue {
    Ready(Box<RemoteObject>),
    /// Still an uninvoked getter — rare for watches, but keep the opt-in path.
    Accessor {
        holder: String,
        getter: String,
    },
    Unavailable(String),
}

fn descriptor_to_row(
    prop: &PropertyDescriptor,
    holder_object_id: Option<&str>,
) -> (ValuePreview, Option<String>, bool, Option<AccessorRef>) {
    if let Some(value) = &prop.value {
        return (
            ValuePreview::from_remote(value),
            value.object_id.clone(),
            false,
            None,
        );
    }
    if let Some(getter) = &prop.get {
        // A getter with no value yet — do not invoke.
        let accessor = match (holder_object_id, getter.object_id.as_ref()) {
            (Some(holder), Some(gid)) => Some(AccessorRef {
                holder_object_id: holder.to_owned(),
                getter_object_id: gid.clone(),
            }),
            _ => None,
        };
        return (ValuePreview::accessor(), None, true, accessor);
    }
    (
        ValuePreview {
            type_name: "undefined".into(),
            subtype: None,
            description: "undefined".into(),
            has_children: false,
        },
        None,
        false,
        None,
    )
}

fn remote_type_name(t: RemoteObjectType) -> &'static str {
    match t {
        RemoteObjectType::Object => "object",
        RemoteObjectType::Function => "function",
        RemoteObjectType::Undefined => "undefined",
        RemoteObjectType::String => "string",
        RemoteObjectType::Number => "number",
        RemoteObjectType::Boolean => "boolean",
        RemoteObjectType::Symbol => "symbol",
        RemoteObjectType::Bigint => "bigint",
    }
}

fn remote_subtype_name(s: RemoteObjectSubtype) -> &'static str {
    match s {
        RemoteObjectSubtype::Array => "array",
        RemoteObjectSubtype::Null => "null",
        RemoteObjectSubtype::Node => "node",
        RemoteObjectSubtype::Regexp => "regexp",
        RemoteObjectSubtype::Date => "date",
        RemoteObjectSubtype::Error => "error",
        RemoteObjectSubtype::Map => "map",
        RemoteObjectSubtype::Set => "set",
        RemoteObjectSubtype::Weakmap => "weakmap",
        RemoteObjectSubtype::Weakset => "weakset",
        RemoteObjectSubtype::Iterator => "iterator",
        RemoteObjectSubtype::Class => "class",
        RemoteObjectSubtype::Proxy => "proxy",
        RemoteObjectSubtype::Weakref => "weakref",
    }
}

fn preview_description(obj: &RemoteObject) -> String {
    if let Some(desc) = &obj.description
        && !desc.is_empty()
    {
        return desc.clone();
    }
    match obj.r#type {
        RemoteObjectType::String => match &obj.value {
            Some(serde_json::Value::String(s)) => format!("\"{s}\""),
            _ => "\"\"".into(),
        },
        RemoteObjectType::Number | RemoteObjectType::Boolean | RemoteObjectType::Bigint => obj
            .value
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| remote_type_name(obj.r#type).into()),
        RemoteObjectType::Undefined => "undefined".into(),
        RemoteObjectType::Object if obj.subtype == Some(RemoteObjectSubtype::Null) => "null".into(),
        RemoteObjectType::Function => obj.class_name.clone().unwrap_or_else(|| "function".into()),
        _ => obj
            .class_name
            .clone()
            .unwrap_or_else(|| remote_type_name(obj.r#type).into()),
    }
}

fn remote_has_children(obj: &RemoteObject) -> bool {
    match obj.r#type {
        RemoteObjectType::Object => {
            obj.subtype != Some(RemoteObjectSubtype::Null) && obj.object_id.is_some()
        }
        RemoteObjectType::Function => obj.object_id.is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_object(
        ty: RemoteObjectType,
        subtype: Option<RemoteObjectSubtype>,
        description: &str,
        object_id: Option<&str>,
        size: Option<i64>,
    ) -> RemoteObject {
        RemoteObject {
            r#type: ty,
            subtype,
            class_name: None,
            value: None,
            description: Some(description.into()),
            object_id: object_id.map(str::to_owned),
            size,
            class_prototype: None,
            preview: None,
        }
    }

    fn prop(name: &str, value: RemoteObject) -> PropertyDescriptor {
        PropertyDescriptor {
            name: name.into(),
            value: Some(value),
            writable: Some(true),
            get: None,
            set: None,
            was_thrown: None,
            configurable: Some(true),
            enumerable: Some(true),
            is_own: Some(true),
            symbol: None,
            is_private: None,
            native_getter: None,
        }
    }

    fn accessor_prop(name: &str, getter_id: &str) -> PropertyDescriptor {
        PropertyDescriptor {
            name: name.into(),
            value: None,
            writable: None,
            get: Some(remote_object(
                RemoteObjectType::Function,
                None,
                "function",
                Some(getter_id),
                None,
            )),
            set: None,
            was_thrown: None,
            configurable: Some(true),
            enumerable: Some(true),
            is_own: Some(true),
            symbol: None,
            is_private: None,
            native_getter: None,
        }
    }

    #[test]
    fn nothing_is_fetched_until_expanded() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "Local",
            Some("scope-1".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        assert!(tree.needs_fetch(root));
        assert!(tree.get(root).unwrap().children.is_none());
        assert!(tree.remaining(root).is_none());
    }

    #[test]
    fn expansion_pages_and_reports_remaining() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "arr",
            Some("arr-1".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: Some("array".into()),
                description: "Array(250)".into(),
                has_children: true,
            },
        );
        tree.set_known_total(root, 250);

        let page: Vec<_> = (0..PAGE_SIZE)
            .map(|i| {
                prop(
                    &i.to_string(),
                    remote_object(RemoteObjectType::Number, None, &i.to_string(), None, None),
                )
            })
            .collect();

        tree.apply_properties(root, 0, PAGE_SIZE, &page, &[], Some("arr-1"));
        assert!(!tree.needs_fetch(root));
        assert_eq!(tree.remaining(root), Some(150));
        assert_eq!(tree.get(root).unwrap().fetched, 0..PAGE_SIZE);
        assert_eq!(
            tree.get(root).unwrap().children.as_ref().unwrap().len(),
            PAGE_SIZE as usize
        );

        let page2: Vec<_> = (PAGE_SIZE..PAGE_SIZE + 50)
            .map(|i| {
                prop(
                    &i.to_string(),
                    remote_object(RemoteObjectType::Number, None, &i.to_string(), None, None),
                )
            })
            .collect();
        tree.apply_properties(root, PAGE_SIZE, PAGE_SIZE, &page2, &[], Some("arr-1"));
        assert_eq!(tree.remaining(root), Some(100));
        assert_eq!(tree.get(root).unwrap().fetched.end, 150);

        let page3: Vec<_> = (150..250)
            .map(|i| {
                prop(
                    &i.to_string(),
                    remote_object(RemoteObjectType::Number, None, &i.to_string(), None, None),
                )
            })
            .collect();
        tree.apply_properties(root, 150, PAGE_SIZE, &page3, &[], Some("arr-1"));
        assert!(tree.remaining(root).is_none());
        assert_eq!(tree.get(root).unwrap().fetched.end, 250);
    }

    #[test]
    fn short_page_without_known_total_exhausts() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "obj",
            Some("o-1".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        let page = vec![prop(
            "a",
            remote_object(RemoteObjectType::Number, None, "1", None, None),
        )];
        tree.apply_properties(root, 0, PAGE_SIZE, &page, &[], Some("o-1"));
        assert!(tree.remaining(root).is_none());
    }

    #[test]
    fn full_page_without_known_total_offers_show_more() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "obj",
            Some("o-1".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        let page: Vec<_> = (0..PAGE_SIZE)
            .map(|i| {
                prop(
                    &format!("k{i}"),
                    remote_object(RemoteObjectType::Number, None, "0", None, None),
                )
            })
            .collect();
        tree.apply_properties(root, 0, PAGE_SIZE, &page, &[], Some("o-1"));
        assert_eq!(tree.remaining(root), Some(PAGE_SIZE));
    }

    #[test]
    fn getter_stays_opt_in_until_applied() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "obj",
            Some("holder".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        tree.apply_properties(
            root,
            0,
            PAGE_SIZE,
            &[accessor_prop("dangerous", "getter-1")],
            &[],
            Some("holder"),
        );
        let child = tree.get(root).unwrap().children.as_ref().unwrap()[0];
        let node = tree.get(child).unwrap();
        assert!(node.is_accessor);
        assert_eq!(node.preview.description, "(...)");
        assert!(tree.needs_fetch(child));
        let acc = tree.accessor_ref(child).unwrap();
        assert_eq!(acc.holder_object_id, "holder");
        assert_eq!(acc.getter_object_id, "getter-1");

        // Still (...) until the session applies an explicit invoke result.
        assert!(tree.get(child).unwrap().is_accessor);

        tree.apply_getter_result(
            child,
            &remote_object(RemoteObjectType::Number, None, "42", None, None),
        );
        let node = tree.get(child).unwrap();
        assert!(!node.is_accessor);
        assert_eq!(node.preview.description, "42");
        assert!(tree.accessor_ref(child).is_none());
        assert!(!tree.needs_fetch(child));
    }

    #[test]
    fn clear_drops_the_whole_tree() {
        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "Local",
            Some("s".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        tree.apply_properties(
            root,
            0,
            PAGE_SIZE,
            &[prop(
                "x",
                remote_object(RemoteObjectType::Number, None, "1", None, None),
            )],
            &[],
            Some("s"),
        );
        assert!(!tree.roots().is_empty());
        tree.clear();
        assert!(tree.roots().is_empty());
        assert!(tree.get(root).is_none());
    }

    #[test]
    fn watch_roots_replace_on_reeval() {
        let mut tree = ValueTree::new();
        tree.push_root(
            "Local",
            Some("scope".into()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "Object".into(),
                has_children: true,
            },
        );
        tree.set_watch_roots([WatchResult {
            expression: "a + 1".into(),
            value: WatchValue::Ready(Box::new(remote_object(
                RemoteObjectType::Number,
                None,
                "2",
                None,
                None,
            ))),
        }]);
        assert_eq!(tree.watch_roots().len(), 1);
        assert_eq!(tree.scope_roots().len(), 1);
        assert_eq!(tree.get(tree.watch_roots()[0]).unwrap().name, "a + 1");

        tree.set_watch_roots([WatchResult {
            expression: "a + 1".into(),
            value: WatchValue::Ready(Box::new(remote_object(
                RemoteObjectType::Number,
                None,
                "3",
                None,
                None,
            ))),
        }]);
        assert_eq!(
            tree.get(tree.watch_roots()[0]).unwrap().preview.description,
            "3"
        );
        assert_eq!(tree.scope_roots().len(), 1);
    }

    #[test]
    fn fixture_get_properties_page_applies() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/breakpoint-hit.jsonl");
        let text = std::fs::read_to_string(&fixture).expect("fixture");

        let mut object_id = None;
        let mut properties = None;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: serde_json::Value = serde_json::from_str(line).unwrap();
            let Some(frame) = entry.get("frame") else {
                continue;
            };
            let inner = if let Some(msg) = frame.pointer("/params/message").and_then(|v| v.as_str())
            {
                serde_json::from_str::<serde_json::Value>(msg).unwrap()
            } else {
                frame.clone()
            };
            if inner.get("method").and_then(|m| m.as_str()) == Some("Runtime.getProperties") {
                object_id = inner
                    .pointer("/params/objectId")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let start = inner
                    .pointer("/params/fetchStart")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let count = inner
                    .pointer("/params/fetchCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                assert_eq!(start, 0);
                assert_eq!(count, u64::from(PAGE_SIZE));
            }
            if properties.is_none()
                && inner
                    .pointer("/result/properties")
                    .and_then(|v| v.as_array())
                    .is_some()
            {
                properties = Some(inner["result"].clone());
            }
        }

        let object_id = object_id.expect("fixture sends getProperties");
        let result = properties.expect("fixture replies with properties");
        let props: Vec<PropertyDescriptor> =
            serde_json::from_value(result["properties"].clone()).unwrap();
        let internal: Vec<InternalPropertyDescriptor> = result
            .get("internalProperties")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .unwrap()
            .unwrap_or_default();

        let mut tree = ValueTree::new();
        let root = tree.push_root(
            "closure",
            Some(object_id.clone()),
            ValuePreview {
                type_name: "object".into(),
                subtype: None,
                description: "DebuggerScope".into(),
                has_children: true,
            },
        );
        assert!(tree.needs_fetch(root));
        tree.apply_properties(root, 0, PAGE_SIZE, &props, &internal, Some(&object_id));
        assert!(!tree.needs_fetch(root));
        assert!(tree.remaining(root).is_none());
        let children = tree.get(root).unwrap().children.as_ref().unwrap();
        assert_eq!(children.len(), props.len() + internal.len());
        assert_eq!(tree.get(children[0]).unwrap().name, "total");
    }
}
