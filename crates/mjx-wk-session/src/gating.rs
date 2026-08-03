//! What this particular debuggee can actually do.
//!
//! # Why this is learned rather than asked
//!
//! WebKit has no "list your capabilities" command. The inspector frontend knows
//! what a build supports because the build *ships the frontend* — the
//! `InspectorBackendCommands.js` inside `libwebkit2gtk` is generated from the
//! same descriptions that build's backend was. We are not shipped alongside the
//! debuggee, so we cannot do that.
//!
//! So the model is **optimistic with a negative cache**: assume everything we
//! generated types for is present, and remember each member the debuggee
//! rejects with `MethodNotFound`. In practice the optimistic guess is right
//! almost always — `xtask verify-protocol` exists to keep the generated set
//! aligned with the WebKit actually installed — and the cache makes the rare
//! miss cost exactly one error, once.
//!
//! The alternative, refusing to send anything not proven present, would mean a
//! debugger that works with one WebKit version and silently does less with the
//! next.

use std::collections::HashSet;

use mjx_wk_dialect::{Dialect, Support};
use mjx_wk_protocol::{Domain, ProtocolError, TargetType};

use crate::UnsupportedReason;

/// What a session has learned about its debuggee.
#[derive(Debug)]
pub struct Capabilities {
    /// Members this debuggee has rejected as unknown.
    unavailable: HashSet<(Domain, String)>,
    /// What kind of target this is, which gates whole domains.
    target_kind: TargetType,
}

impl Capabilities {
    /// Start out assuming everything the generated protocol covers is present.
    pub fn new(target_kind: TargetType) -> Self {
        Self {
            unavailable: HashSet::new(),
            target_kind,
        }
    }

    /// The kind of target these capabilities describe.
    pub fn target_kind(&self) -> TargetType {
        self.target_kind
    }

    /// Whether a member can be used, checking the dialect and what we have
    /// learned from the debuggee.
    ///
    /// The dialect is consulted first: a member the wire cannot express is
    /// unavailable regardless of what the debuggee would have said.
    pub fn supports(&self, dialect: &dyn Dialect, domain: Domain, member: &str) -> Support {
        match dialect.supports(domain, member) {
            Support::Unsupported => Support::Unsupported,
            expressible => {
                if self.is_known_absent(domain, member) {
                    Support::Unsupported
                } else {
                    expressible
                }
            }
        }
    }

    /// Why a member is unavailable, for an error message that tells the user
    /// something actionable.
    pub fn reason(&self, dialect: &dyn Dialect, domain: Domain, member: &str) -> UnsupportedReason {
        if dialect.supports(domain, member) == Support::Unsupported {
            UnsupportedReason::Dialect
        } else if domain_needs_page_target(domain) && !target_is_page_like(self.target_kind) {
            UnsupportedReason::TargetKind
        } else {
            UnsupportedReason::DebuggeeBuild
        }
    }

    /// Whether we have already been told this member does not exist.
    pub fn is_known_absent(&self, domain: Domain, member: &str) -> bool {
        // A whole domain is recorded as an empty member name.
        self.unavailable.contains(&(domain, String::new()))
            || self.unavailable.contains(&(domain, member.to_owned()))
    }

    /// Record what a failed command taught us.
    ///
    /// Only `MethodNotFound` is durable knowledge. Every other error says
    /// something about *this call*, not about the member's existence — marking
    /// a member absent because one call had bad arguments would disable a
    /// working feature for the rest of the session.
    pub fn learn_from_failure(&mut self, domain: Domain, member: &str, error: &ProtocolError) {
        if error.is_unsupported() {
            self.unavailable.insert((domain, member.to_owned()));
        }
    }

    /// Record that a whole domain is absent, e.g. after `Domain.enable` fails.
    pub fn mark_domain_absent(&mut self, domain: Domain) {
        self.unavailable.insert((domain, String::new()));
    }

    /// How many members have been learned absent. For diagnostics.
    pub fn absent_count(&self) -> usize {
        self.unavailable.len()
    }
}

/// Domains that only exist where there is a page.
///
/// A `service-worker` or bare `javascript` target has no document, so these are
/// absent by construction rather than by build configuration.
fn domain_needs_page_target(domain: Domain) -> bool {
    matches!(
        domain,
        Domain::Page
            | Domain::Dom
            | Domain::Css
            | Domain::DomStorage
            | Domain::LayerTree
            | Domain::Animation
            | Domain::Canvas
            | Domain::DomDebugger
    )
}

fn target_is_page_like(kind: TargetType) -> bool {
    matches!(kind, TargetType::Page | TargetType::WebPage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mjx_wk_dialect::{CdpDialect, WebKitDialect};

    fn not_found() -> ProtocolError {
        ProtocolError {
            code: -32601,
            message: "'Debugger.setPauseOnMicrotasks' was not found".into(),
            data: None,
        }
    }

    fn bad_args() -> ProtocolError {
        ProtocolError {
            code: -32602,
            message: "Parameter 'lineNumber' is required".into(),
            data: None,
        }
    }

    #[test]
    fn everything_is_assumed_present_until_the_debuggee_says_otherwise() {
        let caps = Capabilities::new(TargetType::WebPage);
        assert_eq!(
            caps.supports(&WebKitDialect, Domain::Debugger, "setPauseOnMicrotasks"),
            Support::Native
        );
    }

    #[test]
    fn a_method_not_found_is_remembered() {
        let mut caps = Capabilities::new(TargetType::WebPage);
        caps.learn_from_failure(Domain::Debugger, "setPauseOnMicrotasks", &not_found());

        assert_eq!(
            caps.supports(&WebKitDialect, Domain::Debugger, "setPauseOnMicrotasks"),
            Support::Unsupported
        );
        // Only that member; its neighbours are untouched.
        assert_eq!(
            caps.supports(&WebKitDialect, Domain::Debugger, "resume"),
            Support::Native
        );
    }

    #[test]
    fn an_argument_error_teaches_us_nothing_durable() {
        // Disabling a working feature because one call passed bad arguments
        // would be a silent, session-long regression.
        let mut caps = Capabilities::new(TargetType::WebPage);
        caps.learn_from_failure(Domain::Debugger, "setBreakpointByUrl", &bad_args());
        assert_eq!(caps.absent_count(), 0);
        assert_eq!(
            caps.supports(&WebKitDialect, Domain::Debugger, "setBreakpointByUrl"),
            Support::Native
        );
    }

    #[test]
    fn marking_a_domain_absent_covers_all_its_members() {
        let mut caps = Capabilities::new(TargetType::WebPage);
        caps.mark_domain_absent(Domain::Security);
        for member in ["enable", "disable", "anythingElse"] {
            assert_eq!(
                caps.supports(&WebKitDialect, Domain::Security, member),
                Support::Unsupported
            );
        }
    }

    #[test]
    fn the_dialect_veto_comes_first() {
        // Canvas has no CDP equivalent, so it is unavailable over CDP even
        // though nothing has been learned from the debuggee.
        let caps = Capabilities::new(TargetType::WebPage);
        assert_eq!(
            caps.supports(&CdpDialect, Domain::Canvas, "requestContent"),
            Support::Unsupported
        );
        assert_eq!(
            caps.reason(&CdpDialect, Domain::Canvas, "requestContent"),
            UnsupportedReason::Dialect
        );
    }

    #[test]
    fn page_domains_are_attributed_to_the_target_kind_not_the_build() {
        // The user-facing difference matters: "your build lacks this" invites a
        // version upgrade; "service workers have no DOM" does not.
        let worker = Capabilities::new(TargetType::ServiceWorker);
        assert_eq!(
            worker.reason(&WebKitDialect, Domain::Dom, "getDocument"),
            UnsupportedReason::TargetKind
        );

        let page = Capabilities::new(TargetType::WebPage);
        assert_eq!(
            page.reason(&WebKitDialect, Domain::Dom, "getDocument"),
            UnsupportedReason::DebuggeeBuild
        );
    }

    #[test]
    fn debugger_and_runtime_are_available_on_every_target_kind() {
        // Whatever we attach to, we can debug script in it. If this ever
        // stopped holding, the source and debugger panels would need gating.
        for kind in [
            TargetType::WebPage,
            TargetType::ServiceWorker,
            TargetType::JavaScript,
            TargetType::Worker,
        ] {
            let caps = Capabilities::new(kind);
            for domain in [Domain::Debugger, Domain::Runtime, Domain::Console] {
                assert_eq!(
                    caps.supports(&WebKitDialect, domain, "enable"),
                    Support::Native,
                    "{domain} on {kind}"
                );
            }
        }
    }
}
