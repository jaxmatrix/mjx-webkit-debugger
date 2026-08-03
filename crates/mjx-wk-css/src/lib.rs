//! L4 — matched rules, computed styles, and live editing.
//!
//! **Phase 6.**
//!
//! The Styles panel's whole value is showing *why* a property has the value it
//! has: which rule set it, which rules were overridden, and what was inherited
//! from where. So [`MatchedStyles`] keeps the losers, not just the winner.

use std::sync::Arc;

use async_trait::async_trait;
use mjx_wk_dialect::NormalizedFrame;
use mjx_wk_protocol::Domain;
use mjx_wk_session::{DomainAgent, SessionError, SessionHandle};

/// Whether a declaration took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyStatus {
    /// It applies.
    Active,
    /// A later rule of equal or higher specificity won.
    Inactive,
    /// The property name is not recognised.
    Unknown,
    /// The value is not valid for the property.
    Invalid,
}

/// One declaration.
#[derive(Debug, Clone)]
pub struct CssProperty {
    pub name: String,
    pub value: String,
    pub important: bool,
    pub status: PropertyStatus,
    /// Where it is written, so the panel can link to the stylesheet.
    pub location: Option<mjx_wk_source::SourceLocation>,
}

/// One rule that matched.
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: String,
    /// Which selector in a list matched, for highlighting it.
    pub matching_selector: Option<usize>,
    pub properties: Vec<CssProperty>,
    pub origin: RuleOrigin,
    pub stylesheet: Option<mjx_wk_source::SourceId>,
    /// Enclosing `@media`, `@supports`, `@layer`, `@container`.
    pub groupings: Vec<String>,
}

/// Where a rule came from, which decides cascade order and editability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOrigin {
    Author,
    UserAgent,
    User,
    /// Added by the inspector itself.
    Inspector,
}

/// Everything affecting one element.
#[derive(Debug, Clone, Default)]
pub struct MatchedStyles {
    /// Most specific first.
    pub matched: Vec<CssRule>,
    /// Rules on ancestors that inherit down, outermost last.
    pub inherited: Vec<(mjx_wk_source::NodeId, Vec<CssRule>)>,
    /// Rules on `::before`, `::selection`, and friends.
    pub pseudo: Vec<(String, Vec<CssRule>)>,
    /// The element's own `style` attribute.
    pub inline: Vec<CssProperty>,
}

/// The Styles and Computed panels.
#[derive(Debug, Default)]
pub struct CssModel {
    pub node: Option<mjx_wk_source::NodeId>,
    pub matched: MatchedStyles,
    /// Every property with its final value, alphabetically.
    pub computed: Vec<(String, String)>,
    /// Pseudo-classes forced on for inspection — `:hover` without hovering.
    pub forced_pseudo_classes: Vec<String>,
}

/// Owns Domain::Css.
#[derive(Debug, Default)]
pub struct CssAgent {
    _private: (),
}

#[async_trait]
impl DomainAgent for CssAgent {
    type Model = CssModel;

    const DOMAINS: &'static [Domain] = &[Domain::Css];
    const NAME: &'static str = "mjx-wk-css";

    async fn attach(&mut self, _session: &SessionHandle) -> Result<(), SessionError> {
        todo!("Phase 6 — docs/tasks/T-602-styles-panel.md")
    }

    async fn on_event(&mut self, _event: &NormalizedFrame) -> Result<(), SessionError> {
        todo!("Phase 6 — docs/tasks/T-602-styles-panel.md")
    }

    fn snapshot(&self) -> Arc<Self::Model> {
        todo!("Phase 6 — docs/tasks/T-602-styles-panel.md")
    }
}
