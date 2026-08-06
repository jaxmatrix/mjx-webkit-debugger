//! Search across sources.
//!
//! **Owned by `docs/tasks/T-012-search.md`.**

use mjx_wk_source::search::SearchError;
use mjx_wk_source::{SearchHit, SearchIndex, SearchQuery, SourceId};

use crate::{Action, PanelCtx};

/// A query box and its results.
///
/// Local results appear as the user types (caller runs `SearchIndex::search_local`
/// and passes the hits in). Remote results arrive later; the caller merges with
/// [`SearchIndex::merge_remote`] so the list this widget paints never jumps.
#[derive(Debug)]
pub struct SearchBar {
    text: String,
    case_sensitive: bool,
    is_regex: bool,
    /// Restrict to one source when set (Ctrl-F vs Ctrl-Shift-F).
    within: Option<SourceId>,
    /// Last regex compile error, shown instead of pretending the query is fine.
    regex_error: Option<String>,
    /// Query text for which we last emitted [`Action::Search`], so we do not
    /// spam the session task every frame.
    last_emitted: String,
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            case_sensitive: false,
            is_regex: false,
            within: None,
            regex_error: None,
            last_emitted: String::new(),
        }
    }

    /// Current query, suitable for [`SearchIndex::search_local`] /
    /// [`SearchIndex::search_remote`].
    pub fn query(&self) -> SearchQuery {
        SearchQuery {
            text: self.text.clone(),
            case_sensitive: self.case_sensitive,
            is_regex: self.is_regex,
            within: self.within,
        }
    }

    /// Limit search to one source, or `None` for all sources.
    pub fn set_within(&mut self, within: Option<SourceId>) {
        self.within = within;
    }

    /// Why the current regex cannot run, if any.
    pub fn regex_error(&self) -> Option<&str> {
        self.regex_error.as_deref()
    }

    /// Draw the box and the hit list.
    ///
    /// Local results appear as the user types; remote results arrive later and
    /// merge in. The list must not jump when they do — a result the user is
    /// about to click must not move. This widget paints `hits` in the order
    /// given and never reorders them.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        hits: &[SearchHit],
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let theme = ctx.theme;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.text)
                    .hint_text("Search sources")
                    .desired_width(ui.available_width().clamp(120.0, 280.0))
                    .text_color(theme.text),
            );

            if ui
                .checkbox(&mut self.case_sensitive, "Match case")
                .changed()
                || ui.checkbox(&mut self.is_regex, "Regex").changed()
                || response.changed()
            {
                self.refresh_regex_error();
                self.last_emitted = self.text.clone();
                actions.push(Action::Search(self.text.clone()));
            }

            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.refresh_regex_error();
                self.last_emitted = self.text.clone();
                actions.push(Action::Search(self.text.clone()));
            }
        });

        if let Some(err) = &self.regex_error {
            ui.colored_label(theme.breakpoint_conditional, format!("Invalid regex: {err}"));
        }

        ui.add_space(4.0);

        // Paint hits in caller order. Truncation already happened in SearchIndex;
        // still clip here so a buggy caller cannot blow the frame.
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .max_height(ui.available_height().max(80.0))
            .show(ui, |ui| {
                for (i, hit) in hits.iter().enumerate() {
                    let label = format_hit_label(hit);
                    let response = ui
                        .push_id(i, |ui| {
                            ui.add(
                                egui::Button::new(
                                    egui::RichText::new(label)
                                        .color(theme.text)
                                        .monospace()
                                        .size(theme.monospace_size),
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .frame(false),
                            )
                        })
                        .inner;
                    if response.clicked() {
                        actions.push(Action::OpenSource(
                            hit.location.source,
                            Some(hit.location.line),
                        ));
                    }
                }

                if hits.is_empty() && self.regex_error.is_none() && !self.text.is_empty() {
                    ui.colored_label(theme.text_dim, "No results");
                }
            });

        actions
    }

    fn refresh_regex_error(&mut self) {
        if !self.is_regex || self.text.is_empty() {
            self.regex_error = None;
            return;
        }
        match SearchIndex::check_query(&self.query()) {
            Ok(()) => self.regex_error = None,
            Err(SearchError::InvalidRegex(msg)) => self.regex_error = Some(msg),
        }
    }
}

fn format_hit_label(hit: &SearchHit) -> String {
    const MAX: usize = 240;
    let mut line = hit.line_text.clone();
    if line.len() > MAX {
        let end = floor_char_boundary(&line, MAX);
        line.truncate(end);
        line.push('…');
    }
    // One-based line for humans.
    format!(
        "{}:{}  {}",
        hit.location.source,
        hit.location.line + 1,
        line
    )
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
