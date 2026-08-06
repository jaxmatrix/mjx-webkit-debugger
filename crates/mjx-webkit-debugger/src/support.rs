//! [`mjx_wk_ui::SupportQuery`] adapters for the app shell.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_session::SessionHandle;
use mjx_wk_ui::SupportQuery;

/// Answers support queries from a live session (cheap mutex read, never awaits).
#[derive(Debug)]
pub struct SessionSupport {
    session: SessionHandle,
}

impl SessionSupport {
    pub fn new(session: &SessionHandle) -> Self {
        Self {
            session: session.clone(),
        }
    }
}

impl SupportQuery for SessionSupport {
    fn supports(&self, domain: Domain, member: &str) -> Support {
        self.session.supports(domain, member)
    }
}

/// Used before a session exists (picker / failed attach). Everything is
/// unsupported so panels render disabled with a reason rather than pretending.
#[derive(Debug, Default)]
pub struct DetachedSupport;

impl DetachedSupport {
    pub fn not_attached() -> Self {
        Self
    }
}

impl SupportQuery for DetachedSupport {
    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        Support::Unsupported
    }
}
