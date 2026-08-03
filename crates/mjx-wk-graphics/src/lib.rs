//! L4 — canvas, shaders, compositing layers, and animations.
//!
//! **Phase 7.** Mostly WebKit-only territory: `Canvas` alone has 28 members,
//! including shader source editing, and Chromium has no equivalent — see the
//! unsupported table in `mjx_wk_dialect::cdp`.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// A canvas rendering context in the page.
#[derive(Debug, Clone)]
pub struct CanvasContext {
    pub id: String,
    /// `"canvas-2d"`, `"webgl"`, `"webgl2"`, `"webgpu"`, `"bitmaprenderer"`.
    pub context_type: String,
    pub node: Option<mjx_wk_source::NodeId>,
    pub memory_bytes: Option<u64>,
}

/// A shader program, whose source can be edited live.
#[derive(Debug, Clone)]
pub struct ShaderProgram {
    pub id: String,
    pub canvas_id: String,
    pub vertex_source: Option<String>,
    pub fragment_source: Option<String>,
    pub disabled: bool,
}

/// A compositing layer.
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: String,
    pub node: Option<mjx_wk_source::NodeId>,
    pub bounds: (f64, f64, f64, f64),
    pub memory_bytes: u64,
    /// Why this got its own layer — the whole point of the layers panel.
    pub compositing_reasons: Vec<String>,
}

/// One animation.
#[derive(Debug, Clone)]
pub struct AnimationEntry {
    pub id: String,
    pub name: Option<String>,
    pub target: Option<mjx_wk_source::NodeId>,
    pub duration_ms: Option<f64>,
    pub iterations: Option<f64>,
    pub playback_rate: f64,
}

/// The graphics panels.
#[derive(Debug, Default)]
pub struct GraphicsModel {
    pub canvases: Vec<CanvasContext>,
    pub shaders: Vec<ShaderProgram>,
    pub layers: Vec<Layer>,
    pub animations: Vec<AnimationEntry>,
}

/// Owns Domain::Canvas, Domain::Recording, Domain::LayerTree, Domain::Animation.
#[derive(Debug, Default)]
pub struct GraphicsAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for GraphicsAgent {
    type Model = GraphicsModel;

    const DOMAINS: &'static [Domain] = &[
        Domain::Canvas,
        Domain::Recording,
        Domain::LayerTree,
        Domain::Animation,
    ];
    const NAME: &'static str = "mjx-wk-graphics";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-702-canvas-inspection.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 7 — docs/tasks/T-702-canvas-inspection.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 7 — docs/tasks/T-702-canvas-inspection.md")
    }
}
