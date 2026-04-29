use markdown_tui::widget::MarkdownWidget;
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Margin,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::app::ui::Styles;

use super::{Editor, EditorMode, highlight::patch_preview_highlights};

impl Editor<'_> {
    pub fn render_widget(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        styles: &Styles,
        search_query: Option<&str>,
    ) {
        if self.show_preview {
            if self.is_active && matches!(self.mode, EditorMode::Insert) {
                self.render_wrap_edit(frame, area, styles);
            } else {
                self.last_wrap_width = None;
                self.render_preview(frame, area, styles, search_query);
            }
            return;
        }
        self.last_wrap_width = None;
        let mut title = "Content".to_owned();
        if self.is_active {
            let mode_caption = match self.mode {
                EditorMode::Normal => " - NORMAL",
                EditorMode::Insert => " - EDIT",
                EditorMode::Visual => " - Visual",
            };
            title.push_str(mode_caption);
        }
        if self.has_unsaved {
            title.push_str(" *");
        }

        let estyles = &styles.editor;

        let text_block_style = match (self.mode, self.is_active) {
            (EditorMode::Insert, _) => estyles.block_insert,
            (EditorMode::Visual, _) => estyles.block_visual,
            (EditorMode::Normal, true) => estyles.block_normal_active,
            (EditorMode::Normal, false) => estyles.block_normal_inactive,
        };

        self.text_area.set_block(
            Block::default()
                .borders(Borders::ALL)
                .style(text_block_style)
                .title(title),
        );

        let cursor_style = if self.is_active {
            let s = match self.mode {
                EditorMode::Normal => estyles.cursor_normal,
                EditorMode::Insert => estyles.cursor_insert,
                EditorMode::Visual => estyles.cursor_visual,
            };
            Style::from(s)
        } else {
            Style::reset()
        };
        self.text_area.set_cursor_style(cursor_style);

        self.text_area.set_cursor_line_style(Style::reset());

        self.text_area.set_style(Style::reset());

        self.text_area
            .set_selection_style(Style::default().bg(Color::White).fg(Color::Black));

        frame.render_widget(&self.text_area, area);

        self.render_vertical_scrollbar(frame, area);
        self.render_horizontal_scrollbar(frame, area);
    }

    fn render_preview(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        styles: &Styles,
        search_query: Option<&str>,
    ) {
        let mut title = "Preview".to_owned();
        if self.is_active {
            let mode_caption = match self.mode {
                EditorMode::Normal => " - NORMAL",
                EditorMode::Insert => " - EDIT",
                EditorMode::Visual => " - Visual",
            };
            title.push_str(mode_caption);
        }
        if self.has_unsaved {
            title.push_str(" *");
        }

        let estyles = &styles.editor;
        let block_style = match (self.mode, self.is_active) {
            (EditorMode::Insert, _) => estyles.block_insert,
            (EditorMode::Visual, _) => estyles.block_visual,
            (EditorMode::Normal, true) => estyles.block_normal_active,
            (EditorMode::Normal, false) => estyles.block_normal_inactive,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(block_style)
            .title_style(block_style)
            .title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let content = self.get_content();
        frame.render_widget(
            MarkdownWidget::new(&content).scroll(self.preview_scroll),
            inner,
        );

        if let Some(query) = search_query.filter(|q| !q.is_empty()) {
            patch_preview_highlights(
                frame.buffer_mut(),
                inner,
                query,
                styles.general.search_highlight.into(),
            );
        }

        self.render_preview_scrollbar(frame, area, inner);
    }

    fn render_wrap_edit(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let mut title = "Preview".to_owned();
        let mode_caption = match self.mode {
            EditorMode::Normal => " - NORMAL",
            EditorMode::Insert => " - EDIT",
            EditorMode::Visual => " - Visual",
        };
        title.push_str(mode_caption);
        if self.has_unsaved {
            title.push_str(" *");
        }

        let estyles = &styles.editor;
        let block_style = match self.mode {
            EditorMode::Insert => estyles.block_insert,
            EditorMode::Visual => estyles.block_visual,
            EditorMode::Normal => estyles.block_normal_active,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(block_style)
            .title_style(block_style)
            .title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        self.last_wrap_width = Some(inner.width);

        let lines: Vec<&str> = self.text_area.lines().iter().map(String::as_str).collect();
        let (cursor_row, cursor_col) = self.text_area.cursor();
        let cursor_visual = wrapped_cursor_position(&lines, cursor_row, cursor_col, inner.width);

        if let Some((vrow, _)) = cursor_visual {
            if vrow < self.preview_scroll {
                self.preview_scroll = vrow;
            } else if inner.height > 0 && vrow >= self.preview_scroll + inner.height {
                self.preview_scroll = vrow + 1 - inner.height;
            }
        }

        let visual_lines = char_wrap_lines(&lines, inner.width);
        let paragraph = Paragraph::new(visual_lines).scroll((self.preview_scroll, 0));
        frame.render_widget(paragraph, inner);

        if let Some((vrow, vcol)) = cursor_visual {
            let scrolled_row = vrow.saturating_sub(self.preview_scroll);
            if vrow >= self.preview_scroll && scrolled_row < inner.height && vcol < inner.width {
                frame.set_cursor_position((inner.x + vcol, inner.y + scrolled_row));
            }
        }
    }

    fn render_preview_scrollbar(&mut self, frame: &mut Frame, area: Rect, inner: Rect) {
        let total_lines = self.text_area.lines().len();
        if total_lines as u16 <= inner.height {
            return;
        }

        let mut state = ScrollbarState::default()
            .content_length(total_lines)
            .position(self.preview_scroll as usize);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some(symbols::line::VERTICAL))
            .thumb_symbol(symbols::block::FULL);

        let scroll_area = area.inner(Margin {
            horizontal: 0,
            vertical: 1,
        });

        frame.render_stateful_widget(scrollbar, scroll_area, &mut state);
    }

    pub fn render_vertical_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        let lines_count = self.text_area.lines().len();

        if lines_count as u16 <= area.height - 2 {
            return;
        }

        let (row, _) = self.text_area.cursor();

        let mut state = ScrollbarState::default()
            .content_length(lines_count)
            .position(row);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some(symbols::line::VERTICAL))
            .thumb_symbol(symbols::block::FULL);

        let scroll_area = area.inner(Margin {
            horizontal: 0,
            vertical: 1,
        });

        frame.render_stateful_widget(scrollbar, scroll_area, &mut state);
    }

    pub fn render_horizontal_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        let max_width = self
            .text_area
            .lines()
            .iter()
            .map(|line| line.len())
            .max()
            .unwrap_or_default();

        if max_width as u16 <= area.width - 2 {
            return;
        }

        let (_, col) = self.text_area.cursor();

        let mut state = ScrollbarState::default()
            .content_length(max_width)
            .position(col);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .begin_symbol(Some("◄"))
            .end_symbol(Some("►"))
            .track_symbol(Some(symbols::line::HORIZONTAL))
            .thumb_symbol("🬋");

        let scroll_area = area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });

        frame.render_stateful_widget(scrollbar, scroll_area, &mut state);
    }
}

fn char_wrap_lines<'a>(lines: &[&'a str], width: u16) -> Vec<Line<'a>> {
    if width == 0 {
        return lines.iter().map(|l| Line::raw(l.to_string())).collect();
    }
    let width_us = width as usize;
    let mut out: Vec<Line<'a>> = Vec::with_capacity(lines.len());
    for line in lines {
        if line.is_empty() {
            out.push(Line::raw(String::new()));
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(width_us) {
            let s: String = chunk.iter().collect();
            out.push(Line::raw(s));
        }
    }
    out
}

pub(super) fn visual_to_source(
    lines: &[&str],
    target_vrow: u16,
    vcol: u16,
    width: u16,
) -> (usize, usize) {
    if width == 0 || lines.is_empty() {
        return (0, 0);
    }
    let width_us = width as usize;
    let target_vrow_us = target_vrow as usize;
    let vcol_us = vcol as usize;
    let mut visual_row_acc: usize = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_chars = line.chars().count();
        let rows_for_line = if line_chars == 0 {
            1
        } else {
            line_chars.div_ceil(width_us)
        };
        if target_vrow_us < visual_row_acc + rows_for_line {
            let row_within = target_vrow_us - visual_row_acc;
            let target_col = row_within * width_us + vcol_us;
            return (i, target_col.min(line_chars));
        }
        visual_row_acc += rows_for_line;
    }
    let last = lines.len() - 1;
    (last, lines[last].chars().count())
}

pub(super) fn wrapped_cursor_position(
    lines: &[&str],
    cursor_row: usize,
    cursor_col: usize,
    width: u16,
) -> Option<(u16, u16)> {
    if width == 0 {
        return None;
    }
    let width_us = width as usize;
    let mut visual_row: usize = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_chars = line.chars().count();
        if i < cursor_row {
            let rows = if line_chars == 0 {
                1
            } else {
                line_chars.div_ceil(width_us)
            };
            visual_row = visual_row.saturating_add(rows);
        } else {
            let col = cursor_col.min(line_chars);
            let added = col / width_us;
            let visual_col = col % width_us;
            return Some((
                u16::try_from(visual_row.saturating_add(added)).unwrap_or(u16::MAX),
                u16::try_from(visual_col).unwrap_or(u16::MAX),
            ));
        }
    }
    Some((u16::try_from(visual_row).unwrap_or(u16::MAX), 0))
}

#[cfg(test)]
mod tests {
    use super::{char_wrap_lines, visual_to_source, wrapped_cursor_position};

    #[test]
    fn visual_to_source_within_wrapped_line() {
        let lines = vec!["abcdefghij"];
        // visual row 1 col 2 at width 4 → source col 6 (chars 'efgh' on row 1, col 2 = 'g')
        assert_eq!(visual_to_source(&lines, 1, 2, 4), (0, 6));
    }

    #[test]
    fn visual_to_source_lands_on_next_source_line() {
        let lines = vec!["abcdefghij", "xyz"];
        // first line takes 3 visual rows at width 4 (10 chars: "abcd","efgh","ij")
        // visual row 3 col 1 → source line 1 col 1
        assert_eq!(visual_to_source(&lines, 3, 1, 4), (1, 1));
    }

    #[test]
    fn visual_to_source_clamps_past_end_of_short_visual_row() {
        let lines = vec!["abcdefghij"];
        // last visual row is "ij" (2 chars). vcol 5 at width 4, target row 2 → clamp to col 10 (end of source)
        assert_eq!(visual_to_source(&lines, 2, 5, 4), (0, 10));
    }

    #[test]
    fn visual_to_source_round_trips_with_wrapped_cursor_position() {
        let lines = vec!["abcdefghijklmnop", "second"];
        let width = 5;
        for src_row in 0..2 {
            let line_len = lines[src_row].chars().count();
            for src_col in 0..=line_len {
                let (vrow, vcol) =
                    wrapped_cursor_position(&lines, src_row, src_col, width).unwrap();
                let (back_row, back_col) = visual_to_source(&lines, vrow, vcol, width);
                assert_eq!(
                    (back_row, back_col),
                    (src_row, src_col),
                    "round trip failed for ({src_row}, {src_col})"
                );
            }
        }
    }

    #[test]
    fn char_wrap_splits_long_line_into_width_chunks() {
        let lines = vec!["abcdefghij"];
        let wrapped = char_wrap_lines(&lines, 4);
        let strings: Vec<String> = wrapped
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(strings, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn char_wrap_preserves_empty_lines() {
        let lines = vec!["", "after"];
        let wrapped = char_wrap_lines(&lines, 10);
        assert_eq!(wrapped.len(), 2);
    }

    #[test]
    fn cursor_and_wrap_agree_on_visual_position() {
        let lines = vec!["abcdefghij"];
        let wrapped = char_wrap_lines(&lines, 4);
        let (vrow, vcol) = wrapped_cursor_position(&lines, 0, 5, 4).unwrap();
        // Cursor at source col 5 should land on row 1 col 1 of wrapped output;
        // wrapped row 1 is "efgh", col 1 is 'f' — matches what user sees.
        assert_eq!((vrow, vcol), (1, 1));
        assert_eq!(wrapped[vrow as usize].spans[0].content, "efgh");
    }

    #[test]
    fn cursor_on_short_line_unwrapped() {
        let lines = vec!["hello"];
        assert_eq!(wrapped_cursor_position(&lines, 0, 3, 80), Some((0, 3)));
    }

    #[test]
    fn cursor_at_wrap_boundary_starts_next_visual_row() {
        let lines = vec!["abcdefghij"];
        // width 5: cursor at col 5 wraps to (1, 0)
        assert_eq!(wrapped_cursor_position(&lines, 0, 5, 5), Some((1, 0)));
    }

    #[test]
    fn cursor_on_second_source_line_after_wrapped_first() {
        let lines = vec!["abcdefghij", "xyz"];
        // first line wraps to 2 rows at width 5; cursor at line 1 col 1 → visual (2, 1)
        assert_eq!(wrapped_cursor_position(&lines, 1, 1, 5), Some((2, 1)));
    }

    #[test]
    fn empty_line_takes_one_visual_row() {
        let lines = vec!["", "after"];
        assert_eq!(wrapped_cursor_position(&lines, 1, 2, 10), Some((1, 2)));
    }

    #[test]
    fn cursor_clamps_to_line_length() {
        let lines = vec!["abc"];
        assert_eq!(wrapped_cursor_position(&lines, 0, 99, 10), Some((0, 3)));
    }

    #[test]
    fn zero_width_returns_none() {
        let lines = vec!["abc"];
        assert!(wrapped_cursor_position(&lines, 0, 1, 0).is_none());
    }
}
