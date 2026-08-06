//! [`mjx_wk_ui::SupportQuery`] adapters for the app shell.
//!
//! **Owned by `docs/tasks/T-010-app-shell.md`.**

use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;
use mjx_wk_session::SessionHandle;
use mjx_wk_ui::SupportQuery;

/// Answers support queries from a live session (cheap mutex read, never awaits).
///
/// Wired once Wave 2 keeps a `SessionHandle` reachable from the UI thread
/// (via `Arc` / snapshot) without awaiting.
#[derive(Debug)]
#[allow(dead_code, reason = "Wave 2: live SupportQuery once session is UI-reachable")]
pub struct SessionSupport {
    session: SessionHandle,
}

impl SessionSupport {
    #[allow(dead_code, reason = "Wave 2: constructed when attach succeeds")]
    pub fn new(session: SessionHandle) -> Self {
        Self { session }
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
