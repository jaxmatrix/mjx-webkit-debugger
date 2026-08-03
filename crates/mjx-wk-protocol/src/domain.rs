//! The protocol's three vocabularies: domains, debuggable types, target types.
//!
//! These are hand-written rather than generated. They change about once a
//! decade, and every other crate matches on them, so a stable hand-authored
//! enum is worth more than a regenerated one.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A protocol domain — the first half of every method name.
///
/// 26 domains exist in the WebKit sources at the pinned ref. A given build
/// activates a subset: WebKitGTK 2.52.3 activates 25, omitting [`Domain::Security`].
/// Never assume a domain is present — ask the session, which tracks what the
/// debuggee actually announced. See `docs/PROTOCOL-NOTES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Domain {
    Animation,
    Audit,
    Browser,
    #[serde(rename = "CPUProfiler")]
    CpuProfiler,
    Canvas,
    Console,
    #[serde(rename = "CSS")]
    Css,
    #[serde(rename = "DOM")]
    Dom,
    #[serde(rename = "DOMDebugger")]
    DomDebugger,
    #[serde(rename = "DOMStorage")]
    DomStorage,
    Debugger,
    Heap,
    #[serde(rename = "IndexedDB")]
    IndexedDb,
    Inspector,
    LayerTree,
    Memory,
    Network,
    Page,
    Recording,
    Runtime,
    ScriptProfiler,
    Security,
    ServiceWorker,
    Target,
    Timeline,
    Worker,
}

impl Domain {
    /// Every domain, in the order they appear on the wire.
    pub const ALL: &'static [Domain] = &[
        Domain::Animation,
        Domain::Audit,
        Domain::Browser,
        Domain::CpuProfiler,
        Domain::Canvas,
        Domain::Console,
        Domain::Css,
        Domain::Dom,
        Domain::DomDebugger,
        Domain::DomStorage,
        Domain::Debugger,
        Domain::Heap,
        Domain::IndexedDb,
        Domain::Inspector,
        Domain::LayerTree,
        Domain::Memory,
        Domain::Network,
        Domain::Page,
        Domain::Recording,
        Domain::Runtime,
        Domain::ScriptProfiler,
        Domain::Security,
        Domain::ServiceWorker,
        Domain::Target,
        Domain::Timeline,
        Domain::Worker,
    ];

    /// The exact wire spelling. Case matters: `"CSS"`, not `"Css"`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Domain::Animation => "Animation",
            Domain::Audit => "Audit",
            Domain::Browser => "Browser",
            Domain::CpuProfiler => "CPUProfiler",
            Domain::Canvas => "Canvas",
            Domain::Console => "Console",
            Domain::Css => "CSS",
            Domain::Dom => "DOM",
            Domain::DomDebugger => "DOMDebugger",
            Domain::DomStorage => "DOMStorage",
            Domain::Debugger => "Debugger",
            Domain::Heap => "Heap",
            Domain::IndexedDb => "IndexedDB",
            Domain::Inspector => "Inspector",
            Domain::LayerTree => "LayerTree",
            Domain::Memory => "Memory",
            Domain::Network => "Network",
            Domain::Page => "Page",
            Domain::Recording => "Recording",
            Domain::Runtime => "Runtime",
            Domain::ScriptProfiler => "ScriptProfiler",
            Domain::Security => "Security",
            Domain::ServiceWorker => "ServiceWorker",
            Domain::Target => "Target",
            Domain::Timeline => "Timeline",
            Domain::Worker => "Worker",
        }
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Domain {
    type Err = UnknownDomain;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Domain::ALL
            .iter()
            .copied()
            .find(|d| d.as_str() == s)
            .ok_or_else(|| UnknownDomain(s.to_owned()))
    }
}

/// A method arrived naming a domain this build of the debugger does not model.
///
/// This is expected, not exceptional: a newer WebKit may add a domain. Callers
/// log and skip rather than tearing down the session.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown protocol domain `{0}`")]
pub struct UnknownDomain(pub String);

/// What kind of thing is being debugged, as a whole.
///
/// This is the axis `activateDomain` uses in the inspector frontend, and it is
/// how the session decides which domains a connection may speak at all.
///
/// Distinct from [`TargetType`] — see the note there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DebuggableType {
    /// A page in a web content process — the ordinary case.
    WebPage,
    /// A bare `JSContext` with no page around it.
    JavaScript,
    /// A service worker.
    ServiceWorker,
    /// Apple's internal template markup language.
    Itml,
    /// A WebAssembly debugging session.
    WasmDebugger,
}

impl DebuggableType {
    /// The exact wire spelling (kebab-case, e.g. `"web-page"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            DebuggableType::WebPage => "web-page",
            DebuggableType::JavaScript => "javascript",
            DebuggableType::ServiceWorker => "service-worker",
            DebuggableType::Itml => "itml",
            DebuggableType::WasmDebugger => "wasm-debugger",
        }
    }
}

impl fmt::Display for DebuggableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An individual target *within* a debuggable, as reported by `Target.*`.
///
/// Deliberately a different type from [`DebuggableType`], because the protocol
/// uses two overlapping-but-unequal vocabularies. `targetTypes` in the domain
/// descriptions carries seven values; `debuggableTypes` carries five. The extra
/// two — [`TargetType::Page`] and [`TargetType::Worker`] — appear only here.
/// Conflating them silently mis-gates domains on multi-process pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TargetType {
    /// A page target inside a multi-process debuggable.
    Page,
    /// A dedicated or shared worker target.
    Worker,
    /// The whole web page.
    WebPage,
    /// A bare `JSContext`.
    JavaScript,
    /// A service worker.
    ServiceWorker,
    /// Apple's internal template markup language.
    Itml,
    /// A WebAssembly debugging session.
    WasmDebugger,
}

impl TargetType {
    /// The exact wire spelling (kebab-case).
    pub const fn as_str(self) -> &'static str {
        match self {
            TargetType::Page => "page",
            TargetType::Worker => "worker",
            TargetType::WebPage => "web-page",
            TargetType::JavaScript => "javascript",
            TargetType::ServiceWorker => "service-worker",
            TargetType::Itml => "itml",
            TargetType::WasmDebugger => "wasm-debugger",
        }
    }
}

impl fmt::Display for TargetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_round_trips_through_its_wire_spelling() {
        for &d in Domain::ALL {
            assert_eq!(
                Ok(d),
                d.as_str().parse::<Domain>(),
                "{d} did not round-trip"
            );
        }
    }

    #[test]
    fn acronym_domains_keep_their_upper_case_wire_spelling() {
        // Getting these wrong is silent: the method name is simply never matched.
        assert_eq!(Domain::Css.as_str(), "CSS");
        assert_eq!(Domain::Dom.as_str(), "DOM");
        assert_eq!(Domain::CpuProfiler.as_str(), "CPUProfiler");
        assert_eq!(Domain::IndexedDb.as_str(), "IndexedDB");
        assert_eq!(Domain::DomStorage.as_str(), "DOMStorage");
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        let mut seen: Vec<&str> = Domain::ALL.iter().map(|d| d.as_str()).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len(), "Domain::ALL contains a duplicate");
        assert_eq!(before, 26, "expected 26 domains at the pinned WebKit ref");
    }

    #[test]
    fn target_type_is_a_superset_of_debuggable_type() {
        // The two vocabularies overlap but are not equal; `page` and `worker`
        // exist only as target types. See the doc comment on `TargetType`.
        for d in [
            DebuggableType::WebPage,
            DebuggableType::JavaScript,
            DebuggableType::ServiceWorker,
            DebuggableType::Itml,
            DebuggableType::WasmDebugger,
        ] {
            let matching = [
                TargetType::WebPage,
                TargetType::JavaScript,
                TargetType::ServiceWorker,
                TargetType::Itml,
                TargetType::WasmDebugger,
            ]
            .iter()
            .any(|t| t.as_str() == d.as_str());
            assert!(matching, "{d} has no TargetType counterpart");
        }
    }
}
