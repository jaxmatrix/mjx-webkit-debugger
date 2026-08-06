//! Pause state — `docs/tasks/T-202-pause-and-stepping.md`.
//!
//! Fixture: `fixtures/breakpoint-hit.jsonl`. Does not drive `DebugAgent`
//! (owned by T-201); it folds `Debugger.paused` through `PauseState` directly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use mjx_wk_debug::{CallFrame, PauseReason, PauseState, Scope, ScopeKind, StepKind};
use mjx_wk_protocol::generated::debugger;
use mjx_wk_protocol::generated::debugger::events::Paused;
use mjx_wk_source::{SourceId, SourceLocation};
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("crates/mjx-wk-debug → repo root")
}

fn unwrap_inner(frame: &Value) -> Value {
    if let Some(message) = frame.pointer("/params/message").and_then(Value::as_str) {
        return serde_json::from_str(message).expect("Target.* message JSON");
    }
    frame.clone()
}

/// Every `Debugger.paused` params object in the fixture, in order.
fn fixture_paused_events() -> Vec<Paused> {
    let text = std::fs::read_to_string(repo_root().join("fixtures/breakpoint-hit.jsonl"))
        .expect("read breakpoint-hit.jsonl");
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(line).expect("fixture line");
        let Some(frame) = entry.get("frame") else {
            continue;
        };
        let inner = unwrap_inner(frame);
        if inner.get("method").and_then(Value::as_str) != Some("Debugger.paused") {
            continue;
        }
        let params = inner.get("params").cloned().expect("paused params");
        out.push(serde_json::from_value(params).expect("Paused"));
    }
    out
}

fn resolve_script(script_id: &str) -> Option<SourceId> {
    script_id.parse::<u32>().ok().map(SourceId)
}

fn sample_pause() -> PauseState {
    PauseState {
        reason: PauseReason::Breakpoint,
        call_frames: vec![CallFrame {
            id: "frame-0".into(),
            function_name: "top".into(),
            location: SourceLocation {
                source: SourceId(1),
                line: 10,
                column: 0,
            },
            scopes: vec![Scope {
                kind: ScopeKind::Local,
                object_id: Some("scope-0".into()),
                name: None,
                values: None,
            }],
            this_object_id: Some("this-0".into()),
            is_blackboxed: false,
        }],
        async_stack: vec![CallFrame {
            id: "async-0".into(),
            function_name: "then".into(),
            location: SourceLocation {
                source: SourceId(2),
                line: 4,
                column: 0,
            },
            scopes: Vec::new(),
            this_object_id: Some("async-this".into()),
            is_blackboxed: false,
        }],
        selected_frame: 0,
    }
}

#[test]
fn invalidate_clears_every_remote_object_handle() {
    let mut paused = sample_pause();
    assert!(paused.call_frames[0].this_object_id.is_some());
    assert!(paused.call_frames[0].scopes[0].object_id.is_some());
    assert!(paused.async_stack[0].this_object_id.is_some());

    paused.invalidate();

    assert!(paused.call_frames[0].this_object_id.is_none());
    assert!(paused.call_frames[0].scopes[0].object_id.is_none());
    assert!(paused.call_frames[0].scopes[0].values.is_none());
    assert!(paused.async_stack[0].this_object_id.is_none());
}

#[test]
fn invalidate_is_the_handler_for_resumed_and_global_object_cleared() {
    // Both events kill every objectId; the agent must call the same method.
    // Naming the methods here pins that contract for T-201's on_event fold.
    assert!(PauseState::must_invalidate("Debugger.resumed"));
    assert!(PauseState::must_invalidate("Debugger.globalObjectCleared"));
    assert!(!PauseState::must_invalidate("Debugger.paused"));
    assert!(!PauseState::must_invalidate(
        "Runtime.executionContextCreated"
    ));
}

#[test]
fn fixture_paused_shows_stop_location_and_call_stack() {
    let events = fixture_paused_events();
    assert!(
        !events.is_empty(),
        "breakpoint-hit.jsonl must contain Debugger.paused"
    );

    let state = PauseState::from_paused(&events[0], resolve_script, |_| false);

    assert_eq!(state.reason, PauseReason::Breakpoint);
    assert_eq!(state.selected_frame, 0);
    assert_eq!(state.call_frames.len(), 2);

    let top = state.current_frame().expect("top frame");
    assert_eq!(top.function_name, "computeTotal");
    assert_eq!(
        top.location,
        SourceLocation {
            source: SourceId(2),
            line: 3,
            column: 2,
        }
    );
    assert!(top.this_object_id.is_some());
    assert!(!top.scopes.is_empty());
    assert!(matches!(top.scopes[0].kind, ScopeKind::Closure));
    assert!(top.scopes[0].object_id.is_some());

    let outer = &state.call_frames[1];
    assert_eq!(outer.function_name, "(anonymous)");
}

#[test]
fn select_frame_retargets_current_frame() {
    let events = fixture_paused_events();
    let mut state = PauseState::from_paused(&events[0], resolve_script, |_| false);
    assert_eq!(state.current_frame().unwrap().function_name, "computeTotal");

    assert!(state.select_frame(1));
    assert_eq!(state.selected_frame, 1);
    assert_ne!(state.current_frame().unwrap().id, state.call_frames[0].id);

    assert!(!state.select_frame(99));
    assert_eq!(state.selected_frame, 1);
}

#[test]
fn from_paused_marks_blackboxed_frames() {
    let events = fixture_paused_events();
    let state = PauseState::from_paused(&events[0], resolve_script, |id| id == SourceId(2));
    assert!(state.call_frames.iter().all(|f| f.is_blackboxed));
}

#[test]
fn from_paused_flattens_async_stack_trace() {
    let paused = Paused {
        call_frames: vec![debugger::CallFrame {
            call_frame_id: "sync-0".into(),
            function_name: "handler".into(),
            location: debugger::Location {
                script_id: "1".into(),
                line_number: 1,
                column_number: Some(0),
            },
            scope_chain: Vec::new(),
            this: mjx_wk_protocol::generated::runtime::RemoteObject {
                r#type: mjx_wk_protocol::generated::runtime::RemoteObjectType::Undefined,
                subtype: None,
                class_name: None,
                value: None,
                description: None,
                object_id: None,
                size: None,
                class_prototype: None,
                preview: None,
            },
            is_tail_deleted: false,
        }],
        reason: debugger::PausedReason::Other,
        data: None,
        async_stack_trace: Some(mjx_wk_protocol::generated::console::StackTrace {
            call_frames: vec![
                mjx_wk_protocol::generated::console::CallFrame {
                    function_name: "setTimeout".into(),
                    url: "https://example.com/app.js".into(),
                    script_id: "1".into(),
                    line_number: 20,
                    column_number: 0,
                },
                mjx_wk_protocol::generated::console::CallFrame {
                    function_name: "boot".into(),
                    url: "https://example.com/app.js".into(),
                    script_id: "1".into(),
                    line_number: 5,
                    column_number: 2,
                },
            ],
            top_call_frame_is_boundary: Some(true),
            truncated: None,
            parent_stack_trace: Some(Box::new(mjx_wk_protocol::generated::console::StackTrace {
                call_frames: vec![mjx_wk_protocol::generated::console::CallFrame {
                    function_name: "main".into(),
                    url: "https://example.com/app.js".into(),
                    script_id: "1".into(),
                    line_number: 0,
                    column_number: 0,
                }],
                top_call_frame_is_boundary: None,
                truncated: None,
                parent_stack_trace: None,
            })),
        }),
    };

    let state = PauseState::from_paused(&paused, resolve_script, |_| false);
    assert_eq!(state.call_frames.len(), 1);
    assert_eq!(state.async_stack.len(), 3);
    assert_eq!(state.async_stack[0].function_name, "setTimeout");
    assert_eq!(state.async_stack[1].function_name, "boot");
    assert_eq!(state.async_stack[2].function_name, "main");
    // Async frames have no live scopes — they are historical.
    assert!(state.async_stack.iter().all(|f| f.scopes.is_empty()));
    assert!(state.async_stack.iter().all(|f| f.this_object_id.is_none()));
}

#[test]
fn step_kind_wire_members_gate_webkit_only_modes() {
    assert_eq!(StepKind::Over.wire_member(), Some("stepOver"));
    assert_eq!(StepKind::Into.wire_member(), Some("stepInto"));
    assert_eq!(StepKind::Out.wire_member(), Some("stepOut"));
    assert_eq!(StepKind::Next.wire_member(), Some("stepNext"));
    assert_eq!(
        StepKind::UntilNextRunLoop.wire_member(),
        Some("continueUntilNextRunLoop")
    );
    assert_eq!(
        StepKind::ContinueTo(SourceLocation::line_start(SourceId(1), 0)).wire_member(),
        Some("continueToLocation")
    );

    assert!(StepKind::Next.is_webkit_only());
    assert!(StepKind::UntilNextRunLoop.is_webkit_only());
    assert!(!StepKind::Over.is_webkit_only());
    assert!(!StepKind::Into.is_webkit_only());
    assert!(!StepKind::Out.is_webkit_only());
}

#[test]
fn exception_pause_maps_to_protocol_state() {
    use mjx_wk_debug::pause::ExceptionPause;
    use mjx_wk_protocol::generated::debugger::SetPauseOnExceptionsState;

    assert_eq!(
        ExceptionPause::None.as_protocol(),
        SetPauseOnExceptionsState::None
    );
    assert_eq!(
        ExceptionPause::Uncaught.as_protocol(),
        SetPauseOnExceptionsState::Uncaught
    );
    assert_eq!(
        ExceptionPause::All.as_protocol(),
        SetPauseOnExceptionsState::All
    );
}
