//! L4 — timelines, sampling profiles, memory, and heap snapshots.
//!
//! **Phase 5.**
//!
//! # WebKit has no `Profiler` domain
//!
//! What Chrome puts in `Profiler` and `Tracing`, WebKit splits across five:
//! `Timeline` (the record tree), `ScriptProfiler` (JavaScript samples),
//! `CPUProfiler` (per-thread CPU), `Memory` (category timeline), and `Heap`
//! (snapshots and GC events). They start and stop independently, which is why
//! [`ProfileModel`] tracks instruments rather than one recording flag.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// One thing that can be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    Timeline,
    ScriptProfiler,
    CpuProfiler,
    Memory,
    Heap,
}

/// A node in the timeline record tree.
#[derive(Debug, Clone)]
pub struct TimelineRecord {
    pub kind: String,
    pub start: f64,
    pub end: Option<f64>,
    pub children: Vec<TimelineRecord>,
    /// Where in the page's code this happened, when it is known.
    pub location: Option<mjx_wk_source::SourceLocation>,
}

/// One frame in a flame graph.
#[derive(Debug, Clone)]
pub struct FlameFrame {
    pub function_name: String,
    pub location: Option<mjx_wk_source::SourceLocation>,
    /// Samples in this frame only.
    pub self_samples: u64,
    /// Samples in this frame and everything it called.
    pub total_samples: u64,
    pub children: Vec<FlameFrame>,
}

/// A node in a heap snapshot.
#[derive(Debug, Clone)]
pub struct HeapNode {
    pub id: u64,
    pub class_name: String,
    pub size: u64,
    /// What keeps this alive. The retaining path is the answer to "why has this
    /// not been collected", which is the only question a heap snapshot is
    /// really asked.
    pub retainers: Vec<u64>,
}

/// The performance panel.
#[derive(Debug, Default)]
pub struct ProfileModel {
    pub recording: Vec<Instrument>,
    pub timeline: Vec<TimelineRecord>,
    pub flame: Option<FlameFrame>,
    /// Sampled memory by category over time.
    pub memory: Vec<(f64, Vec<(String, u64)>)>,
    pub heap_snapshot: Option<Vec<HeapNode>>,
}

/// Owns Domain::Timeline, Domain::ScriptProfiler, Domain::CpuProfiler, Domain::Heap, Domain::Memory.
#[derive(Debug, Default)]
pub struct ProfileAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for ProfileAgent {
    type Model = ProfileModel;

    const DOMAINS: &'static [Domain] = &[
        Domain::Timeline,
        Domain::ScriptProfiler,
        Domain::CpuProfiler,
        Domain::Heap,
        Domain::Memory,
    ];
    const NAME: &'static str = "mjx-wk-profile";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 5 — docs/tasks/T-501-timeline.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 5 — docs/tasks/T-501-timeline.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 5 — docs/tasks/T-501-timeline.md")
    }
}
