//! L5 — the panel contract and the widgets that implement it.
//!
//! # This crate holds no session
//!
//! Deliberately, and it is the rule that keeps the UI fast. A widget is a pure
//! function of a model: it renders what it is given and returns [`Action`]s
//! describing what the user did. It cannot await, cannot call the debuggee, and
//! cannot block — so it cannot drop a frame no matter what the debuggee is
//! doing.
//!
//! ```text
//!   Model (Arc snapshot) ──► Panel::ui ──► Vec<Action> ──► session task
//!        ▲                                                      │
//!        └──────────────────── new snapshot ◄───────────────────┘
//! ```
//!
//! # `Panel` is the extension point
//!
//! Together with [`DomainAgent`](mjx_wk_session::DomainAgent), it is how a phase
//! nobody has scoped yet already has a shape: one agent, one panel, one line in
//! the registry. Adding the network panel in Phase 3 changes nothing written in
//! Phase 1.

pub mod action;
pub mod breakpoint_list;
pub mod call_stack;
pub mod code_view;
pub mod console_view;
pub mod dom_tree;
pub mod flame;
pub mod network_table;
pub mod search;
pub mod source_tree;
pub mod storage_table;
pub mod styles;
pub mod theme;
pub mod variables;

use mjx_wk_dialect::Support;
use mjx_wk_protocol::Domain;

pub use action::{Action, StepKind};
pub use theme::Theme;

/// A stable identifier for a panel, used by the dock and by settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PanelId(pub &'static str);

impl std::fmt::Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// What a panel is handed each frame.
///
/// Everything here is cheap to read: snapshots are `Arc` clones and the theme is
/// borrowed. Nothing in here can block.
#[derive(Debug)]
pub struct PanelCtx<'a> {
    pub theme: &'a Theme,
    /// Answers "may I offer this control?" without touching the wire.
    pub support: &'a dyn SupportQuery,
}

/// Lets a panel ask whether a protocol member is usable here.
///
/// A trait rather than a concrete type so this crate keeps its promise of not
/// depending on the session.
pub trait SupportQuery: std::fmt::Debug {
    /// Whether a member can be used against the attached debuggee.
    fn supports(&self, domain: Domain, member: &str) -> Support;
}

/// One dockable view.
pub trait Panel {
    /// Stable identity.
    fn id(&self) -> PanelId;

    /// The tab label.
    fn title(&self) -> &str;

    /// Protocol members this panel needs.
    ///
    /// If any is [`Support::Unsupported`], the panel is shown disabled with an
    /// explanation rather than hidden. Hiding it would leave a user wondering
    /// where the network tab went on a Chromium WebView; showing it greyed out,
    /// with a reason, answers the question before it is asked.
    fn requires(&self) -> &[(Domain, &'static str)];

    /// Draw, and report what the user did.
    ///
    /// Must return promptly. Anything that could take a millisecond belongs on
    /// the session task, reached by returning an [`Action`].
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>) -> Vec<Action>;
}
