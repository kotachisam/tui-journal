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
        let rows = word_wrap_lines(&lines, inner.width);
        let cursor_visual = wrapped_cursor_position(&rows, cursor_row, cursor_col);

        if let Some((vrow, _)) = cursor_visual {
            if vrow < self.preview_scroll {
                self.preview_scroll = vrow;
            } else if inner.height > 0 && vrow >= self.preview_scroll + inner.height {
                self.preview_scroll = vrow + 1 - inner.height;
            }
        }

        let visual_lines: Vec<Line<'_>> =
            rows.iter().map(|r| Line::raw(r.content.clone())).collect();
        let paragraph = Paragraph::new(visual_lines).scroll((self.preview_scroll, 0));
        frame.render_widget(paragraph, inner);

        if let Some((vrow, vcol)) = cursor_visual {
            let scrolled_row = vrow.saturating_sub(self.preview_scroll);
            if vrow >= self.preview_scroll && scrolled_row < inner.height && vcol < inner.width {
                frame.set_cursor_position((inner.x + vcol, inner.y + scrolled_row));
            }

            if let Some(mention) = self.mention.as_ref()
                && vrow >= self.preview_scroll
                && scrolled_row < inner.height
            {
                let anchor = Rect {
                    x: inner.x + vcol.min(inner.width.saturating_sub(1)),
                    y: inner.y + scrolled_row,
                    width: 1,
                    height: 1,
                };
                super::mention::render_overlay(frame, anchor, mention);
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

pub(super) struct WrapRow {
    pub source_line: usize,
    pub source_start: usize,
    pub content: String,
}

pub(super) fn word_wrap_lines(lines: &[&str], width: u16) -> Vec<WrapRow> {
    if lines.is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return lines
            .iter()
            .enumerate()
            .map(|(i, l)| WrapRow {
                source_line: i,
                source_start: 0,
                content: (*l).to_string(),
            })
            .collect();
    }
    let width_us = width as usize;
    let mut rows: Vec<WrapRow> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            rows.push(WrapRow {
                source_line: line_idx,
                source_start: 0,
                content: String::new(),
            });
            continue;
        }

        let mut row_start: usize = 0;
        let mut last_space: Option<usize> = None;
        let mut i: usize = 0;

        while i < chars.len() {
            let chars_on_row = i - row_start;
            let c = chars[i];

            if chars_on_row >= width_us && c != ' ' {
                let (push_end, next_start) = match last_space {
                    Some(sp) if sp >= row_start => (sp, sp + 1),
                    _ => (i, i),
                };
                let content: String = chars[row_start..push_end].iter().collect();
                rows.push(WrapRow {
                    source_line: line_idx,
                    source_start: row_start,
                    content,
                });
                row_start = next_start;
                last_space = None;
                continue;
            }

            if c == ' ' {
                last_space = Some(i);
            }
            i += 1;
        }

        let content: String = chars[row_start..].iter().collect();
        rows.push(WrapRow {
            source_line: line_idx,
            source_start: row_start,
            content,
        });
    }

    rows
}

pub(super) fn wrapped_cursor_position(
    rows: &[WrapRow],
    cursor_line: usize,
    cursor_col: usize,
) -> Option<(u16, u16)> {
    if rows.is_empty() {
        return None;
    }
    let mut best: Option<usize> = None;
    for (idx, row) in rows.iter().enumerate() {
        if row.source_line < cursor_line {
            continue;
        }
        if row.source_line > cursor_line {
            break;
        }
        if row.source_start <= cursor_col {
            best = Some(idx);
        } else {
            break;
        }
    }
    let idx = best?;
    let row = &rows[idx];
    let visual_col = cursor_col - row.source_start;
    let row_chars = row.content.chars().count();
    let clamped = visual_col.min(row_chars);
    Some((
        u16::try_from(idx).unwrap_or(u16::MAX),
        u16::try_from(clamped).unwrap_or(u16::MAX),
    ))
}

pub(super) fn visual_to_source(rows: &[WrapRow], target_vrow: u16, vcol: u16) -> (usize, usize) {
    if rows.is_empty() {
        return (0, 0);
    }
    let target_idx = (target_vrow as usize).min(rows.len() - 1);
    let row = &rows[target_idx];
    let row_chars = row.content.chars().count();
    let clamped = (vcol as usize).min(row_chars);
    (row.source_line, row.source_start + clamped)
}

#[cfg(test)]
mod tests {
    use super::{visual_to_source, word_wrap_lines, wrapped_cursor_position};

    fn contents(rows: &[super::WrapRow]) -> Vec<&str> {
        rows.iter().map(|r| r.content.as_str()).collect()
    }

    #[test]
    fn word_wrap_keeps_short_line_intact() {
        let rows = word_wrap_lines(&["hello"], 80);
        assert_eq!(contents(&rows), vec!["hello"]);
    }

    #[test]
    fn word_wrap_breaks_at_space_when_word_would_overflow() {
        let rows = word_wrap_lines(&["hello world"], 5);
        assert_eq!(contents(&rows), vec!["hello", "world"]);
    }

    #[test]
    fn word_wrap_falls_back_to_char_break_for_long_word() {
        let rows = word_wrap_lines(&["abcdefghij"], 4);
        assert_eq!(contents(&rows), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn word_wrap_handles_long_word_after_short_word() {
        // "hi abcdefghij" width 5: "hi" fits, then long word char-breaks
        let rows = word_wrap_lines(&["hi abcdefghij"], 5);
        assert_eq!(contents(&rows), vec!["hi", "abcde", "fghij"]);
    }

    #[test]
    fn word_wrap_preserves_empty_lines() {
        let rows = word_wrap_lines(&["", "after"], 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].content, "");
        assert_eq!(rows[1].content, "after");
    }

    #[test]
    fn word_wrap_multi_paragraph() {
        let rows = word_wrap_lines(&["one two three", "second line"], 7);
        let texts = contents(&rows);
        assert_eq!(texts, vec!["one two", "three", "second", "line"]);
    }

    #[test]
    fn cursor_on_short_line_unwrapped() {
        let rows = word_wrap_lines(&["hello"], 80);
        assert_eq!(wrapped_cursor_position(&rows, 0, 3), Some((0, 3)));
    }

    #[test]
    fn cursor_at_word_boundary_after_wrap() {
        let rows = word_wrap_lines(&["hello world"], 5);
        // Cursor at source col 6 ('w'): row 1 col 0
        assert_eq!(wrapped_cursor_position(&rows, 0, 6), Some((1, 0)));
    }

    #[test]
    fn cursor_on_consumed_space_lands_at_end_of_previous_row() {
        let rows = word_wrap_lines(&["hello world"], 5);
        // Source col 5 (the space): row 0, col 5 (end of "hello")
        assert_eq!(wrapped_cursor_position(&rows, 0, 5), Some((0, 5)));
    }

    #[test]
    fn cursor_on_second_source_line() {
        let rows = word_wrap_lines(&["hello world", "next"], 5);
        // After "hello world" wraps to 2 rows, cursor at line 1 col 2 → row 2 col 2
        assert_eq!(wrapped_cursor_position(&rows, 1, 2), Some((2, 2)));
    }

    #[test]
    fn cursor_clamps_past_end_of_line() {
        let rows = word_wrap_lines(&["abc"], 10);
        assert_eq!(wrapped_cursor_position(&rows, 0, 99), Some((0, 3)));
    }

    #[test]
    fn empty_line_takes_one_visual_row() {
        let rows = word_wrap_lines(&["", "after"], 10);
        // Empty line is row 0, "after" is row 1. Cursor at line 1 col 2 → (1, 2).
        assert_eq!(wrapped_cursor_position(&rows, 1, 2), Some((1, 2)));
    }

    #[test]
    fn visual_to_source_within_wrapped_line() {
        let rows = word_wrap_lines(&["hello world"], 5);
        // Visual row 1 col 2 ('r' in "world") → source line 0, col 8
        assert_eq!(visual_to_source(&rows, 1, 2), (0, 8));
    }

    #[test]
    fn visual_to_source_lands_on_next_source_line() {
        let rows = word_wrap_lines(&["hello world", "next"], 5);
        // Source line 0 takes rows 0-1, source line 1 is row 2
        assert_eq!(visual_to_source(&rows, 2, 2), (1, 2));
    }

    #[test]
    fn visual_to_source_clamps_to_row_content_length() {
        let rows = word_wrap_lines(&["hello world"], 5);
        // Row 1 is "world" (5 chars). vcol 99 clamps to col 11 (source end).
        assert_eq!(visual_to_source(&rows, 1, 99), (0, 11));
    }

    #[test]
    fn cursor_round_trip_word_wrap() {
        let lines = vec!["the quick brown fox jumps over the lazy dog", "second line"];
        let width = 10;
        let rows = word_wrap_lines(&lines, width);
        for (src_row, line) in lines.iter().enumerate() {
            let line_len = line.chars().count();
            for src_col in 0..=line_len {
                let (vrow, vcol) =
                    wrapped_cursor_position(&rows, src_row, src_col).expect("position");
                let (back_row, back_col) = visual_to_source(&rows, vrow, vcol);
                assert_eq!(
                    (back_row, back_col),
                    (src_row, src_col),
                    "round trip failed for ({src_row}, {src_col})"
                );
            }
        }
    }

    #[test]
    fn empty_lines_returns_empty_rows() {
        let rows = word_wrap_lines(&[], 80);
        assert!(rows.is_empty());
    }

    #[test]
    fn zero_width_keeps_lines_as_single_rows() {
        let rows = word_wrap_lines(&["hello", "world"], 0);
        assert_eq!(rows.len(), 2);
    }
}
