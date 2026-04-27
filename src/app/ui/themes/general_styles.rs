use ratatui::style::Modifier;
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneralStyles {
    // input text
    #[serde(default = "input_block_active")]
    pub input_block_active: Style,
    #[serde(default = "input_block_invalid")]
    pub input_block_invalid: Style,
    #[serde(default = "input_cursor_active")]
    // Previous versions had a `corsur` typo, so support reading the old name. The behavior of
    // `alias` is that providing old and new names will error.
    #[serde(alias = "input_corsur_active")]
    pub input_cursor_active: Style,
    #[serde(default = "input_cursor_invalid")]
    // See above `alias` comment.
    #[serde(alias = "input_corsur_invalid")]
    pub input_cursor_invalid: Style,

    // General list items
    #[serde(default = "list_item_selected")]
    pub list_item_selected: Style,
    #[serde(default = "list_highlight_active")]
    pub list_highlight_active: Style,
    #[serde(default = "list_highlight_inactive")]
    pub list_highlight_inactive: Style,

    #[serde(default = "search_highlight")]
    pub search_highlight: Style,

    // TODO(upstream): if this lands in Ammar's repo, add a `toast` entry to the help/?
    // tooltip and document the [general.toast] section in THEMES.md / README themes table.
    #[serde(default = "toast")]
    pub toast: Style,
}

impl Default for GeneralStyles {
    fn default() -> Self {
        Self {
            input_block_active: input_block_active(),
            input_block_invalid: input_block_invalid(),
            input_cursor_active: input_cursor_active(),
            input_cursor_invalid: input_cursor_invalid(),
            list_item_selected: list_item_selected(),
            list_highlight_active: list_highlight_active(),
            list_highlight_inactive: list_highlight_inactive(),
            search_highlight: search_highlight(),
            toast: toast(),
        }
    }
}

#[inline]
fn input_block_invalid() -> Style {
    Style {
        fg: Some(INVALID_CONTROL_COLOR),
        ..Default::default()
    }
}

#[inline]
fn input_block_active() -> Style {
    Style {
        fg: Some(ACTIVE_INPUT_BORDER_COLOR),
        ..Default::default()
    }
}

#[inline]
fn input_cursor_active() -> Style {
    Style {
        bg: Some(ACTIVE_INPUT_BORDER_COLOR),
        fg: Some(Color::Black),
        ..Default::default()
    }
}

#[inline]
fn input_cursor_invalid() -> Style {
    Style {
        bg: Some(INVALID_CONTROL_COLOR),
        fg: Some(Color::Black),
        ..Default::default()
    }
}

#[inline]
fn list_item_selected() -> Style {
    Style {
        fg: Some(Color::LightYellow),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}
fn list_highlight_active() -> Style {
    Style {
        fg: Some(Color::Black),
        bg: Some(Color::LightGreen),
        ..Default::default()
    }
}

fn list_highlight_inactive() -> Style {
    Style {
        fg: Some(Color::Black),
        bg: Some(Color::LightBlue),
        ..Default::default()
    }
}

fn search_highlight() -> Style {
    Style {
        fg: Some(Color::Black),
        bg: Some(Color::Yellow),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

fn toast() -> Style {
    Style {
        fg: Some(Color::LightCyan),
        modifiers: Modifier::ITALIC | Modifier::BOLD,
        ..Default::default()
    }
}
