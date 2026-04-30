use ratatui::{
    style::{Color, Style},
    widgets::{Block, Borders},
};
use tui_textarea::TextArea;

use crate::app::ui::Styles;

pub struct FieldStyles {
    active_block: Style,
    invalid_block: Style,
    reset_block: Style,
    active_cursor: Style,
    invalid_cursor: Style,
    deactivate_cursor: Style,
}

impl FieldStyles {
    pub fn new(styles: &Styles) -> Self {
        let g = &styles.general;
        Self {
            active_block: Style::from(g.input_block_active),
            invalid_block: Style::from(g.input_block_invalid),
            reset_block: Style::reset(),
            active_cursor: Style::from(g.input_cursor_active),
            invalid_cursor: Style::from(g.input_cursor_invalid),
            deactivate_cursor: Style::default().bg(Color::Reset),
        }
    }
}

pub fn render_field(
    txt: &mut TextArea,
    is_active: bool,
    err: &str,
    normal_title: &str,
    active_title: Option<&str>,
    styles: &FieldStyles,
) {
    if err.is_empty() {
        let (block_style, cursor_style) = if is_active {
            (styles.active_block, styles.active_cursor)
        } else {
            (styles.reset_block, styles.deactivate_cursor)
        };
        let title = if is_active {
            active_title.unwrap_or(normal_title)
        } else {
            normal_title
        };
        txt.set_style(block_style);
        txt.set_cursor_style(cursor_style);
        txt.set_block(
            Block::default()
                .borders(Borders::ALL)
                .style(block_style)
                .title(title.to_string()),
        );
    } else {
        let cursor_style = if is_active {
            styles.invalid_cursor
        } else {
            styles.deactivate_cursor
        };
        txt.set_style(styles.invalid_block);
        txt.set_cursor_style(cursor_style);
        txt.set_block(
            Block::default()
                .borders(Borders::ALL)
                .style(styles.invalid_block)
                .title(format!("{normal_title} : {err}")),
        );
    }
}
