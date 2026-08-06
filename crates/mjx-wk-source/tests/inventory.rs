//! Source inventory — merge scriptParsed + getResourceTree, stable ids, tree.
//!
//! **Owned by `docs/tasks/T-004-source-inventory.md`.**

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use mjx_wk_protocol::generated::debugger::events::ScriptParsed;
use mjx_wk_protocol::generated::page::commands::GetResourceTreeReturns;
use mjx_wk_protocol::generated::page::{Frame, FrameResource, FrameResourceTree, ResourceType};
use mjx_wk_source::{SourceInventory, SourceKind, SourceTreeNode};

fn script_parsed(script_id: &str, url: &str, module: bool, content_script: bool) -> ScriptParsed {
    ScriptParsed {
        script_id: script_id.to_owned(),
        url: url.to_owned(),
        start_line: 0,
        start_column: 0,
        end_line: 0,
        end_column: 0,
        is_content_script: Some(content_script),
        source_url: None,
        source_map_url: None,
        module: Some(module),
    }
}

fn frame_tree(frame_id: &str, frame_url: &str, resources: Vec<FrameResource>) -> FrameResourceTree {
    FrameResourceTree {
        frame: Frame {
            id: frame_id.to_owned(),
            parent_id: None,
            loader_id: "loader".to_owned(),
            name: None,
            url: frame_url.to_owned(),
            security_origin: origin_of(frame_url),
            mime_type: "text/html".to_owned(),
        },
        child_frames: None,
        resources,
    }
}

fn origin_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_else(|_| url.to_owned())
}

fn resource(url: &str, ty: ResourceType) -> FrameResource {
    FrameResource {
        url: url.to_owned(),
        r#type: ty,
        mime_type: "application/octet-stream".to_owned(),
        failed: None,
        canceled: None,
        source_map_url: None,
        target_id: None,
    }
}

fn attach_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/attach.jsonl")
}

/// Pull `Debugger.scriptParsed` events and the `Page.getResourceTree` result
/// out of the recorded attach trace (Target.*-wrapped).
fn feeds_from_attach_fixture() -> (Vec<ScriptParsed>, FrameResourceTree) {
    let text = std::fs::read_to_string(attach_fixture_path()).expect("read attach.jsonl");
    let mut scripts = Vec::new();
    let mut tree = None;

    for line in text.lines() {
        let row: serde_json::Value = serde_json::from_str(line).expect("jsonl row");
        let frame = &row["frame"];
        if frame["method"] != "Target.dispatchMessageFromTarget" {
            continue;
        }
        let message = frame["params"]["message"]
            .as_str()
            .expect("dispatch message string");
        let inner: serde_json::Value = serde_json::from_str(message).expect("inner frame");
        if inner["method"] == "Debugger.scriptParsed" {
            let event: ScriptParsed =
                serde_json::from_value(inner["params"].clone()).expect("scriptParsed");
            scripts.push(event);
        }
        if inner
            .get("result")
            .and_then(|r| r.get("frameTree"))
            .is_some()
        {
            let returns: GetResourceTreeReturns =
                serde_json::from_value(inner["result"].clone()).expect("frameTree result");
            tree = Some(returns.frame_tree);
        }
    }

    (
        scripts,
        tree.expect("attach.jsonl must contain a getResourceTree reply"),
    )
}

#[test]
fn display_name_uses_last_path_segment() {
    let mut inv = SourceInventory::new();
    let id = inv.on_script_parsed(&script_parsed(
        "1",
        "https://example.com/js/app.js",
        false,
        false,
    ));
    assert_eq!(inv.get(id).unwrap().display_name(), "app.js");
}

#[test]
fn display_name_for_eval_with_empty_url_is_usable() {
    let mut inv = SourceInventory::new();
    let id = inv.on_script_parsed(&script_parsed("9", "", false, false));
    let name = inv.get(id).unwrap().display_name();
    assert!(!name.is_empty(), "blank rows are useless");
    assert!(
        name.contains("eval"),
        "empty-URL scripts should read as eval, got {name:?}"
    );
}

#[test]
fn script_and_resource_tree_describe_the_same_file_once() {
    let mut inv = SourceInventory::new();
    let url = "http://127.0.0.1:8731/app.js";
    let id = inv.on_script_parsed(&script_parsed("2", url, false, false));
    inv.on_resource_tree(&frame_tree(
        "0.1",
        "http://127.0.0.1:8731/index.html",
        vec![
            resource(url, ResourceType::Script),
            resource("http://127.0.0.1:8731/index.html", ResourceType::Document),
            resource("http://127.0.0.1:8731/style.css", ResourceType::StyleSheet),
        ],
    ));

    let script_hits: Vec<_> = inv.entries().iter().filter(|e| e.url == url).collect();
    assert_eq!(script_hits.len(), 1, "must not duplicate the script");
    assert_eq!(script_hits[0].id, id);
    assert_eq!(script_hits[0].script_id.as_deref(), Some("2"));
    assert!(
        script_hits[0].frame.is_some(),
        "resource tree supplies frameId"
    );
    assert!(inv.by_url(url).is_some());
    assert_eq!(inv.by_script_id("2"), Some(id));

    // Document + stylesheet + script = 3 distinct URLs.
    assert_eq!(inv.entries().len(), 3);
}

#[test]
fn source_id_for_a_url_is_stable_across_navigation() {
    let mut inv = SourceInventory::new();
    let url = "https://example.com/app.js";
    let before = inv.on_script_parsed(&script_parsed("1", url, false, false));
    assert_eq!(inv.get(before).unwrap().script_id.as_deref(), Some("1"));

    inv.on_navigated();
    assert!(inv.by_script_id("1").is_none());
    assert_eq!(
        inv.get(before).unwrap().script_id,
        None,
        "stale script ids must not survive navigation"
    );
    assert_eq!(inv.by_url(url), Some(before));

    let after = inv.on_script_parsed(&script_parsed("99", url, false, false));
    assert_eq!(after, before, "URL keeps its SourceId across reload");
    assert_eq!(inv.get(after).unwrap().script_id.as_deref(), Some("99"));
    assert_eq!(inv.by_script_id("99"), Some(after));
}

#[test]
fn tree_groups_by_origin_and_sorts_stably() {
    let mut inv = SourceInventory::new();
    inv.on_script_parsed(&script_parsed("1", "https://b.example/z.js", false, false));
    inv.on_script_parsed(&script_parsed(
        "2",
        "https://a.example/lib/a.js",
        false,
        false,
    ));
    inv.on_script_parsed(&script_parsed(
        "3",
        "https://a.example/lib/b.js",
        false,
        false,
    ));
    inv.on_script_parsed(&script_parsed(
        "4",
        "https://a.example/root.js",
        false,
        false,
    ));

    let SourceTreeNode::Group {
        label: root_label,
        children: origins,
    } = inv.tree()
    else {
        panic!("root must be a group");
    };
    assert!(root_label.is_empty());
    assert_eq!(origins.len(), 2);

    let SourceTreeNode::Group {
        label: first_origin,
        children: a_children,
    } = &origins[0]
    else {
        panic!("expected origin group");
    };
    assert_eq!(first_origin, "https://a.example");

    // Under a.example: dir "lib" then leaf "root.js", dirs before leaves, sorted.
    assert_eq!(a_children.len(), 2);
    match &a_children[0] {
        SourceTreeNode::Group { label, children } => {
            assert_eq!(label, "lib");
            let labels: Vec<_> = children
                .iter()
                .map(|n| match n {
                    SourceTreeNode::Leaf { label, .. } => label.as_str(),
                    SourceTreeNode::Group { label, .. } => label.as_str(),
                })
                .collect();
            assert_eq!(labels, ["a.js", "b.js"]);
        }
        other => panic!("expected lib group, got {other:?}"),
    }
    match &a_children[1] {
        SourceTreeNode::Leaf { label, .. } => assert_eq!(label, "root.js"),
        other => panic!("expected root.js leaf, got {other:?}"),
    }

    let SourceTreeNode::Group {
        label: second_origin,
        ..
    } = &origins[1]
    else {
        panic!("expected origin group");
    };
    assert_eq!(second_origin, "https://b.example");
}

#[test]
fn attach_fixture_merges_script_parsed_and_resource_tree() {
    let (scripts, tree) = feeds_from_attach_fixture();
    assert!(
        !scripts.is_empty(),
        "fixture should carry at least one scriptParsed"
    );

    let mut inv = SourceInventory::new();
    for event in &scripts {
        inv.on_script_parsed(event);
    }
    inv.on_resource_tree(&tree);

    let app = inv
        .by_url("http://127.0.0.1:8731/app.js")
        .expect("app.js present");
    let entry = inv.get(app).unwrap();
    assert!(matches!(entry.kind, SourceKind::Script { .. }));
    assert_eq!(entry.script_id.as_deref(), Some("2"));
    assert_eq!(entry.frame.as_ref().map(|f| f.0.as_str()), Some("0.1"));

    assert!(inv.by_url("http://127.0.0.1:8731/index.html").is_some());
    assert!(inv.by_url("http://127.0.0.1:8731/style.css").is_some());

    // One entry per URL — script must not appear twice.
    let urls: Vec<_> = inv.entries().iter().map(|e| e.url.as_str()).collect();
    assert_eq!(
        urls.iter()
            .filter(|u| **u == "http://127.0.0.1:8731/app.js")
            .count(),
        1
    );
    assert_eq!(inv.entries().len(), 3);

    let tree = inv.tree();
    let SourceTreeNode::Group { children, .. } = tree else {
        panic!("root group");
    };
    assert_eq!(children.len(), 1);
    let SourceTreeNode::Group { label, children } = &children[0] else {
        panic!("origin group");
    };
    assert_eq!(label, "http://127.0.0.1:8731");
    let labels: Vec<_> = children
        .iter()
        .map(|n| match n {
            SourceTreeNode::Leaf { label, .. } => label.clone(),
            SourceTreeNode::Group { label, .. } => label.clone(),
        })
        .collect();
    assert_eq!(labels, vec!["app.js", "index.html", "style.css"]);
}
