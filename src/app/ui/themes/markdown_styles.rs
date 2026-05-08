use markdown_tui::widget::MarkdownStyles as RenderMarkdownStyles;
use ratatui::style::{Color, Modifier};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkdownStyles {
    #[serde(default = "heading_1")]
    pub heading_1: Style,
    #[serde(default = "heading_2")]
    pub heading_2: Style,
    #[serde(default = "heading_3")]
    pub heading_3: Style,
    #[serde(default = "heading_4")]
    pub heading_4: Style,
    #[serde(default = "heading_5")]
    pub heading_5: Style,
    #[serde(default = "heading_6")]
    pub heading_6: Style,
    #[serde(default = "bold")]
    pub bold: Style,
    #[serde(default = "italic")]
    pub italic: Style,
    #[serde(default = "bold_italic")]
    pub bold_italic: Style,
    #[serde(default = "inline_code")]
    pub inline_code: Style,
    #[serde(default = "code_block")]
    pub code_block: Style,
    #[serde(default = "block_quote")]
    pub block_quote: Style,
    #[serde(default = "rule")]
    pub rule: Style,
    #[serde(default = "text")]
    pub text: Style,
}

impl Default for MarkdownStyles {
    fn default() -> Self {
        Self {
            heading_1: heading_1(),
            heading_2: heading_2(),
            heading_3: heading_3(),
            heading_4: heading_4(),
            heading_5: heading_5(),
            heading_6: heading_6(),
            bold: bold(),
            italic: italic(),
            bold_italic: bold_italic(),
            inline_code: inline_code(),
            code_block: code_block(),
            block_quote: block_quote(),
            rule: rule(),
            text: text(),
        }
    }
}

impl From<&MarkdownStyles> for RenderMarkdownStyles {
    fn from(styles: &MarkdownStyles) -> Self {
        Self {
            heading: [
                styles.heading_1.into(),
                styles.heading_2.into(),
                styles.heading_3.into(),
                styles.heading_4.into(),
                styles.heading_5.into(),
                styles.heading_6.into(),
            ],
            bold: styles.bold.into(),
            italic: styles.italic.into(),
            bold_italic: styles.bold_italic.into(),
            inline_code: styles.inline_code.into(),
            code_block: styles.code_block.into(),
            block_quote: styles.block_quote.into(),
            rule: styles.rule.into(),
            text: styles.text.into(),
        }
    }
}

const CODE_BG: Color = Color::Rgb(50, 50, 60);

#[inline]
fn heading_1() -> Style {
    Style {
        fg: Some(Color::Yellow),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn heading_2() -> Style {
    Style {
        fg: Some(Color::Green),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn heading_3() -> Style {
    Style {
        fg: Some(Color::Cyan),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn heading_4() -> Style {
    Style {
        fg: Some(Color::Magenta),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn heading_5() -> Style {
    Style {
        fg: Some(Color::Blue),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn heading_6() -> Style {
    Style {
        fg: Some(Color::Red),
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn bold() -> Style {
    Style {
        modifiers: Modifier::BOLD,
        ..Default::default()
    }
}

#[inline]
fn italic() -> Style {
    Style {
        modifiers: Modifier::ITALIC,
        ..Default::default()
    }
}

#[inline]
fn bold_italic() -> Style {
    Style {
        modifiers: Modifier::BOLD | Modifier::ITALIC,
        ..Default::default()
    }
}

#[inline]
fn inline_code() -> Style {
    Style {
        fg: Some(Color::LightYellow),
        bg: Some(CODE_BG),
        ..Default::default()
    }
}

#[inline]
fn code_block() -> Style {
    Style {
        fg: Some(Color::LightYellow),
        bg: Some(CODE_BG),
        ..Default::default()
    }
}

#[inline]
fn block_quote() -> Style {
    Style {
        fg: Some(Color::DarkGray),
        modifiers: Modifier::ITALIC,
        ..Default::default()
    }
}

#[inline]
fn rule() -> Style {
    Style {
        fg: Some(Color::DarkGray),
        ..Default::default()
    }
}

#[inline]
fn text() -> Style {
    Style::default()
}
