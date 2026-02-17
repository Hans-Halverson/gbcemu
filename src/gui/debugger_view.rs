use std::{collections::VecDeque, f32};

use eframe::egui::{
    self, Align, Color32, FontId, Key, Label, Layout, Margin, Pos2, Rect, RichText, ScrollArea,
    TextEdit, TextFormat, TextStyle, Vec2, ViewportId,
    text::{CCursor, CCursorRange, LayoutJob},
};

use crate::gui::shell::EmulatorShellApp;

pub const WINDOW_INNER_SIZE: Vec2 = Vec2::new(400.0, 800.0);
const WINDOW_PADDING: f32 = 4.0;

const SCROLL_AREA_HEIGHT: f32 = 700.0;

const MAX_OUTPUT_LINES: usize = 10000;
const MAX_INPUT_HISTORY_ENTRIES: usize = 10000;

const GBDB_PREFIX: &str = "(gbdb)";

pub struct DebuggerViewport {
    /// Whether the viewport is currently shown
    is_shown: bool,
    /// Initial position of the viewport
    initial_position: Pos2,
    /// Current input in the debugger command line
    current_input: String,
    /// History of input commands entered. Front is the newest entry, back is the oldest.
    input_history: VecDeque<String>,
    /// If currently navigating input history, the index of the current entry
    input_history_cursor: Option<usize>,
    /// Output lines in the debugger console. Earlier lines are at the front of the queue.
    output_lines: VecDeque<OutputLine>,
    /// The last known size of the viewport
    viewport_rect: Rect,
    /// The number of glyphs that fit on a single line in the output section
    num_glyphs_per_output_line: usize,
    /// Width of a single glyph in the monospace font
    monospace_glyph_width: f32,
    /// Height of a single glyph in the monospace font
    monospace_glyph_height: f32,
}

struct OutputLine {
    /// The full text of the output line without wrapping
    text: String,
    /// Total number of glyphs in the line (assumes one char == one glyph in a monospace font)
    num_glyphs: usize,
    /// The wrapped line number that this line starts at
    starting_line: usize,
}

impl OutputLine {
    fn new(text: String, starting_line: usize) -> Self {
        let num_glyphs = text.chars().count();
        Self {
            text,
            num_glyphs,
            starting_line,
        }
    }

    /// Starting line number for the wrapped line after this one.
    ///
    /// Even if empty this line will still fill a full wrapped line.
    fn next_wrapped_line_num(&self, glyphs_per_line: usize) -> usize {
        self.starting_line + self.num_glyphs.max(1).div_ceil(glyphs_per_line)
    }
}

impl DebuggerViewport {
    pub fn new() -> Self {
        Self {
            is_shown: false,
            initial_position: Pos2::ZERO,
            current_input: String::new(),
            input_history: VecDeque::new(),
            input_history_cursor: None,
            output_lines: VecDeque::new(),
            viewport_rect: Rect::NOTHING,
            num_glyphs_per_output_line: 0,
            monospace_glyph_width: 0.0,
            monospace_glyph_height: 0.0,
        }
    }

    pub fn is_shown(&self) -> bool {
        self.is_shown
    }

    pub fn open(&mut self, initial_position: Pos2) {
        self.is_shown = true;
        self.initial_position = initial_position;
    }

    pub fn close(&mut self) {
        self.is_shown = false;
    }
}

impl EmulatorShellApp {
    pub fn debugger_viewport_id(&self) -> ViewportId {
        ViewportId::from_hash_of("debugger_viewport_id")
    }

    pub(super) fn draw_debugger_viewport(&mut self, ui: &mut egui::Ui) {
        ui.ctx().show_viewport_immediate(
            self.debugger_viewport_id(),
            egui::ViewportBuilder::default()
                .with_inner_size(WINDOW_INNER_SIZE)
                .with_min_inner_size(WINDOW_INNER_SIZE)
                .with_position(self.debugger_view().initial_position)
                .with_resizable(true)
                .with_active(true)
                .with_transparent(true)
                .with_title("Debugger"),
            |ctx, _| {
                self.update_measurements(ctx);
                self.update_viewport_rect(ctx);

                self.handle_history_navigation(ctx);

                // Background color and padding for the entire window
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::NONE
                            .fill(Self::background_color())
                            .inner_margin(WINDOW_PADDING),
                    )
                    .show(ctx, |ui| self.draw_debugger_view(ui));
            },
        );
    }

    fn background_color() -> Color32 {
        Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0xD9)
    }

    /// Update all cached internal measurements
    fn update_measurements(&mut self, ctx: &egui::Context) {
        let view = self.debugger_view_mut();
        let monospace_font_id = Self::monospace_font_id(&ctx.style());

        view.monospace_glyph_width = ctx.fonts_mut(|f| f.glyph_width(&monospace_font_id, 'a'));
        view.monospace_glyph_height = ctx.fonts_mut(|f| f.row_height(&monospace_font_id));
    }

    /// Update cached data when viewport is resized
    fn update_viewport_rect(&mut self, ctx: &egui::Context) {
        let new_viewport_rect = ctx.input(|i| i.viewport().inner_rect.unwrap());
        let debugger_view = self.debugger_view_mut();
        if new_viewport_rect == debugger_view.viewport_rect {
            return;
        }

        // Viewport size has changed, recalculate layout
        debugger_view.viewport_rect = new_viewport_rect;

        // Recalculate the number of glyphs that fit on a single line based on the new viewport size
        let output_area_width = debugger_view.viewport_rect.width() - (WINDOW_PADDING * 2.0);
        let new_glyphs_per_line =
            (output_area_width / debugger_view.monospace_glyph_width) as usize;

        // If the number of glyphs per line has changed, recalculate the start of each wrapped
        // output line.
        if debugger_view.num_glyphs_per_output_line != new_glyphs_per_line {
            debugger_view.num_glyphs_per_output_line = new_glyphs_per_line;

            for i in 1..debugger_view.output_lines.len() {
                debugger_view.output_lines[i].starting_line =
                    debugger_view.output_lines[i - 1].next_wrapped_line_num(new_glyphs_per_line);
            }
        }
    }

    fn draw_debugger_view(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            self.draw_scrollable_output_area(ui);

            ui.separator();

            self.draw_text_input(ui);
        });
    }

    fn draw_scrollable_output_area(&mut self, ui: &mut egui::Ui) {
        let scroll_area_size = Vec2::new(ui.available_width(), SCROLL_AREA_HEIGHT);
        let scroll_area_layout = Layout::top_down(Align::LEFT).with_cross_justify(true);

        ui.allocate_ui_with_layout(scroll_area_size, scroll_area_layout, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            let debugger_view = self.debugger_view();
            let num_glyphs_per_line = debugger_view.num_glyphs_per_output_line;
            let row_height = debugger_view.monospace_glyph_height;
            let output_lines = &debugger_view.output_lines;

            let first_row = output_lines
                .front()
                .map(|line| line.starting_line)
                .unwrap_or(0);

            let last_row = output_lines
                .back()
                .map(|line| line.next_wrapped_line_num(num_glyphs_per_line))
                .unwrap_or(0);

            let total_rows = last_row - first_row;

            ScrollArea::vertical()
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show_rows(ui, row_height, total_rows, |ui, rows_to_display| {
                    if output_lines.is_empty() {
                        return;
                    }

                    // OutputLine for the first row. Use `partition_point` to find the first
                    // OutputLine *after* the target row, then take the output line before that.
                    let mut output_line_index = output_lines.partition_point(|line| {
                        rows_to_display.start >= (line.starting_line - first_row)
                    }) - 1;

                    let mut current_output_line = &debugger_view.output_lines[output_line_index];

                    // Index of the wrapped line within the current OutputLine corresponding to the
                    // target row.
                    let mut wrapped_line_index =
                        (current_output_line.starting_line - first_row) - rows_to_display.start;

                    for _ in rows_to_display {
                        // Find the starting char for the target row within the OutputLine
                        let char_index_start = wrapped_line_index * num_glyphs_per_line;
                        let mut chars = current_output_line.text.chars().skip(char_index_start);

                        // Build the range of characters in this line, padding with spaces
                        let mut row_contents = String::new();
                        for _ in 0..num_glyphs_per_line {
                            row_contents.push(chars.next().unwrap_or(' '));
                        }

                        // Draw target row to the screen
                        ui.add(Label::new(LayoutJob::simple_format(
                            row_contents,
                            Self::text_format(ui),
                        )));

                        // If there are still characters left in this OutputLine then wrap to
                        // another line, otherwise move to the next OutputLine.
                        if chars.next().is_some() {
                            wrapped_line_index += 1;
                        } else {
                            wrapped_line_index = 0;
                            output_line_index += 1;

                            if output_line_index >= debugger_view.output_lines.len() {
                                break;
                            }

                            current_output_line = &debugger_view.output_lines[output_line_index];
                        }
                    }
                });
        });
    }

    fn draw_text_input(&mut self, ui: &mut egui::Ui) {
        // Calculate where the `(gbdb)` prefix will be drawn
        let prefix_width = self.debugger_view().monospace_glyph_width * (GBDB_PREFIX.len() as f32);
        let prefix_rect = Rect::from_min_size(ui.cursor().left_top(), Vec2::new(prefix_width, 0.0));

        // Write the `(gbdb)` prefix without taking any space
        ui.place(prefix_rect, |ui: &mut egui::Ui| {
            ui.label(
                RichText::new(GBDB_PREFIX)
                    .color(Color32::GRAY)
                    .font(Self::monospace_font_id(ui.style())),
            )
        });

        // Always write a leading space to avoid a rendering bug in egui where the cursor is
        // rendered in the wrong place when the input is empty.
        if !self.debugger_view().current_input.starts_with(' ') {
            self.debugger_view_mut().current_input.insert(0, ' ');
        }

        // Write the text input field with an offset to account for the prefix
        let text_edit_output = TextEdit::multiline(&mut self.debugger_view_mut().current_input)
            .background_color(Color32::TRANSPARENT)
            .margin(Margin::ZERO)
            .frame(false)
            .lock_focus(true)
            .desired_width(ui.available_width())
            .desired_rows(1)
            .return_key(None)
            .layouter(&mut |ui, text, wrap_width| {
                let mut layout_job = LayoutJob::default();
                Self::set_layout_job_wrapping(&mut layout_job, wrap_width);

                layout_job.append(text.as_str(), prefix_width, Self::text_format(ui));

                ui.fonts_mut(|f| f.layout_job(layout_job))
            })
            .show(ui);

        // Adjust cursor position to always appear after the prefix
        let is_cursor_before_prefix = text_edit_output
            .cursor_range
            .map(|range| range.as_sorted_char_range().start == 0)
            .unwrap_or(false);
        if is_cursor_before_prefix {
            if let Some(mut state) = TextEdit::load_state(ui.ctx(), text_edit_output.response.id) {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(1))));
                state.store(ui.ctx(), text_edit_output.response.id);
            }
        }

        // Lock focus to the input field whenever debugger viewport has focus
        if ui.input(|input| input.focused) {
            text_edit_output.response.request_focus();
        }

        if ui.input(|input| input.key_pressed(Key::Enter)) {
            self.submit_current_line();
        }
    }

    fn monospace_font_id(style: &egui::Style) -> FontId {
        TextStyle::Monospace.resolve(style)
    }

    fn text_format(ui: &egui::Ui) -> TextFormat {
        TextFormat::simple(Self::monospace_font_id(ui.style()), Color32::WHITE)
    }

    /// Create a job for laying text using standard terminal formatting
    fn set_layout_job_wrapping(layout_job: &mut LayoutJob, wrap_width: f32) {
        layout_job.wrap.max_width = wrap_width;
        layout_job.wrap.break_anywhere = true;
    }

    fn handle_history_navigation(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(Key::ArrowUp)) {
            // Increment the input history cursor
            let input_history_length = self.debugger_view().input_history.len();
            let input_history_cursor = self
                .debugger_view()
                .input_history_cursor
                .map_or(0, |cursor| {
                    (cursor + 1).min(input_history_length.saturating_sub(1))
                });

            self.debugger_view_mut().input_history_cursor = Some(input_history_cursor);
            self.navigate_to_history_entry(input_history_cursor);
        } else if ctx.input(|input| input.key_pressed(Key::ArrowDown)) {
            if let Some(cursor) = self.debugger_view().input_history_cursor {
                // If at the bottom of the history, clear the input
                if cursor == 0 {
                    self.debugger_view_mut().input_history_cursor = None;
                    self.debugger_view_mut().current_input = String::new();
                    return;
                }

                // Decrement the input history cursor
                let input_history_cursor = cursor - 1;
                self.debugger_view_mut().input_history_cursor = Some(input_history_cursor);
                self.navigate_to_history_entry(input_history_cursor);
            }
        }
    }

    fn navigate_to_history_entry(&mut self, index: usize) {
        if let Some(entry) = self.debugger_view().input_history.get(index) {
            self.debugger_view_mut().current_input = entry.clone();
        }
    }

    fn submit_current_line(&mut self) {
        let mut raw_input_line =
            std::mem::replace(&mut self.debugger_view_mut().current_input, String::new());

        // Strip the leading space that is always added
        if raw_input_line.starts_with(' ') {
            raw_input_line.remove(0);
        }

        if !raw_input_line.is_empty() {
            self.push_history_entry(raw_input_line.clone());
        }

        self.debugger_view_mut().input_history_cursor = None;

        self.push_output_line(format!("{} {}", GBDB_PREFIX, raw_input_line));
    }

    fn push_output_line(&mut self, line: String) {
        let debugger_view = self.debugger_view_mut();
        let num_glyphs_per_line = debugger_view.num_glyphs_per_output_line;
        let output_lines = &mut debugger_view.output_lines;

        if output_lines.len() >= MAX_OUTPUT_LINES {
            output_lines.pop_front();
        }

        let next_line_num = output_lines
            .back()
            .map(|line| line.next_wrapped_line_num(num_glyphs_per_line))
            .unwrap_or(0);

        output_lines.push_back(OutputLine::new(line, next_line_num));
        // self.debugger_view_mut().scroll_layout_on_next_frame = true;
    }

    fn push_history_entry(&mut self, entry: String) {
        let input_history = &mut self.debugger_view_mut().input_history;

        if input_history.len() >= MAX_INPUT_HISTORY_ENTRIES {
            input_history.pop_back();
        }

        input_history.push_front(entry);
    }
}
