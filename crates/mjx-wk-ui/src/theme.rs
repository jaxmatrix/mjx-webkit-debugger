//! Colour and metric tokens.
//!
//! Widgets never write a literal colour. They ask the theme, which is what lets
//! light and dark be one code path — see `DESIGN.md`, which owns this contract:
//! **if you change a token here, change it there in the same commit.**

use egui::Color32;

/// The colours and metrics every widget draws with.
#[derive(Debug, Clone)]
pub struct Theme {
    pub is_dark: bool,

    // Surfaces.
    pub background: Color32,
    pub panel: Color32,
    pub gutter: Color32,
    pub hairline: Color32,

    // Text.
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,

    // Syntax, mapped from `mjx_wk_source::HighlightKind`.
    pub syntax_keyword: Color32,
    pub syntax_string: Color32,
    pub syntax_number: Color32,
    pub syntax_comment: Color32,
    pub syntax_function: Color32,
    pub syntax_type: Color32,
    pub syntax_property: Color32,
    pub syntax_tag: Color32,

    // Debugger states. These are load-bearing, not decorative: a user must be
    // able to tell a resolved breakpoint from a pending one at a glance, which
    // is Chrome's blue-versus-hollow distinction.
    pub breakpoint_resolved: Color32,
    pub breakpoint_pending: Color32,
    pub breakpoint_conditional: Color32,
    pub breakpoint_logpoint: Color32,
    pub execution_line: Color32,

    // Metrics.
    pub row_height: f32,
    pub gutter_width: f32,
    pub indent_width: f32,
    pub monospace_size: f32,
}

impl Theme {
    /// The dark theme. The default, as in every other developer tool.
    pub fn dark() -> Self {
        todo!("T-008 — token values are specified in DESIGN.md")
    }

    /// The light theme.
    pub fn light() -> Self {
        todo!("T-008 — token values are specified in DESIGN.md")
    }

    /// Follow the host's preference.
    pub fn from_visuals(visuals: &egui::Visuals) -> Self {
        if visuals.dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// The colour for a highlight kind.
    pub fn syntax(&self, kind: mjx_wk_source::HighlightKind) -> Color32 {
        use mjx_wk_source::HighlightKind as K;
        match kind {
            K::Keyword => self.syntax_keyword,
            K::String | K::Regex => self.syntax_string,
            K::Number | K::Constant => self.syntax_number,
            K::Comment => self.syntax_comment,
            K::Function => self.syntax_function,
            K::Type => self.syntax_type,
            K::Property | K::Attribute => self.syntax_property,
            K::Tag => self.syntax_tag,
            K::Variable | K::Operator | K::Punctuation => self.text,
        }
    }
}
