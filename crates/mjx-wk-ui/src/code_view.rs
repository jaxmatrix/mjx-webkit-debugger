//! The source editor.
//!
//! **Owned by `docs/tasks/T-008-code-view.md`.** The single most demanding
//! widget in the application.
//!
//! # Virtualisation is the whole design
//!
//! A 5 MB bundle can be 200 000 lines, or one line 5 MB long. Both must scroll
//! at 60 fps. So:
//!
//! - lay out only the visible rows plus a small margin;
//! - ask the highlighter only for that window;
//! - never measure the whole text to size the scroll area — use
//!   `line_count * row_height`, which is why the font is monospace and the row
//!   height fixed;
//! - clip a very long line horizontally rather than wrapping it, or a minified
//!   file becomes one row several million pixels tall.
//!
//! # The gutter carries five states
//!
//! Empty, resolved, pending, conditional, logpoint — plus the execution-line
//! marker, which is not a breakpoint and must not look like one. `DESIGN.md`
//! specifies each.

use std::ops::Range;

use egui::{
    Color32, CornerRadius, FontId, Galley, Pos2, Rect, Sense, Shape, Stroke, TextFormat, Vec2,
    epaint::text::LayoutJob, pos2,
};

use mjx_wk_source::{HighlightSpan, SourceId, SourceLocation, SourceText};

use crate::breakpoint_list::{BreakpointEdit, encode_probe_action, probe_supported};
use crate::{Action, PanelCtx, Theme};

/// Extra lines requested around the viewport for highlighting. Keeps scroll
/// from flashing unhighlighted rows without highlighting the whole file.
pub const HIGHLIGHT_MARGIN_LINES: u32 = 5;

/// Soft cap on painted characters per line. A 5 MB minified line must not build
/// a multi-megabyte galley; the row clips horizontally instead.
const MAX_PAINT_CHARS: usize = 512;

/// A virtualised, syntax-highlighted, breakpoint-aware source view.
#[derive(Debug, Default)]
pub struct CodeView {
    /// When set, the next `ui` centres this zero-based line in the viewport.
    reveal_line: Option<u32>,
    /// Visible line range from the last frame (zero-based, end-exclusive).
    last_visible: Range<u32>,
    /// Reused so line-number labels do not allocate every row every frame.
    line_no_buf: String,
    /// Condition / logpoint / probe editor opened from the gutter context menu.
    gutter_edit: Option<BreakpointEdit>,
    /// Request focus on the gutter editor once after it opens.
    focus_gutter_edit: bool,
}

impl CodeView {
    pub fn new() -> Self {
        Self {
            reveal_line: None,
            last_visible: 0..0,
            line_no_buf: String::with_capacity(8),
            gutter_edit: None,
            focus_gutter_edit: false,
        }
    }

    /// Open gutter editor, if any. Useful from tests.
    pub fn gutter_edit(&self) -> Option<&BreakpointEdit> {
        self.gutter_edit.as_ref()
    }

    /// Open a condition / logpoint / probe editor (gutter menu / tests).
    pub fn begin_gutter_edit(&mut self, edit: BreakpointEdit) {
        self.gutter_edit = Some(edit);
        self.focus_gutter_edit = true;
    }

    /// Visible lines from the previous frame — what the highlighter should cover
    /// (expand with [`highlight_window`] before calling).
    pub fn last_visible_line_range(&self) -> Range<u32> {
        self.last_visible.clone()
    }

    /// Expand a viewport range by [`HIGHLIGHT_MARGIN_LINES`], clamped to the file.
    pub fn highlight_window(visible: Range<u32>, line_count: u32) -> Range<u32> {
        if line_count == 0 {
            return 0..0;
        }
        let start = visible.start.saturating_sub(HIGHLIGHT_MARGIN_LINES);
        let end = visible
            .end
            .saturating_add(HIGHLIGHT_MARGIN_LINES)
            .min(line_count);
        start..end
    }

    /// Draw the visible window of a source.
    ///
    /// `model` carries the text, its highlight spans, the breakpoints in it,
    /// and the paused location if execution stopped here.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        model: &CodeViewModel<'_>,
    ) -> Vec<Action> {
        self.paint(ui, ctx, model)
    }

    /// Scroll so a line is visible, centring it if it is far off screen.
    pub fn reveal_line(&mut self, line: u32) {
        self.reveal_line = Some(line);
    }

    fn paint(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        model: &CodeViewModel<'_>,
    ) -> Vec<Action> {
        let theme = ctx.theme;
        let row_height = theme.row_height;
        let line_count = model.text.line_count();
        let source_id = model.text.source_id();

        // Fixed row pitch: scroll height is exactly line_count × row_height.
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.spacing_mut().item_spacing.x = 0.0;

        let mut actions = Vec::new();
        let viewport_height = ui.available_height();

        let mut scroll = egui::ScrollArea::both()
            .id_salt("mjx.code_view")
            .auto_shrink([false, false]);

        if let Some(line) = self.reveal_line.take() {
            let line = line.min(line_count.saturating_sub(1));
            let target = (line as f32 * row_height) - (viewport_height * 0.5) + (row_height * 0.5);
            scroll = scroll.vertical_scroll_offset(target.max(0.0));
        }

        let font = FontId::monospace(theme.monospace_size);
        // Monospace advance used only to decide how many characters fit — never
        // to size the scroll area.
        let char_width = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);

        let output = scroll.show_rows(ui, row_height, line_count as usize, |ui, row_range| {
            let start = row_range.start as u32;
            let end = row_range.end as u32;
            self.last_visible = start..end;

            let available_code_width = (ui.available_width() - theme.gutter_width).max(0.0);

            for row in row_range {
                let line = row as u32;
                let y = ui.cursor().top();
                let full = Rect::from_min_size(
                    pos2(ui.max_rect().left(), y),
                    Vec2::new(ui.available_width(), row_height),
                );

                if model.execution_line == Some(line) {
                    let wash = Color32::from_rgba_unmultiplied(
                        theme.execution_line.r(),
                        theme.execution_line.g(),
                        theme.execution_line.b(),
                        40,
                    );
                    ui.painter().rect_filled(full, CornerRadius::ZERO, wash);
                }

                let gutter =
                    Rect::from_min_size(full.min, Vec2::new(theme.gutter_width, row_height));
                ui.painter()
                    .rect_filled(gutter, CornerRadius::ZERO, theme.gutter);

                let mark = model
                    .breakpoints
                    .iter()
                    .find(|(l, _)| *l == line)
                    .map(|(_, m)| *m);

                let gutter_id = ui.id().with(("gutter", line));
                let gutter_resp = ui.interact(gutter, gutter_id, Sense::click());
                if gutter_resp.clicked() {
                    actions.push(Action::ToggleBreakpoint(SourceLocation::line_start(
                        source_id, line,
                    )));
                }

                let loc = SourceLocation::line_start(source_id, line);
                let has_breakpoint = mark.is_some();
                let is_disabled = mark == Some(BreakpointMark::Disabled);
                gutter_resp.context_menu(|ui| {
                    if ui.button("Edit condition…").clicked() {
                        self.gutter_edit = Some(BreakpointEdit::Condition {
                            location: loc,
                            draft: String::new(),
                        });
                        self.focus_gutter_edit = true;
                        ui.close();
                    }
                    if ui.button("Add logpoint…").clicked() {
                        self.gutter_edit = Some(BreakpointEdit::Logpoint {
                            location: loc,
                            draft: String::new(),
                        });
                        self.focus_gutter_edit = true;
                        ui.close();
                    }
                    if probe_supported(ctx) && ui.button("Add probe…").clicked() {
                        self.gutter_edit = Some(BreakpointEdit::Probe {
                            location: loc,
                            draft: String::new(),
                        });
                        self.focus_gutter_edit = true;
                        ui.close();
                    }
                    if has_breakpoint && !is_disabled && ui.button("Disable").clicked() {
                        actions.push(Action::ToggleBreakpoint(loc));
                        ui.close();
                    }
                    if is_disabled && ui.button("Enable").clicked() {
                        actions.push(Action::ToggleBreakpoint(loc));
                        ui.close();
                    }
                    if has_breakpoint && ui.button("Remove breakpoint").clicked() {
                        actions.push(Action::RemoveBreakpoint(loc));
                        ui.close();
                    }
                });

                // AccessKit / snapshot distinguishability for each mark.
                if let Some(mark) = mark {
                    gutter_resp.clone().on_hover_text(mark.access_label());
                    // `widget_info` keeps kittest labels stable without depending on hover.
                    gutter_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            mark.access_label(),
                        )
                    });
                } else {
                    gutter_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "gutter empty")
                    });
                }

                let mark_center = pos2(gutter.left() + 10.0, gutter.center().y);
                if let Some(mark) = mark {
                    paint_breakpoint_mark(ui, mark_center, mark, theme);
                }
                if model.execution_line == Some(line) {
                    paint_execution_arrow(ui, mark_center, theme.execution_line);
                }

                self.line_no_buf.clear();
                // Display is one-based; protocol lines stay zero-based.
                use std::fmt::Write as _;
                let _ = write!(self.line_no_buf, "{}", line + 1);
                let line_no_pos = pos2(gutter.left() + 22.0, gutter.top());
                ui.painter().text(
                    line_no_pos,
                    egui::Align2::LEFT_TOP,
                    self.line_no_buf.as_str(),
                    font.clone(),
                    theme.text_dim,
                );

                let hair_x = gutter.right() - 1.0;
                ui.painter().line_segment(
                    [pos2(hair_x, gutter.top()), pos2(hair_x, gutter.bottom())],
                    Stroke::new(1.0, theme.hairline),
                );

                let code_rect = Rect::from_min_max(pos2(gutter.right(), full.top()), full.max);

                let Some(raw_line) = model.text.line(line) else {
                    ui.allocate_rect(full, Sense::hover());
                    continue;
                };

                // Clip: never wrap, never lay out megabytes of a minified line.
                let max_chars =
                    ((available_code_width / char_width).ceil() as usize).clamp(1, MAX_PAINT_CHARS);
                let painted = truncate_chars(raw_line, max_chars);

                let span_idx = line
                    .checked_sub(model.spans_start_line)
                    .and_then(|i| usize::try_from(i).ok());
                let spans = span_idx
                    .and_then(|i| model.spans.get(i))
                    .map(|s| s.as_slice())
                    .unwrap_or(&[]);

                let galley = layout_line(ui, painted, spans, theme, font.clone());
                let text_pos = pos2(code_rect.left() + 4.0, code_rect.top());
                ui.painter()
                    .with_clip_rect(code_rect)
                    .galley(text_pos, galley, theme.text);

                if let Some((_, value)) = model.inline_values.iter().find(|(l, _)| *l == line) {
                    let galley = ui.fonts_mut(|f| {
                        f.layout_no_wrap(value.clone(), font.clone(), theme.text_dim)
                    });
                    let x = code_rect.right() - galley.size().x - 6.0;
                    ui.painter().with_clip_rect(code_rect).galley(
                        pos2(x.max(code_rect.left()), code_rect.top()),
                        galley,
                        theme.text_dim,
                    );
                }

                // Advance the cursor by exactly one fixed row.
                ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::hover());
            }
        });

        // Assertable without measuring text: egui sizes content to rows × height
        // (with item_spacing.y = 0, that is exactly line_count × row_height).
        let _ = output;

        if let Some(edit_actions) = self.paint_gutter_edit(ui, ctx) {
            actions.extend(edit_actions);
        }

        actions
    }

    fn paint_gutter_edit(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>) -> Option<Vec<Action>> {
        let theme = ctx.theme;
        let Some(edit) = self.gutter_edit.as_mut() else {
            return None;
        };

        let (title, hint) = match edit {
            BreakpointEdit::Condition { .. } => ("Condition", "Break when true"),
            BreakpointEdit::Logpoint { .. } => ("Logpoint", "Message to log"),
            BreakpointEdit::Probe { .. } => ("Probe", "Expression to sample"),
        };

        let mut submit = false;
        let mut cancel = false;
        let mut actions = Vec::new();
        let want_focus = self.focus_gutter_edit;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).color(theme.accent));
            let draft = match edit {
                BreakpointEdit::Condition { draft, .. }
                | BreakpointEdit::Logpoint { draft, .. }
                | BreakpointEdit::Probe { draft, .. } => draft,
            };
            let response = ui.add(
                egui::TextEdit::singleline(draft)
                    .hint_text(hint)
                    .desired_width(ui.available_width().max(120.0))
                    .font(egui::TextStyle::Monospace)
                    .id_salt("mjx_gutter_bp_edit"),
            );
            if want_focus {
                response.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && (response.has_focus() || response.lost_focus()) {
                submit = true;
            }
            if ui.button("Set").clicked() {
                submit = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
        self.focus_gutter_edit = false;

        if cancel {
            self.gutter_edit = None;
            return Some(actions);
        }
        if submit {
            let finished = self.gutter_edit.take();
            if let Some(edit) = finished {
                match edit {
                    BreakpointEdit::Condition { location, draft } => {
                        let value = {
                            let t = draft.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t.to_owned())
                            }
                        };
                        actions.push(Action::SetBreakpointCondition(location, value));
                    }
                    BreakpointEdit::Logpoint { location, draft } => {
                        let value = {
                            let t = draft.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t.to_owned())
                            }
                        };
                        actions.push(Action::SetBreakpointAction(location, value));
                    }
                    BreakpointEdit::Probe { location, draft } => {
                        let value = {
                            let t = draft.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(encode_probe_action(t))
                            }
                        };
                        actions.push(Action::SetBreakpointAction(location, value));
                    }
                }
            }
            return Some(actions);
        }
        Some(actions)
    }
}

/// What the code view needs for one frame. Borrowed, never owned: this is
/// rebuilt every frame and must not allocate.
#[derive(Debug)]
pub struct CodeViewModel<'a> {
    pub text: &'a dyn CodeViewText,
    /// Spans for a window of lines starting at [`Self::spans_start_line`].
    pub spans: &'a [Vec<HighlightSpan>],
    /// Line number (zero-based) that `spans[0]` describes.
    pub spans_start_line: u32,
    /// Which lines carry a breakpoint, and in what state.
    pub breakpoints: &'a [(u32, BreakpointMark)],
    /// Where execution is stopped, if it is stopped here.
    pub execution_line: Option<u32>,
    /// Probe values to render inline at the end of a line — WebKit's live
    /// gutter values, which Chrome has no equivalent for.
    pub inline_values: &'a [(u32, String)],
}

/// Line access the code view needs. Production code passes [`SourceText`] once
/// T-005 fills it; tests pass [`SyntheticSource`] while that peer is unfinished.
pub trait CodeViewText: std::fmt::Debug {
    fn source_id(&self) -> SourceId;
    fn line_count(&self) -> u32;
    fn line(&self, line: u32) -> Option<&str>;
}

impl CodeViewText for SourceText {
    fn source_id(&self) -> SourceId {
        self.id()
    }

    fn line_count(&self) -> u32 {
        self.index().line_count()
    }

    fn line(&self, line: u32) -> Option<&str> {
        SourceText::line(self, line)
    }
}

/// In-test / pre-T-005 stand-in for [`SourceText`].
#[derive(Debug)]
pub struct SyntheticSource<'a> {
    pub id: SourceId,
    pub line_count: u32,
    /// Returned for every in-range line. Distinct per-line bodies are optional;
    /// a shared slice keeps a 200 000-line scroll fixture allocation-free.
    pub line: &'a str,
    /// Optional per-line bodies. When set and long enough, preferred over [`Self::line`].
    pub lines: Option<&'a [&'a str]>,
}

impl CodeViewText for SyntheticSource<'_> {
    fn source_id(&self) -> SourceId {
        self.id
    }

    fn line_count(&self) -> u32 {
        self.line_count
    }

    fn line(&self, line: u32) -> Option<&str> {
        if line >= self.line_count {
            return None;
        }
        if let Some(lines) = self.lines {
            return lines.get(line as usize).copied();
        }
        Some(self.line)
    }
}

/// How a breakpoint should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointMark {
    Resolved,
    /// Set, but no matching script has parsed. Hollow, as in Chrome.
    Pending,
    Conditional,
    Logpoint,
    Disabled,
}

impl BreakpointMark {
    fn access_label(self) -> &'static str {
        match self {
            Self::Resolved => "breakpoint resolved",
            Self::Pending => "breakpoint pending",
            Self::Conditional => "breakpoint conditional",
            Self::Logpoint => "breakpoint logpoint",
            Self::Disabled => "breakpoint disabled",
        }
    }

    fn color(self, theme: &Theme) -> Color32 {
        match self {
            Self::Resolved => theme.breakpoint_resolved,
            Self::Pending => theme.breakpoint_pending,
            Self::Conditional => theme.breakpoint_conditional,
            Self::Logpoint => theme.breakpoint_logpoint,
            Self::Disabled => theme.text_dim,
        }
    }
}

fn paint_breakpoint_mark(ui: &egui::Ui, center: Pos2, mark: BreakpointMark, theme: &Theme) {
    let color = mark.color(theme);
    let painter = ui.painter();
    let r = 5.0;
    match mark {
        BreakpointMark::Pending => {
            // Hollow — Chrome's unbound marker.
            painter.circle_stroke(center, r, Stroke::new(1.5, color));
        }
        BreakpointMark::Resolved | BreakpointMark::Disabled => {
            painter.circle_filled(center, r, color);
        }
        BreakpointMark::Conditional => {
            painter.circle_filled(center, r, color);
            // Small tick so conditional stays distinct from resolved at a glance.
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "?",
                FontId::monospace(9.0),
                theme.background,
            );
        }
        BreakpointMark::Logpoint => {
            // Diamond — distinct shape from every circle mark and from the arrow.
            let d = 5.5;
            let points = vec![
                pos2(center.x, center.y - d),
                pos2(center.x + d, center.y),
                pos2(center.x, center.y + d),
                pos2(center.x - d, center.y),
            ];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
    }
}

fn paint_execution_arrow(ui: &egui::Ui, center: Pos2, color: Color32) {
    // Right-pointing chevron in the gutter — shape and colour both differ from
    // every breakpoint mark (circles / diamond).
    let painter = ui.painter();
    let points = vec![
        pos2(center.x - 4.0, center.y - 6.0),
        pos2(center.x + 5.0, center.y),
        pos2(center.x - 4.0, center.y + 6.0),
    ];
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

fn layout_line(
    ui: &egui::Ui,
    line: &str,
    spans: &[HighlightSpan],
    theme: &Theme,
    font: FontId,
) -> std::sync::Arc<Galley> {
    let mut job = LayoutJob::default();
    job.wrap.break_anywhere = false;
    job.wrap.max_width = f32::INFINITY;

    if spans.is_empty() {
        job.append(
            line,
            0.0,
            TextFormat {
                font_id: font,
                color: theme.text,
                ..Default::default()
            },
        );
    } else {
        let bytes = line.as_bytes();
        let mut cursor = 0usize;
        for span in spans {
            let start = span.range.start as usize;
            let end = (span.range.end as usize).min(bytes.len());
            if start > cursor && cursor < bytes.len() {
                let end_plain = start.min(bytes.len());
                if let Ok(plain) = std::str::from_utf8(&bytes[cursor..end_plain]) {
                    job.append(
                        plain,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: theme.text,
                            ..Default::default()
                        },
                    );
                }
            }
            if start < bytes.len() && end > start {
                let s = start.min(bytes.len());
                let e = end.min(bytes.len());
                if let Ok(frag) = std::str::from_utf8(&bytes[s..e]) {
                    job.append(
                        frag,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: theme.syntax(span.kind),
                            ..Default::default()
                        },
                    );
                }
            }
            cursor = end.max(cursor);
        }
        if cursor < bytes.len()
            && let Ok(rest) = std::str::from_utf8(&bytes[cursor..])
        {
            job.append(
                rest,
                0.0,
                TextFormat {
                    font_id: font,
                    color: theme.text,
                    ..Default::default()
                },
            );
        }
    }

    ui.fonts_mut(|f| f.layout_job(job))
}

fn truncate_chars(s: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod scroll_sizing {
    //! Pure checks that do not need egui_kittest.

    #[test]
    fn highlight_window_adds_margin() {
        let w = super::CodeView::highlight_window(10..20, 100);
        assert_eq!(
            w,
            (10 - super::HIGHLIGHT_MARGIN_LINES)..(20 + super::HIGHLIGHT_MARGIN_LINES)
        );
    }

    #[test]
    fn highlight_window_clamps() {
        let w = super::CodeView::highlight_window(0..3, 10);
        assert_eq!(w.start, 0);
        assert_eq!(w.end, 3 + super::HIGHLIGHT_MARGIN_LINES);
        let w = super::CodeView::highlight_window(8..10, 10);
        assert_eq!(w.end, 10);
    }

    #[test]
    fn truncate_chars_respects_boundaries() {
        assert_eq!(super::truncate_chars("abcdef", 3), "abc");
        assert_eq!(super::truncate_chars("a😀c", 2), "a😀");
        assert_eq!(super::truncate_chars("ab", 10), "ab");
    }
}
