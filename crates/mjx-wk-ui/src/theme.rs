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
        Self {
            is_dark: true,
            background: Color32::from_rgb(0x1e, 0x1e, 0x1e),
            panel: Color32::from_rgb(0x25, 0x25, 0x26),
            gutter: Color32::from_rgb(0x1e, 0x1e, 0x1e),
            hairline: Color32::from_rgb(0x3c, 0x3c, 0x3c),
            text: Color32::from_rgb(0xd4, 0xd4, 0xd4),
            text_dim: Color32::from_rgb(0x85, 0x85, 0x85),
            accent: Color32::from_rgb(0x37, 0x94, 0xff),
            syntax_keyword: Color32::from_rgb(0x56, 0x9c, 0xd6),
            syntax_string: Color32::from_rgb(0xce, 0x91, 0x78),
            syntax_number: Color32::from_rgb(0xb5, 0xce, 0xa8),
            syntax_comment: Color32::from_rgb(0x6a, 0x99, 0x55),
            syntax_function: Color32::from_rgb(0xdc, 0xdc, 0xaa),
            syntax_type: Color32::from_rgb(0x4e, 0xc9, 0xb0),
            syntax_property: Color32::from_rgb(0x9c, 0xdc, 0xfe),
            syntax_tag: Color32::from_rgb(0x56, 0x9c, 0xd6),
            // Chrome-like red for a bound breakpoint.
            breakpoint_resolved: Color32::from_rgb(0xe5, 0x14, 0x00),
            // Same hue as resolved; drawn hollow so pending stays distinct.
            breakpoint_pending: Color32::from_rgb(0xe5, 0x14, 0x00),
            breakpoint_conditional: Color32::from_rgb(0xf5, 0xa6, 0x23),
            breakpoint_logpoint: Color32::from_rgb(0x9b, 0x59, 0xb6),
            // Yellow wash/arrow — must not read as any breakpoint mark.
            execution_line: Color32::from_rgb(0xc6, 0xc6, 0x00),
            row_height: 18.0,
            gutter_width: 72.0,
            indent_width: 16.0,
            monospace_size: 13.0,
        }
    }

    /// The light theme.
    pub fn light() -> Self {
        Self {
            is_dark: false,
            background: Color32::from_rgb(0xff, 0xff, 0xff),
            panel: Color32::from_rgb(0xf3, 0xf3, 0xf3),
            gutter: Color32::from_rgb(0xf3, 0xf3, 0xf3),
            hairline: Color32::from_rgb(0xd4, 0xd4, 0xd4),
            text: Color32::from_rgb(0x1e, 0x1e, 0x1e),
            text_dim: Color32::from_rgb(0x6e, 0x6e, 0x6e),
            accent: Color32::from_rgb(0x00, 0x66, 0xda),
            syntax_keyword: Color32::from_rgb(0x00, 0x00, 0xff),
            syntax_string: Color32::from_rgb(0xa3, 0x15, 0x15),
            syntax_number: Color32::from_rgb(0x09, 0x86, 0x58),
            syntax_comment: Color32::from_rgb(0x00, 0x80, 0x00),
            syntax_function: Color32::from_rgb(0x79, 0x5e, 0x26),
            syntax_type: Color32::from_rgb(0x26, 0x7f, 0x99),
            syntax_property: Color32::from_rgb(0x00, 0x1e, 0x80),
            syntax_tag: Color32::from_rgb(0x80, 0x00, 0x00),
            breakpoint_resolved: Color32::from_rgb(0xe5, 0x14, 0x00),
            breakpoint_pending: Color32::from_rgb(0xe5, 0x14, 0x00),
            breakpoint_conditional: Color32::from_rgb(0xd4, 0x86, 0x00),
            breakpoint_logpoint: Color32::from_rgb(0x8e, 0x44, 0xad),
            execution_line: Color32::from_rgb(0xb0, 0xb0, 0x00),
            row_height: 18.0,
            gutter_width: 72.0,
            indent_width: 16.0,
            monospace_size: 13.0,
        }
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
