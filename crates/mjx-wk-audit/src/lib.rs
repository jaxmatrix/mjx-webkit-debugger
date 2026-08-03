//! L4 — audits, and the inspector/browser lifecycle.
//!
//! **Phase 7.** WebKit's `Audit` domain runs JavaScript test functions inside
//! the debuggee and reports structured results — closer to a scriptable
//! assertion runner than to Lighthouse.
//!
//! This crate also owns `Inspector` (enable, initialized, the `inspect` event
//! that fires when the user picks an element) and `Browser` (extension
//! discovery), which belong to no panel of their own.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// A named group of audit tests.
#[derive(Debug, Clone)]
pub struct AuditSuite {
    pub name: String,
    /// A JavaScript function body, run in the debuggee.
    pub test_source: String,
}

/// What an audit reported.
#[derive(Debug, Clone)]
pub struct AuditResult {
    pub suite: String,
    pub level: AuditLevel,
    pub message: String,
    /// Nodes the result points at, so it can be clicked through to the DOM.
    pub nodes: Vec<mjx_wk_source::NodeId>,
}

/// How serious a result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuditLevel {
    Pass,
    Info,
    Warn,
    Fail,
    /// The test itself threw.
    Error,
}

/// The audit panel.
#[derive(Debug, Default)]
pub struct AuditModel {
    pub suites: Vec<AuditSuite>,
    pub results: Vec<AuditResult>,
    pub running: bool,
}

/// Owns Domain::Audit, Domain::Browser, Domain::Inspector.
#[derive(Debug, Default)]
pub struct AuditAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for AuditAgent {
    type Model = AuditModel;

    const DOMAINS: &'static [Domain] = &[Domain::Audit, Domain::Browser, Domain::Inspector];
    const NAME: &'static str = "mjx-wk-audit";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-703-audits.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-703-audits.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 7 — docs/tasks/T-703-audits.md")
    }
}
