//! L4 — console messages and expression evaluation.
//!
//! **Phase 2.**
//!
//! # Evaluation routes two ways
//!
//! When execution is paused, an expression must go to
//! `Debugger.evaluateOnCallFrame` so it sees the local scope; otherwise to
//! `Runtime.evaluate`. Sending everything to `Runtime.evaluate` is the common
//! mistake, and it makes the console useless at exactly the moment it matters —
//! you stop at a breakpoint and cannot see the local variable you stopped for.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::generated::console as proto_console;
use mjx_wk_protocol::generated::debugger as proto_debugger;
use mjx_wk_protocol::generated::runtime as proto_runtime;
use mjx_wk_protocol::generated::runtime::RemoteObject;
use mjx_wk_protocol::{Domain, Frame};
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};
use serde_json::Value;

/// Hard cap on retained messages. A busy page can log without limit; the
/// debugger must not grow alongside it. Dropped count is reported so the UI can
/// say what was lost rather than silently rewriting history.
pub const MESSAGE_CAPACITY: usize = 1_000;

/// Object group used for console evaluations so detach can release them.
const CONSOLE_OBJECT_GROUP: &str = "console";

/// Where a message came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSource {
    Javascript,
    Network,
    ConsoleApi,
    Storage,
    Appcache,
    Rendering,
    Css,
    Security,
    Other,
}

/// How loud a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageLevel {
    Debug,
    Log,
    Info,
    Warning,
    Error,
}

/// One console message.
#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub source: MessageSource,
    pub level: MessageLevel,
    pub text: String,
    /// Remote handles for the arguments, so objects stay expandable.
    pub argument_object_ids: Vec<String>,
    pub location: Option<mjx_wk_source::SourceLocation>,
    /// Collapsed repeats, from `Console.messageRepeatCountUpdated`. A message
    /// logged in a render loop must not push everything else off screen.
    pub repeat_count: u32,
}

/// The console log.
#[derive(Debug, Clone, Default)]
pub struct ConsoleModel {
    /// Bounded: a page can log without limit, and the debugger must not grow
    /// without limit alongside it.
    pub messages: Vec<ConsoleMessage>,
    /// Messages dropped to stay within the bound, so the UI can say so rather
    /// than silently losing history.
    pub dropped: u64,
}

impl ConsoleModel {
    /// An empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a message, dropping the oldest when over capacity.
    pub fn push(&mut self, message: ConsoleMessage) {
        if self.messages.len() >= MESSAGE_CAPACITY {
            self.messages.remove(0);
            self.dropped = self.dropped.saturating_add(1);
        }
        self.messages.push(message);
    }

    /// Update the repeat count on the most recent message.
    ///
    /// WebKit sends `messageRepeatCountUpdated` instead of flooding identical
    /// `messageAdded` events. No-op when the log is empty.
    pub fn set_last_repeat_count(&mut self, count: u32) {
        if let Some(last) = self.messages.last_mut() {
            last.repeat_count = count.max(1);
        }
    }

    /// Drop every retained message. Does not reset [`Self::dropped`] — that
    /// count is "lost to the bound", not "cleared by the page".
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Record a user evaluation as a pair of input / output rows.
    pub fn record_evaluation(&mut self, expression: &str, evaluation: &Evaluation) {
        self.push(ConsoleMessage {
            source: MessageSource::ConsoleApi,
            level: MessageLevel::Log,
            text: format!("> {expression}"),
            argument_object_ids: Vec::new(),
            location: None,
            repeat_count: 1,
        });
        self.push(evaluation.to_message());
    }
}

/// Where to evaluate an expression.
///
/// The selected call-frame id comes from the pause model (T-202 / T-203). This
/// crate does not depend on `mjx-wk-debug` — the host passes the id in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalTarget<'a> {
    /// Page is running — `Runtime.evaluate`.
    Runtime,
    /// Paused — `Debugger.evaluateOnCallFrame` on this frame.
    CallFrame { call_frame_id: &'a str },
}

/// Result of a console evaluation.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub text: String,
    pub object_id: Option<String>,
    pub was_thrown: bool,
}

impl Evaluation {
    /// Turn the result into a log row the UI can paint.
    pub fn to_message(&self) -> ConsoleMessage {
        ConsoleMessage {
            source: MessageSource::ConsoleApi,
            level: if self.was_thrown {
                MessageLevel::Error
            } else {
                MessageLevel::Log
            },
            text: self.text.clone(),
            argument_object_ids: self.object_id.iter().cloned().collect(),
            location: None,
            repeat_count: 1,
        }
    }
}

/// Evaluate `expression`, routing by [`EvalTarget`].
///
/// Does not mutate a [`ConsoleModel`] — the host (or
/// [`ConsoleAgent::evaluate_and_record`]) folds the result in. Keeps the agent
/// task free of Action handling while still pinning the pause-vs-running rule
/// in one place.
pub async fn evaluate(
    session: &SessionHandle,
    expression: &str,
    target: EvalTarget<'_>,
) -> Result<Evaluation, SessionError> {
    match target {
        EvalTarget::Runtime => {
            let returns = session
                .call(proto_runtime::commands::Evaluate {
                    expression: expression.to_owned(),
                    object_group: Some(CONSOLE_OBJECT_GROUP.to_owned()),
                    include_command_line_api: Some(true),
                    do_not_pause_on_exceptions_and_mute_console: None,
                    context_id: None,
                    return_by_value: None,
                    generate_preview: Some(true),
                    save_result: Some(true),
                    emulate_user_gesture: None,
                })
                .await?;
            Ok(evaluation_from_remote(
                returns.result,
                returns.was_thrown.unwrap_or(false),
            ))
        }
        EvalTarget::CallFrame { call_frame_id } => {
            let returns = session
                .call(proto_debugger::commands::EvaluateOnCallFrame {
                    call_frame_id: call_frame_id.to_owned(),
                    expression: expression.to_owned(),
                    object_group: Some(CONSOLE_OBJECT_GROUP.to_owned()),
                    include_command_line_api: Some(true),
                    do_not_pause_on_exceptions_and_mute_console: None,
                    return_by_value: None,
                    generate_preview: Some(true),
                    save_result: Some(true),
                    emulate_user_gesture: None,
                })
                .await?;
            Ok(evaluation_from_remote(
                returns.result,
                returns.was_thrown.unwrap_or(false),
            ))
        }
    }
}

fn evaluation_from_remote(result: RemoteObject, was_thrown: bool) -> Evaluation {
    Evaluation {
        text: format_remote_object(&result),
        object_id: result.object_id,
        was_thrown,
    }
}

/// Owns Domain::Console.
#[derive(Debug, Default)]
pub struct ConsoleAgent {
    model: ConsoleModel,
}

impl ConsoleAgent {
    /// An empty agent.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate and fold the result into the model.
    ///
    /// For hosts that still own the agent (tests, or a session task that has
    /// not yet moved it behind [`mjx_wk_session::AgentRegistry`]).
    pub async fn evaluate_and_record(
        &mut self,
        session: &SessionHandle,
        expression: &str,
        target: EvalTarget<'_>,
    ) -> Result<Evaluation, SessionError> {
        let evaluation = evaluate(session, expression, target).await?;
        self.model.record_evaluation(expression, &evaluation);
        Ok(evaluation)
    }
}

#[async_trait]
impl DomainAgent for ConsoleAgent {
    type Model = ConsoleModel;

    const DOMAINS: &'static [Domain] = &[Domain::Console];
    const NAME: &'static str = "mjx-wk-console";

    async fn attach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        // Enabling drains messages buffered while the domain was off via
        // messageAdded — that is why attach must enable before the UI reads a
        // snapshot, not after.
        session.call(proto_console::commands::Enable {}).await?;
        Ok(())
    }

    async fn on_event(&mut self, event: &NormalizedFrame) -> Result<(), SessionError> {
        let Frame::Event { method, params } = &event.frame else {
            return Ok(());
        };

        match method.as_str() {
            "Console.messageAdded" => {
                let added: proto_console::events::MessageAdded =
                    serde_json::from_value(params.clone()).map_err(|err| {
                        SessionError::Dialect(mjx_wk_dialect::DialectError::Envelope(format!(
                            "Console.messageAdded: {err}"
                        )))
                    })?;
                self.model.push(message_from_protocol(added.message));
            }
            "Console.messageRepeatCountUpdated" => {
                let updated: proto_console::events::MessageRepeatCountUpdated =
                    serde_json::from_value(params.clone()).map_err(|err| {
                        SessionError::Dialect(mjx_wk_dialect::DialectError::Envelope(format!(
                            "Console.messageRepeatCountUpdated: {err}"
                        )))
                    })?;
                let count = u32::try_from(updated.count.max(1)).unwrap_or(u32::MAX);
                self.model.set_last_repeat_count(count);
            }
            "Console.messagesCleared" => {
                self.model.clear_messages();
            }
            _ => {}
        }
        Ok(())
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        Arc::new(self.model.clone())
    }

    async fn detach(&mut self, session: &SessionHandle) -> Result<(), SessionError> {
        // Console evaluations pin remote objects under CONSOLE_OBJECT_GROUP.
        // Leaving them behind leaks heap in the program under test.
        if session
            .supports(Domain::Runtime, "releaseObjectGroup")
            .is_available()
        {
            let _ = session
                .call(proto_runtime::commands::ReleaseObjectGroup {
                    object_group: CONSOLE_OBJECT_GROUP.to_owned(),
                })
                .await;
        }
        let _ = session.call(proto_console::commands::Disable {}).await;
        Ok(())
    }
}

fn message_from_protocol(msg: proto_console::ConsoleMessage) -> ConsoleMessage {
    let argument_object_ids = msg
        .parameters
        .as_ref()
        .map(|params| params.iter().filter_map(|p| p.object_id.clone()).collect())
        .unwrap_or_default();

    let text = format_message_text(&msg);

    let repeat_count = msg
        .repeat_count
        .and_then(|c| u32::try_from(c.max(1)).ok())
        .unwrap_or(1);

    ConsoleMessage {
        source: map_source(msg.source),
        level: map_level(msg.level),
        text,
        argument_object_ids,
        // SourceId resolution needs the inventory (L3); the agent keeps the
        // wire text and leaves location unset rather than inventing ids.
        location: None,
        repeat_count,
    }
}

fn format_message_text(msg: &proto_console::ConsoleMessage) -> String {
    if let Some(params) = msg.parameters.as_ref()
        && !params.is_empty()
    {
        return params
            .iter()
            .map(format_remote_object)
            .collect::<Vec<_>>()
            .join(" ");
    }
    msg.text.clone()
}

fn format_remote_object(obj: &RemoteObject) -> String {
    if let Some(desc) = obj.description.as_ref() {
        return desc.clone();
    }
    if let Some(value) = obj.value.as_ref() {
        return match value {
            Value::String(s) => s.clone(),
            Value::Null => "null".to_owned(),
            other => other.to_string(),
        };
    }
    if let Some(class) = obj.class_name.as_ref() {
        return class.clone();
    }
    format!("{:?}", obj.r#type)
}

fn map_source(source: proto_console::ChannelSource) -> MessageSource {
    match source {
        proto_console::ChannelSource::Javascript => MessageSource::Javascript,
        proto_console::ChannelSource::Network => MessageSource::Network,
        proto_console::ChannelSource::ConsoleApi => MessageSource::ConsoleApi,
        proto_console::ChannelSource::Storage => MessageSource::Storage,
        proto_console::ChannelSource::Rendering => MessageSource::Rendering,
        proto_console::ChannelSource::Css => MessageSource::Css,
        proto_console::ChannelSource::Security => MessageSource::Security,
        _ => MessageSource::Other,
    }
}

fn map_level(level: proto_console::ConsoleMessageLevel) -> MessageLevel {
    match level {
        proto_console::ConsoleMessageLevel::Debug => MessageLevel::Debug,
        proto_console::ConsoleMessageLevel::Log => MessageLevel::Log,
        proto_console::ConsoleMessageLevel::Info => MessageLevel::Info,
        proto_console::ConsoleMessageLevel::Warning => MessageLevel::Warning,
        proto_console::ConsoleMessageLevel::Error => MessageLevel::Error,
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    fn msg(text: &str) -> ConsoleMessage {
        ConsoleMessage {
            source: MessageSource::ConsoleApi,
            level: MessageLevel::Log,
            text: text.to_owned(),
            argument_object_ids: Vec::new(),
            location: None,
            repeat_count: 1,
        }
    }

    #[test]
    fn push_past_capacity_increments_dropped() {
        let mut model = ConsoleModel::new();
        for i in 0..(MESSAGE_CAPACITY + 3) {
            model.push(msg(&format!("m{i}")));
        }
        assert_eq!(model.messages.len(), MESSAGE_CAPACITY);
        assert_eq!(model.dropped, 3);
        assert_eq!(model.messages[0].text, "m3");
        assert_eq!(
            model.messages.last().map(|m| m.text.as_str()),
            Some(&format!("m{}", MESSAGE_CAPACITY + 2)[..])
        );
    }

    #[test]
    fn repeat_count_updates_the_last_message_only() {
        let mut model = ConsoleModel::new();
        model.push(msg("once"));
        model.push(msg("loop"));
        model.set_last_repeat_count(42);
        assert_eq!(model.messages[0].repeat_count, 1);
        assert_eq!(model.messages[1].repeat_count, 42);
    }

    #[test]
    fn clear_keeps_dropped_count() {
        let mut model = ConsoleModel::new();
        for i in 0..(MESSAGE_CAPACITY + 1) {
            model.push(msg(&format!("m{i}")));
        }
        assert_eq!(model.dropped, 1);
        model.clear_messages();
        assert!(model.messages.is_empty());
        assert_eq!(model.dropped, 1);
    }
}
