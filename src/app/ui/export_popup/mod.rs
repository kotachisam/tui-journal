use std::{env, path::PathBuf};

use backend::{DataProvider, Entry};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::UserDirs;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tui_textarea::{CursorMove, TextArea};

use crate::app::{
    App,
    keymap::Input,
    ui::{
        file_dialog::{SaveDialogRequest, save_file_dialog},
        inline_completer::{self, InlineCompleterState},
        path_completer::{
            PathCandidate, PathCompletionProvider, list_directory, parse_path_context,
        },
    },
};

use super::{PopupReturn, Styles, ui_functions::centered_rect_exact_height};

type ExportPopupInputReturn = PopupReturn<(PathBuf, Option<u32>)>;

const FOOTER_MULTI: &str = "Enter: confirm | Esc or <Ctrl-c>: Cancel";
const FOOTER_MULTI_COMPLETING: &str =
    "↑/↓: navigate | Tab/Enter: complete | Esc: dismiss overlay";
const FOOTER_SINGLE: &str =
    "↑/↓/Tab: cycle ext | Ctrl-O: file picker | Enter: confirm | Esc: Cancel";
const FOOTER_SINGLE_COMPLETING: &str =
    "↑/↓: navigate | Tab/Enter: complete | Esc: dismiss overlay";
const FOOTER_MARGINE: u16 = 8;
const DEFAULT_FILE_NAME: &str = "tjournal_export.json";
const CYCLE_EXTENSIONS: &[&str] = &["md", "txt"];
const PATH_SEPARATORS: &[char] = &['/', '\\'];
const MAX_PATH_CANDIDATES: usize = 50;

pub struct ExportPopup<'a> {
    path_txt: TextArea<'a>,
    path_err_msg: String,
    entry_id: Option<u32>,
    paragraph_text: String,
    completion: Option<InlineCompleterState<PathCandidate>>,
}

impl ExportPopup<'_> {
    pub fn create_entry_content<D: DataProvider>(
        entry: &Entry,
        app: &App<D>,
    ) -> anyhow::Result<Self> {
        let mut default_path = if let Some(path) = &app.settings.export.default_path {
            path.clone()
        } else {
            env::current_dir()?
        };

        // Add filename if it's not already defined
        if default_path.extension().is_none() {
            default_path.push(format!("{}.md", entry.title.as_str()));
        }

        let mut path_txt = TextArea::new(vec![default_path.to_string_lossy().to_string()]);
        path_txt.move_cursor(CursorMove::End);

        if let Some(file_name) = default_path.file_name().and_then(|s| s.to_str())
            && let Some(("", _)) = split_cycleable_ext(file_name)
        {
            for _ in 0..file_name.chars().count() {
                path_txt.move_cursor(CursorMove::Back);
            }
        }

        let paragraph_text = format!("Journal: {}", entry.title.to_owned());

        let mut export_popup = ExportPopup {
            path_txt,
            path_err_msg: String::default(),
            entry_id: Some(entry.id),
            paragraph_text,
            completion: None,
        };

        export_popup.validate_path();
        export_popup.refresh_completion();

        Ok(export_popup)
    }

    pub fn create_multi_select<D: DataProvider>(app: &App<D>) -> anyhow::Result<Self> {
        let mut default_path = if let Some(path) = &app.settings.export.default_path {
            path.clone()
        } else {
            env::current_dir()?
        };

        // Add filename if it's not already defined
        if default_path.extension().is_none() {
            default_path.push(DEFAULT_FILE_NAME);
        }

        let mut path_txt = TextArea::new(vec![default_path.to_string_lossy().to_string()]);
        path_txt.move_cursor(CursorMove::End);

        let paragraph_text = format!(
            "Export the selected {} journals",
            app.selected_entries.len()
        );

        let mut export_popup = ExportPopup {
            path_txt,
            path_err_msg: String::default(),
            entry_id: None,
            paragraph_text,
            completion: None,
        };

        export_popup.validate_path();
        export_popup.refresh_completion();

        Ok(export_popup)
    }

    fn current_path_string(&self) -> String {
        self.path_txt
            .lines()
            .first()
            .cloned()
            .unwrap_or_default()
    }

    fn cursor_col(&self) -> usize {
        self.path_txt.cursor().1
    }

    fn replace_path(&mut self, new_path: String) {
        self.path_txt = TextArea::new(vec![new_path]);
        self.path_txt.move_cursor(CursorMove::End);
        self.validate_path();
        self.refresh_completion();
    }

    fn replace_path_at_cursor(&mut self, new_path: String, cursor_char_col: usize) {
        self.path_txt = TextArea::new(vec![new_path]);
        self.path_txt.move_cursor(CursorMove::Head);
        for _ in 0..cursor_char_col {
            self.path_txt.move_cursor(CursorMove::Forward);
        }
        self.validate_path();
        self.refresh_completion();
    }

    fn is_completing(&self) -> bool {
        self.completion
            .as_ref()
            .is_some_and(|c| !c.candidates.is_empty())
    }

    fn refresh_completion(&mut self) {
        let path_text = self.current_path_string();
        if path_text.is_empty() {
            self.completion = None;
            return;
        }

        let cursor_byte = char_to_byte_offset(&path_text, self.cursor_col());
        let prefix = &path_text[..cursor_byte];

        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = UserDirs::new().map(|d| d.home_dir().to_path_buf());

        let ctx = match parse_path_context(prefix, home.as_deref(), &cwd) {
            Some(ctx) => ctx,
            None => {
                self.completion = None;
                return;
            }
        };

        let mut candidates = list_directory(&ctx.parent_dir, &ctx.query, MAX_PATH_CANDIDATES);
        candidates.retain(|c| c.name != ctx.query || c.is_dir);

        if candidates.is_empty() {
            self.completion = None;
            return;
        }

        let preserved_idx = self.completion.as_ref().map(|c| c.selected_idx).unwrap_or(0);
        let mut state = InlineCompleterState::new(0, 0);
        state.candidates = candidates;
        state.selected_idx = preserved_idx.min(state.candidates.len().saturating_sub(1));
        state.query = ctx.query;
        self.completion = Some(state);
    }

    fn apply_completion(&mut self) {
        let candidate = match self.completion.as_ref().and_then(|c| c.selected().cloned()) {
            Some(c) => c,
            None => return,
        };
        let path_text = self.current_path_string();
        let cursor_byte = char_to_byte_offset(&path_text, self.cursor_col());
        let prefix = &path_text[..cursor_byte];
        let suffix = &path_text[cursor_byte..];

        let zone_start = prefix.rfind(PATH_SEPARATORS).map(|i| i + 1).unwrap_or(0);

        let insert_text = if let Some(("", _)) = split_cycleable_ext(suffix) {
            match split_cycleable_ext(&candidate.name) {
                Some((stem, _)) => stem.to_string(),
                None => candidate.name.clone(),
            }
        } else {
            candidate.name.clone()
        };

        let trailing_slash = if candidate.is_dir { "/" } else { "" };

        let mut new_path = String::new();
        new_path.push_str(&path_text[..zone_start]);
        new_path.push_str(&insert_text);
        new_path.push_str(trailing_slash);
        new_path.push_str(suffix);

        let cursor_byte_pos = zone_start + insert_text.len() + trailing_slash.len();
        let cursor_char_pos = new_path[..cursor_byte_pos].chars().count();

        self.replace_path_at_cursor(new_path, cursor_char_pos);
    }

    fn delete_segment_backward(&mut self) {
        let content = self.current_path_string();
        if content.is_empty() {
            return;
        }
        let trimmed = content.trim_end_matches(PATH_SEPARATORS);
        let new_content = match trimmed.rfind(PATH_SEPARATORS) {
            Some(0) => "/".to_string(),
            Some(idx) => trimmed[..idx].to_string(),
            None => String::new(),
        };
        self.replace_path(new_content);
    }

    fn clear_to_home(&mut self) {
        self.replace_path("~/".to_string());
    }

    fn open_save_dialog(&mut self) {
        let current = self.current_path_string();
        let expanded = expand_tilde(&current);
        let default_pb = if expanded.is_empty() {
            env::current_dir().unwrap_or_default()
        } else {
            PathBuf::from(&expanded)
        };

        let (default_dir, default_name) = match (default_pb.parent(), default_pb.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                (parent.to_path_buf(), name.to_string_lossy().into_owned())
            }
            _ => {
                let cwd = env::current_dir().unwrap_or_default();
                let name = default_pb
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| String::from("export.md"));
                (cwd, name)
            }
        };

        let prompt = if self.is_multi_select_mode() {
            "Save journals as"
        } else {
            "Save journal as"
        };

        let req = SaveDialogRequest {
            prompt,
            default_dir: &default_dir,
            default_name: &default_name,
        };

        match save_file_dialog(&req) {
            Ok(Some(path)) => {
                self.replace_path(path.to_string_lossy().into_owned());
            }
            Ok(None) => {}
            Err(err) => {
                self.path_err_msg = format!("File picker unavailable: {err}");
            }
        }
    }

    fn cycle_extension(&mut self) {
        let line = self.current_path_string();
        if let Some(new_path) = cycle_path_extension(&line) {
            self.replace_path(new_path);
        }
    }

    fn validate_path(&mut self) {
        let path = self
            .path_txt
            .lines()
            .first()
            .expect("Path Textbox should always have one line");

        if path.is_empty() {
            self.path_err_msg = "Path can't be empty".into();
        } else {
            self.path_err_msg.clear();
        }
    }

    fn is_input_valid(&self) -> bool {
        self.path_err_msg.is_empty()
    }

    fn is_multi_select_mode(&self) -> bool {
        self.entry_id.is_none()
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let mut area = centered_rect_exact_height(70, 11, area);

        let completing = self.is_completing();
        let footer_text = match (self.is_multi_select_mode(), completing) {
            (true, true) => FOOTER_MULTI_COMPLETING,
            (true, false) => FOOTER_MULTI,
            (false, true) => FOOTER_SINGLE_COMPLETING,
            (false, false) => FOOTER_SINGLE,
        };

        if area.width < footer_text.chars().count() as u16 + FOOTER_MARGINE {
            area.height += 1;
        }

        let title = if self.is_multi_select_mode() {
            "Export journals"
        } else {
            "Export journal content"
        };

        let block = Block::default().borders(Borders::ALL).title(title);

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(4)
            .vertical_margin(2)
            .constraints(
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(area);

        let journal_paragraph =
            Paragraph::new(self.paragraph_text.as_str()).wrap(Wrap { trim: false });
        frame.render_widget(journal_paragraph, chunks[0]);

        if self.path_err_msg.is_empty() {
            let block = Style::from(styles.general.input_block_active);
            let cursor = Style::from(styles.general.input_cursor_active);
            self.path_txt.set_style(block);
            self.path_txt.set_cursor_style(cursor);
            self.path_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title("Path"),
            );
        } else {
            let block = Style::from(styles.general.input_block_invalid);
            let cursor = Style::from(styles.general.input_cursor_invalid);
            self.path_txt.set_style(block);
            self.path_txt.set_cursor_style(cursor);
            self.path_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title(format!("Path : {}", self.path_err_msg)),
            );
        }

        self.path_txt.set_cursor_line_style(Style::default());

        frame.render_widget(&self.path_txt, chunks[1]);

        let footer = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

        frame.render_widget(footer, chunks[3]);

        if let Some(state) = self.completion.as_ref() {
            inline_completer::render_overlay(frame, chunks[1], state, &PathCompletionProvider);
        }
    }

    pub fn handle_input(&mut self, input: &Input) -> ExportPopupInputReturn {
        let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
        let completing = self.is_completing();

        match input.key_code {
            KeyCode::Char('c') if has_ctrl => ExportPopupInputReturn::Cancel,
            KeyCode::Esc => {
                if completing {
                    self.completion = None;
                    ExportPopupInputReturn::KeepPopup
                } else {
                    ExportPopupInputReturn::Cancel
                }
            }
            KeyCode::Enter => {
                if completing {
                    self.apply_completion();
                    ExportPopupInputReturn::KeepPopup
                } else {
                    self.handle_confirm()
                }
            }
            KeyCode::Tab | KeyCode::BackTab if completing => {
                self.apply_completion();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Up if completing => {
                if let Some(c) = self.completion.as_mut() {
                    c.move_up();
                }
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Down if completing => {
                if let Some(c) = self.completion.as_mut() {
                    c.move_down();
                }
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Backspace if has_ctrl => {
                self.delete_segment_backward();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Char('u') if has_ctrl => {
                self.clear_to_home();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Char('o') if has_ctrl => {
                self.open_save_dialog();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab
                if !self.is_multi_select_mode() =>
            {
                self.cycle_extension();
                ExportPopupInputReturn::KeepPopup
            }
            _ => {
                if self.path_txt.input(KeyEvent::from(input)) {
                    self.validate_path();
                    self.path_txt.scroll((0, -1024));
                    self.refresh_completion();
                }
                ExportPopupInputReturn::KeepPopup
            }
        }
    }

    fn handle_confirm(&mut self) -> ExportPopupInputReturn {
        self.validate_path();
        if !self.is_input_valid() {
            return ExportPopupInputReturn::KeepPopup;
        }

        let raw = self
            .path_txt
            .lines()
            .first()
            .expect("Path Textbox should always have one line")
            .clone();
        let path = PathBuf::from(expand_tilde(&raw));

        ExportPopupInputReturn::Apply((path, self.entry_id))
    }
}

fn split_cycleable_ext(file_name: &str) -> Option<(&str, usize)> {
    for (idx, ext) in CYCLE_EXTENSIONS.iter().enumerate() {
        let suffix_len = ext.len() + 1;
        if file_name.len() < suffix_len {
            continue;
        }
        let suffix_start = file_name.len() - suffix_len;
        if file_name.as_bytes()[suffix_start] == b'.'
            && file_name[suffix_start + 1..].eq_ignore_ascii_case(ext)
        {
            return Some((&file_name[..suffix_start], idx));
        }
    }
    None
}

fn cycle_path_extension(line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }

    let pb = PathBuf::from(line);
    let file_name = pb.file_name().and_then(|s| s.to_str())?;

    let (stem_raw, idx) = split_cycleable_ext(file_name)?;
    let next_ext = CYCLE_EXTENSIONS[(idx + 1) % CYCLE_EXTENSIONS.len()];

    let stem = match split_cycleable_ext(stem_raw) {
        Some((cleaner, _)) => cleaner,
        None => stem_raw,
    };

    let new_filename = if stem.is_empty() {
        format!(".{next_ext}")
    } else {
        format!("{stem}.{next_ext}")
    };

    let parent = pb
        .parent()
        .map(|p| p.to_path_buf())
        .filter(|p| !p.as_os_str().is_empty());
    Some(match parent {
        Some(p) => p.join(&new_filename).to_string_lossy().into_owned(),
        None => new_filename,
    })
}

fn char_to_byte_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        return home_dir_string().unwrap_or_else(|| input.to_string());
    }
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = home_dir_string()
    {
        let mut buf = PathBuf::from(home);
        buf.push(rest);
        return buf.to_string_lossy().into_owned();
    }
    input.to_string()
}

fn home_dir_string() -> Option<String> {
    UserDirs::new().map(|d| d.home_dir().to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_md_to_txt_relative() {
        assert_eq!(
            cycle_path_extension("Title.md").as_deref(),
            Some("Title.txt")
        );
    }

    #[test]
    fn cycle_txt_to_md_relative() {
        assert_eq!(
            cycle_path_extension("Title.txt").as_deref(),
            Some("Title.md")
        );
    }

    #[test]
    fn cycle_md_to_txt_absolute() {
        assert_eq!(
            cycle_path_extension("/Users/sam/Title.md").as_deref(),
            Some("/Users/sam/Title.txt")
        );
    }

    #[test]
    fn cycle_handles_filename_with_commas_and_spaces() {
        assert_eq!(
            cycle_path_extension(
                "/Users/sam_r/Developer/oss/tjournal/Wisdom, Judgement and Thinking.md"
            )
            .as_deref(),
            Some("/Users/sam_r/Developer/oss/tjournal/Wisdom, Judgement and Thinking.txt")
        );
    }

    #[test]
    fn cycle_no_extension_is_noop() {
        assert!(cycle_path_extension("/some/dir/Title").is_none());
    }

    #[test]
    fn cycle_unknown_extension_is_noop() {
        assert!(cycle_path_extension("Title.bak").is_none());
    }

    #[test]
    fn cycle_strips_double_md_to_clean_txt() {
        assert_eq!(
            cycle_path_extension("Title.md.md").as_deref(),
            Some("Title.txt")
        );
    }

    #[test]
    fn cycle_strips_double_txt_then_cycles_md_to_txt() {
        // "Title.txt.md" → strip ".txt" suffix from stem, cycle md→txt → "Title.txt"
        assert_eq!(
            cycle_path_extension("Title.txt.md").as_deref(),
            Some("Title.txt")
        );
    }

    #[test]
    fn cycle_keeps_legitimate_dotted_filenames() {
        // "v1.0.md" stem is "v1.0", which doesn't end in a cycle ext — leave alone
        assert_eq!(
            cycle_path_extension("v1.0.md").as_deref(),
            Some("v1.0.txt")
        );
    }

    #[test]
    fn cycle_empty_returns_none() {
        assert!(cycle_path_extension("").is_none());
    }

    #[test]
    fn cycle_dot_md_alone_swaps_to_dot_txt() {
        assert_eq!(cycle_path_extension(".md").as_deref(), Some(".txt"));
    }

    #[test]
    fn cycle_dot_txt_alone_swaps_to_dot_md() {
        assert_eq!(cycle_path_extension(".txt").as_deref(), Some(".md"));
    }

    #[test]
    fn cycle_dot_md_in_directory() {
        assert_eq!(
            cycle_path_extension("/Users/sam/.md").as_deref(),
            Some("/Users/sam/.txt")
        );
    }

    #[test]
    fn char_to_byte_offset_ascii() {
        assert_eq!(char_to_byte_offset("hello", 3), 3);
    }

    #[test]
    fn char_to_byte_offset_past_end_clamps_to_len() {
        assert_eq!(char_to_byte_offset("hi", 10), 2);
    }

    #[test]
    fn char_to_byte_offset_handles_multibyte() {
        // "résumé" — é is 2 bytes; 3 chars in is byte 4
        assert_eq!(char_to_byte_offset("résumé", 3), 4);
    }
}
