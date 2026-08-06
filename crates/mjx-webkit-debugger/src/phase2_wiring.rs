//! Phase 2 shell wiring — fixture flatten + agent registration smoke tests.
//!
//! **Owned by Phase 2 shell wiring (app host).**

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use mjx_wk_console::ConsoleAgent;
    use mjx_wk_debug::DebugAgent;
    use mjx_wk_dialect::{DialectKind, WebKitDialect};
    use mjx_wk_protocol::TargetType;
    use mjx_wk_session::{AgentRegistry, Session};
    use mjx_wk_transport::{ReplayTransport, Target, TargetKey, TransportOrigin};

    use crate::fixture_seed;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name)
    }

    #[test]
    fn flatten_multiplexed_breakpoint_hit_exposes_bare_inspector_enable() {
        let path = fixture("breakpoint-hit.jsonl");
        assert!(fixture_seed::is_multiplexed_fixture(&path));
        let flat = fixture_seed::flatten_multiplexed_trace(&path).expect("flatten");
        assert!(
            flat.contains("\"method\":\"Inspector.enable\""),
            "flattened trace must expose bare Inspector.enable for Session::attach"
        );
        assert!(
            !flat.contains("Target.sendMessageToTarget"),
            "flatten must unwrap page-target sends"
        );
        assert!(flat.contains("Console.messageAdded") || flat.contains("Debugger.paused"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn register_debug_and_console_against_flattened_breakpoint_hit() {
        let path = fixture("breakpoint-hit.jsonl");
        let flat = fixture_seed::flatten_multiplexed_trace(&path).expect("flatten");
        let transport = ReplayTransport::from_str(&flat, "breakpoint-hit-flat").expect("transport");
        let target = Target {
            key: TargetKey("replay/0".into()),
            name: "breakpoint-hit".into(),
            url: "http://127.0.0.1:8731/index.html".into(),
            kind: TargetType::WebPage,
            dialect: DialectKind::WebKitRwi,
            origin: TransportOrigin::Replay {
                fixture: "breakpoint-hit.jsonl".into(),
            },
        };
        let session = Session::attach(Box::new(transport), Box::new(WebKitDialect), target)
            .await
            .expect("attach");

        let mut registry = AgentRegistry::new();
        // Console.enable is earlier in the fixture than Debugger.enable.
        let console = registry
            .register(ConsoleAgent::default(), &session)
            .await
            .expect("register console")
            .expect("console attached");
        let debug = registry
            .register(DebugAgent::default(), &session)
            .await
            .expect("register debug")
            .expect("debug attached");

        assert!(registry.active().contains(&"debug"));
        assert!(registry.active().contains(&"mjx-wk-console"));

        session
            .call(mjx_wk_protocol::generated::debugger::commands::SetBreakpointByUrl {
                line_number: 3,
                url: None,
                url_regex: Some(r".*app\.js".into()),
                column_number: Some(0),
                options: None,
            })
            .await
            .expect("setBreakpointByUrl");
        session
            .call(mjx_wk_protocol::generated::page::commands::Reload {
                ignore_cache: None,
                revalidate_all_resources: None,
            })
            .await
            .expect("reload");

        for _ in 0..100 {
            if debug.load().paused.is_some() || !console.load().messages.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        assert!(
            !console.load().messages.is_empty() || debug.load().paused.is_some(),
            "agents must fold console messages and/or a pause from the fixture"
        );
    }
}
