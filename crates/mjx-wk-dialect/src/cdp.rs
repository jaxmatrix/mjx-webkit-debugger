//! The translating dialect: the Chrome DevTools Protocol.
//!
//! **Phase 4.** `encode`/`decode` are unimplemented; the capability table below
//! is not, because it is the frozen part of the seam and the UI needs it to
//! grey controls out honestly rather than by trial and error.
//!
//! # Why this exists
//!
//! Not for completeness — for Tauri. Tauri/WRY uses a different engine per
//! platform, and two of the five are Chromium:
//!
//! | Platform | Engine | Protocol |
//! |---|---|---|
//! | Linux | WebKitGTK | WebKit RWI |
//! | macOS / iOS | WKWebView | WebKit RWI |
//! | **Windows** | **WebView2** | **CDP** |
//! | **Android** | **System WebView** | **CDP** |
//!
//! So "debug my Tauri app on Windows" and "run the test suite against a
//! Chromium WebView" are the same piece of work.
//!
//! # The shape of the translation
//!
//! CDP is not a renaming of WebKit RWI. The structural differences that matter:
//!
//! | WebKit | CDP |
//! |---|---|
//! | `Target.sendMessageToTarget` (frame as a string) | `sessionId` on the frame |
//! | `Console.messageAdded` | `Runtime.consoleAPICalled` + `Log.entryAdded` |
//! | `Runtime.getProperties` is paginated | unpaginated; slice client-side |
//! | `Network.addInterception` family | the `Fetch` domain |
//! | `Page.getCookies` | the `Storage` domain |
//! | `Timeline` | `Tracing` |
//! | `ScriptProfiler` + `CPUProfiler` | `Profiler` |
//! | breakpoint `options.actions` | condition with a side effect |
//! | `Canvas`, `Recording`, `Audit`, `Memory` | no equivalent |

use mjx_wk_protocol::{Domain, Frame};

use crate::{Dialect, DialectError, DialectKind, NormalizedFrame, Support, TargetId};

/// The Chrome DevTools Protocol, translated to and from WebKit vocabulary.
#[derive(Debug, Clone, Copy, Default)]
pub struct CdpDialect;

/// Domains CDP serves directly, member for member, modulo field names.
const NATIVE_DOMAINS: &[Domain] = &[
    Domain::Debugger,
    Domain::Runtime,
    Domain::Page,
    Domain::Network,
    Domain::Dom,
    Domain::Css,
    Domain::DomDebugger,
    Domain::DomStorage,
    Domain::IndexedDb,
    Domain::LayerTree,
    Domain::Animation,
    Domain::Target,
    Domain::Inspector,
    Domain::Worker,
    Domain::ServiceWorker,
    Domain::Browser,
    Domain::Security,
];

/// Domains reachable only by routing through differently-named CDP domains.
const EMULATED_DOMAINS: &[Domain] = &[
    // Console.messageAdded ← Runtime.consoleAPICalled + Log.entryAdded
    Domain::Console,
    // Timeline ← Tracing
    Domain::Timeline,
    // ScriptProfiler / CPUProfiler ← Profiler
    Domain::ScriptProfiler,
    Domain::CpuProfiler,
    // Heap ← HeapProfiler
    Domain::Heap,
];

/// WebKit members with no CDP counterpart at all.
///
/// Listed by `(domain, member)`; an empty member means the whole domain.
const UNSUPPORTED: &[(Domain, &str)] = &[
    // WebKit's breakpoint actions are richer than a CDP logpoint. Log and
    // Evaluate are emulable via a condition with a side effect; Probe (the
    // live gutter-value feature) and Sound are not.
    (Domain::Debugger, "playBreakpointActionSound"),
    (Domain::Debugger, "setPauseOnMicrotasks"),
    (Domain::Debugger, "setPauseOnAssertions"),
    (Domain::Debugger, "continueUntilNextRunLoop"),
    (Domain::Debugger, "setPauseForInternalScripts"),
    // Runtime's type and control-flow profilers are JavaScriptCore features.
    (Domain::Runtime, "enableTypeProfiler"),
    (Domain::Runtime, "enableControlFlowProfiler"),
    (Domain::Runtime, "getRuntimeTypesForVariablesAtOffsets"),
    (Domain::Runtime, "getBasicBlocks"),
    // Whole domains with no Chromium analogue.
    (Domain::Canvas, ""),
    (Domain::Recording, ""),
    (Domain::Audit, ""),
    (Domain::Memory, ""),
];

impl Dialect for CdpDialect {
    fn kind(&self) -> DialectKind {
        DialectKind::ChromeDevToolsProtocol
    }

    fn encode(&self, _frame: Frame, _target: Option<&TargetId>) -> Result<Frame, DialectError> {
        todo!("Phase 4 — see docs/tasks/T-403-cdp-dialect.md")
    }

    fn decode(&self, _frame: Frame) -> Result<NormalizedFrame, DialectError> {
        todo!("Phase 4 — see docs/tasks/T-403-cdp-dialect.md")
    }

    fn supports(&self, domain: Domain, member: &str) -> Support {
        // A member-level exclusion beats its domain's general availability.
        if UNSUPPORTED
            .iter()
            .any(|(d, m)| *d == domain && (m.is_empty() || *m == member))
        {
            return Support::Unsupported;
        }
        if NATIVE_DOMAINS.contains(&domain) {
            return Support::Native;
        }
        if EMULATED_DOMAINS.contains(&domain) {
            return Support::Emulated;
        }
        Support::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_debugging_is_native_over_cdp() {
        // The Phase 1 and 2 feature set must survive the crossing, or the
        // Windows and Android transports are pointless.
        assert_eq!(
            CdpDialect.supports(Domain::Debugger, "setBreakpointByUrl"),
            Support::Native
        );
        assert_eq!(
            CdpDialect.supports(Domain::Runtime, "getProperties"),
            Support::Native
        );
        assert_eq!(CdpDialect.supports(Domain::Page, "reload"), Support::Native);
    }

    #[test]
    fn renamed_domains_report_as_emulated_rather_than_missing() {
        assert_eq!(
            CdpDialect.supports(Domain::Console, "messageAdded"),
            Support::Emulated
        );
        assert_eq!(
            CdpDialect.supports(Domain::Timeline, "start"),
            Support::Emulated
        );
    }

    #[test]
    fn a_member_exclusion_overrides_its_domains_availability() {
        // Debugger is native overall, but these particular members are not.
        assert_eq!(
            CdpDialect.supports(Domain::Debugger, "setPauseOnMicrotasks"),
            Support::Unsupported
        );
        assert_eq!(
            CdpDialect.supports(Domain::Debugger, "resume"),
            Support::Native
        );
    }

    #[test]
    fn webkit_only_domains_are_unsupported_wholesale() {
        for (domain, member) in [
            (Domain::Canvas, "requestContent"),
            (Domain::Audit, "run"),
            (Domain::Memory, "startTracking"),
            (Domain::Recording, "anything"),
        ] {
            assert_eq!(
                CdpDialect.supports(domain, member),
                Support::Unsupported,
                "{domain}.{member}"
            );
        }
    }

    #[test]
    fn every_domain_has_a_verdict() {
        // A domain missing from all three tables would silently read as
        // unsupported; assert the classification is deliberate for each.
        for &d in Domain::ALL {
            let classified = NATIVE_DOMAINS.contains(&d)
                || EMULATED_DOMAINS.contains(&d)
                || UNSUPPORTED.iter().any(|(u, m)| *u == d && m.is_empty());
            assert!(classified, "{d} is in no CDP support table");
        }
    }
}
