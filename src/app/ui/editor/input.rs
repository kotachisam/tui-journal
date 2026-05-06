use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tui_textarea::{CursorMove, Scrolling};

use backend::DataProvider;

use crate::app::{
    App, keymap::Input, runner::HandleInputReturnType, ui::commands::ClipboardOperation,
};

use super::{Editor, EditorMode, Operator};

impl From<&Input> for KeyEvent {
    fn from(value: &Input) -> Self {
        KeyEvent {
            code: value.key_code,
            modifiers: value.modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
}

impl Editor<'_> {
    pub fn handle_input_prioritized<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &App<D>,
    ) -> anyhow::Result<HandleInputReturnType> {
        if self.is_insert_mode() {
            // We must handle clipboard operation separately if sync with system clipboard is
            // activated
            if app.settings.sync_os_clipboard {
                let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
                // Keymaps are taken from `text_area` source code
                let handled = match input.key_code {
                    KeyCode::Char('x') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Cut)?;
                        true
                    }
                    KeyCode::Char('c') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Copy)?;
                        true
                    }
                    KeyCode::Char('y') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Paste)?;
                        true
                    }
                    _ => false,
                };

                if handled {
                    return Ok(HandleInputReturnType::Handled);
                }
            }

            if self.handle_mention_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.handle_mention_commit(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.try_visual_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.try_snap_vertical_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            self.dismiss_mention_on_break_char(input);

            let is_at_typed = matches!(input.key_code, KeyCode::Char('@'))
                && input.modifiers == KeyModifiers::NONE;

            let key_event = KeyEvent::from(input);
            if self.text_area.input(key_event) {
                self.is_dirty = true;
                self.refresh_has_unsaved(app);
            }

            if is_at_typed {
                self.maybe_open_mention(app);
            } else if self.mention.is_some() {
                self.update_mention(app);
            }

            return Ok(HandleInputReturnType::Handled);
        }

        Ok(HandleInputReturnType::NotFound)
    }

    pub fn handle_input<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &App<D>,
    ) -> anyhow::Result<HandleInputReturnType> {
        if self.try_capture_mention_peek(input) {
            return Ok(HandleInputReturnType::Handled);
        }

        if self.try_capture_mention_follow(input) {
            return Ok(HandleInputReturnType::Handled);
        }

        if self.show_preview {
            match input.key_code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.preview_scroll = self.preview_scroll.saturating_add(1);
                    return Ok(HandleInputReturnType::Handled);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1);
                    return Ok(HandleInputReturnType::Handled);
                }
                KeyCode::Char('p') | KeyCode::Esc => {
                    self.toggle_preview();
                    return Ok(HandleInputReturnType::Handled);
                }
                _ => {
                    self.show_preview = false;
                    self.preview_scroll = 0;
                }
            }
        }
        debug_assert!(!self.is_insert_mode());

        if app.get_current_entry().is_none() {
            return Ok(HandleInputReturnType::Handled);
        }

        let sync_os_clipboard = app.settings.sync_os_clipboard;

        if is_default_navigation(input) {
            if !self.try_snap_vertical_navigation(input) {
                let key_event = KeyEvent::from(input);
                self.text_area.input(key_event);
            }
        } else if !self.is_visual_mode()
            || !self.handle_input_visual_only(input, sync_os_clipboard)?
        {
            self.handle_vim_motions(input, sync_os_clipboard)?;
        }

        // Check if the input led the editor to leave the visual mode and make the corresponding UI changes
        if !self.text_area.is_selecting() && self.is_visual_mode() {
            self.set_editor_mode(EditorMode::Normal);
        }

        self.is_dirty = true;
        self.refresh_has_unsaved(app);

        Ok(HandleInputReturnType::Handled)
    }

    /// Handles input specialized for visual mode only like cut and copy
    fn handle_input_visual_only(
        &mut self,
        input: &Input,
        sync_os_clipboard: bool,
    ) -> anyhow::Result<bool> {
        if !input.modifiers.is_empty() {
            return Ok(false);
        }

        match input.key_code {
            KeyCode::Char('d') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Cut)?;
                } else {
                    self.text_area.cut();
                }
                Ok(true)
            }
            KeyCode::Char('y') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Copy)?;
                } else {
                    self.text_area.copy();
                }
                self.set_editor_mode(EditorMode::Normal);
                Ok(true)
            }
            KeyCode::Char('c') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Copy)?;
                } else {
                    self.text_area.cut();
                }
                self.set_editor_mode(EditorMode::Insert);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_vim_motions(&mut self, input: &Input, sync_os_clipboard: bool) -> anyhow::Result<()> {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);

        if self.pending_operator.is_some() {
            return self.handle_pending_operator(input, sync_os_clipboard);
        }

        if input.modifiers.is_empty() && self.mode == EditorMode::Normal {
            match input.key_code {
                KeyCode::Char('d') => {
                    self.pending_operator = Some(Operator::Delete);
                    return Ok(());
                }
                KeyCode::Char('c') => {
                    self.pending_operator = Some(Operator::Change);
                    return Ok(());
                }
                _ => {}
            }
        }

        match (input.key_code, has_control) {
            (KeyCode::Char('h'), false) => {
                self.text_area.move_cursor(CursorMove::Back);
            }
            (KeyCode::Char('j'), false) => {
                self.text_area.move_cursor(CursorMove::Down);
            }
            (KeyCode::Char('k'), false) => {
                self.text_area.move_cursor(CursorMove::Up);
            }
            (KeyCode::Char('l'), false) => {
                self.text_area.move_cursor(CursorMove::Forward);
            }
            (KeyCode::Char('w'), false) | (KeyCode::Char('e'), false) => {
                self.text_area.move_cursor(CursorMove::WordForward);
            }
            (KeyCode::Char('b'), false) => {
                self.text_area.move_cursor(CursorMove::WordBack);
            }
            (KeyCode::Char('^'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
            }
            (KeyCode::Char('$'), false) => {
                self.text_area.move_cursor(CursorMove::End);
            }
            (KeyCode::Char('D'), false) => {
                self.text_area.delete_line_by_end();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
            }
            (KeyCode::Char('C'), false) => {
                self.text_area.delete_line_by_end();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('p'), false) => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Paste)?;
                } else {
                    self.text_area.paste();
                }
            }
            (KeyCode::Char('x'), false) => {
                self.text_area.delete_next_char();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
            }
            (KeyCode::Char('i'), false) => self.mode = EditorMode::Insert,
            (KeyCode::Char('a'), false) => {
                self.text_area.move_cursor(CursorMove::Forward);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('A'), false) => {
                self.text_area.move_cursor(CursorMove::End);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('o'), false) => {
                self.text_area.move_cursor(CursorMove::End);
                self.text_area.insert_newline();
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('O'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
                self.text_area.insert_newline();
                self.text_area.move_cursor(CursorMove::Up);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('I'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('d'), true) => {
                self.text_area.scroll(Scrolling::HalfPageDown);
            }
            (KeyCode::Char('u'), true) => {
                self.text_area.scroll(Scrolling::HalfPageUp);
            }
            (KeyCode::Char('f'), true) => {
                self.text_area.scroll(Scrolling::PageDown);
            }
            (KeyCode::Char('b'), true) => {
                self.text_area.scroll(Scrolling::PageUp);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_pending_operator(
        &mut self,
        input: &Input,
        sync_os_clipboard: bool,
    ) -> anyhow::Result<()> {
        let Some(op) = self.pending_operator.take() else {
            return Ok(());
        };

        if !input.modifiers.is_empty() {
            return Ok(());
        }

        let acted = match (op, input.key_code) {
            (Operator::Delete, KeyCode::Char('d')) => self.delete_current_line(),
            (Operator::Change, KeyCode::Char('c')) => {
                self.text_area.move_cursor(CursorMove::Head);
                self.text_area.delete_line_by_end()
            }
            (_, KeyCode::Char('w')) | (_, KeyCode::Char('e')) => self.text_area.delete_next_word(),
            (_, KeyCode::Char('b')) => self.text_area.delete_word(),
            (_, KeyCode::Char('h')) => self.text_area.delete_char(),
            (_, KeyCode::Char('l')) => self.text_area.delete_next_char(),
            (_, KeyCode::Char('j')) => {
                let first = self.delete_current_line();
                let second = self.delete_current_line();
                first || second
            }
            (_, KeyCode::Char('k')) => {
                let (row, _) = self.text_area.cursor();
                if row == 0 {
                    self.delete_current_line()
                } else {
                    self.text_area.move_cursor(CursorMove::Up);
                    let above = self.delete_current_line();
                    let here = self.delete_current_line();
                    above || here
                }
            }
            (_, KeyCode::Char('0')) | (_, KeyCode::Char('^')) => {
                self.text_area.delete_line_by_head()
            }
            (_, KeyCode::Char('$')) => self.text_area.delete_line_by_end(),
            _ => return Ok(()),
        };

        if acted && sync_os_clipboard {
            self.exec_os_clipboard(ClipboardOperation::Copy)?;
        }

        if matches!(op, Operator::Change) {
            self.set_editor_mode(EditorMode::Insert);
        }

        Ok(())
    }

    fn delete_current_line(&mut self) -> bool {
        let (row, _) = self.text_area.cursor();
        let line_count = self.text_area.lines().len();

        self.text_area.move_cursor(CursorMove::Head);

        if row + 1 < line_count {
            self.text_area.start_selection();
            self.text_area.move_cursor(CursorMove::Down);
            self.text_area.move_cursor(CursorMove::Head);
            self.text_area.cut()
        } else if row > 0 {
            self.text_area.move_cursor(CursorMove::End);
            self.text_area.start_selection();
            self.text_area.move_cursor(CursorMove::Up);
            self.text_area.move_cursor(CursorMove::End);
            self.text_area.cut()
        } else {
            self.text_area.delete_line_by_end()
        }
    }

    /// Up on first line / Down on last line snaps to line start/end instead of no-op.
    fn handle_mention_navigation(&mut self, input: &Input) -> bool {
        let Some(mention) = self.mention.as_mut() else {
            return false;
        };
        if !input.modifiers.is_empty() {
            return false;
        }
        match input.key_code {
            KeyCode::Esc => {
                self.mention = None;
                true
            }
            KeyCode::Up => {
                mention.move_up();
                true
            }
            KeyCode::Down => {
                mention.move_down();
                true
            }
            _ => false,
        }
    }

    fn handle_mention_commit(&mut self, input: &Input) -> bool {
        if self.mention.is_none() {
            return false;
        }
        if !matches!(input.key_code, KeyCode::Tab | KeyCode::Enter) {
            return false;
        }
        if !input.modifiers.is_empty() {
            return false;
        }
        let Some(mention) = self.mention.as_ref() else {
            return false;
        };
        let Some(candidate) = mention.selected().cloned() else {
            self.mention = None;
            return false;
        };
        self.commit_mention(candidate.id);
        true
    }

    fn try_capture_mention_peek(&mut self, input: &Input) -> bool {
        if input
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        if !matches!(input.key_code, KeyCode::Char('K')) {
            return false;
        }
        let Some(id) = self.mention_at_cursor() else {
            return false;
        };
        self.pending_mention_peek = Some(id);
        true
    }

    fn try_capture_mention_follow(&mut self, input: &Input) -> bool {
        if input
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        if !matches!(input.key_code, KeyCode::Char('F')) {
            return false;
        }
        let Some((id, anchor)) = self.mention_at_cursor_with_anchor() else {
            return false;
        };
        self.pending_mention_follow = Some(super::MentionFollow { id, anchor });
        true
    }

    pub(crate) fn mention_at_cursor(&self) -> Option<u32> {
        self.mention_at_cursor_with_anchor().map(|(id, _)| id)
    }

    fn mention_at_cursor_with_anchor(&self) -> Option<(u32, Option<String>)> {
        let (cursor_line, cursor_col) = self.text_area.cursor();
        let line = self.text_area.lines().get(cursor_line)?;
        let mentions = super::mention::parse_mentions_in_line(line);
        mentions
            .into_iter()
            .find(|m| cursor_col >= m.char_range.start && cursor_col < m.char_range.end)
            .map(|m| (m.id, m.anchor))
    }

    fn dismiss_mention_on_break_char(&mut self, input: &Input) {
        if self.mention.is_none() {
            return;
        }
        if let KeyCode::Char(c) = input.key_code
            && super::mention::is_break_char(c)
        {
            self.mention = None;
        }
    }

    fn maybe_open_mention<D: DataProvider>(&mut self, app: &App<D>) {
        let (cursor_line, cursor_col) = self.text_area.cursor();
        if cursor_col == 0 {
            return;
        }
        let at_col = cursor_col - 1;
        let Some(line) = self.text_area.lines().get(cursor_line) else {
            return;
        };
        if !super::mention::should_open_mention(line, at_col) {
            return;
        }
        let mut state = super::mention::MentionState::new(cursor_line, at_col);
        let candidates = super::mention::build_candidates(
            &app.entries,
            app.current_entry_id,
            &app.settings.date_format,
        );
        state.candidates = super::mention::filter_candidates(&candidates, &state.query);
        self.mention = Some(state);
    }

    fn update_mention<D: DataProvider>(&mut self, app: &App<D>) {
        let Some(mention) = self.mention.as_ref() else {
            return;
        };
        let (cursor_line, cursor_col) = self.text_area.cursor();
        if cursor_line != mention.anchor_line || cursor_col <= mention.anchor_col {
            self.mention = None;
            return;
        }
        let Some(line) = self.text_area.lines().get(cursor_line) else {
            self.mention = None;
            return;
        };
        let query: String = line
            .chars()
            .skip(mention.anchor_col + 1)
            .take(cursor_col - mention.anchor_col - 1)
            .collect();
        let candidates = super::mention::build_candidates(
            &app.entries,
            app.current_entry_id,
            &app.settings.date_format,
        );
        let filtered = super::mention::filter_candidates(&candidates, &query);
        if let Some(mention) = self.mention.as_mut() {
            mention.query = query;
            mention.candidates = filtered;
            if mention.selected_idx >= mention.candidates.len() {
                mention.selected_idx = 0;
            }
        }
    }

    fn commit_mention(&mut self, entry_id: u32) {
        const MAX_ANCHOR_LEN: usize = 30;
        let Some(mention) = self.mention.take() else {
            return;
        };
        let trimmed = mention.query.trim();
        let anchor: String = trimmed.chars().take(MAX_ANCHOR_LEN).collect();
        let anchor_opt = (!anchor.is_empty()).then_some(anchor);
        let token =
            super::mention::format_mention_token(entry_id, anchor_opt.as_deref());
        let (_, cursor_col) = self.text_area.cursor();
        let chars_to_remove = cursor_col.saturating_sub(mention.anchor_col);

        self.text_area.move_cursor(tui_textarea::CursorMove::Jump(
            mention.anchor_line as u16,
            mention.anchor_col as u16,
        ));
        for _ in 0..chars_to_remove {
            self.text_area.delete_next_char();
        }
        self.text_area.insert_str(&token);
    }

    fn try_visual_navigation(&mut self, input: &Input) -> bool {
        if !self.show_preview || !input.modifiers.is_empty() {
            return false;
        }
        let Some(width) = self.last_wrap_width else {
            return false;
        };
        let delta: i32 = match input.key_code {
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            _ => return false,
        };

        let lines_owned: Vec<String> = self.text_area.lines().to_vec();
        let lines: Vec<&str> = lines_owned.iter().map(String::as_str).collect();
        let (cursor_row, cursor_col) = self.text_area.cursor();
        let rows = super::render::word_wrap_lines(&lines, width);

        let Some((vrow, vcol)) =
            super::render::wrapped_cursor_position(&rows, cursor_row, cursor_col)
        else {
            return false;
        };

        let target_signed = vrow as i32 + delta;
        if target_signed < 0 {
            self.text_area.move_cursor(CursorMove::Top);
            self.text_area.move_cursor(CursorMove::Head);
            return true;
        }
        let (target_row, target_col) =
            super::render::visual_to_source(&rows, target_signed as u16, vcol);
        self.text_area
            .move_cursor(CursorMove::Jump(target_row as u16, target_col as u16));
        true
    }

    fn try_snap_vertical_navigation(&mut self, input: &Input) -> bool {
        if !input.modifiers.is_empty() || self.is_visual_mode() {
            return false;
        }
        match input.key_code {
            KeyCode::Up if self.is_on_first_line() => {
                self.text_area.move_cursor(CursorMove::Head);
                true
            }
            KeyCode::Down if self.is_on_last_line() => {
                self.text_area.move_cursor(CursorMove::End);
                true
            }
            _ => false,
        }
    }

    fn is_on_first_line(&self) -> bool {
        self.text_area.cursor().0 == 0
    }

    fn is_on_last_line(&self) -> bool {
        let (row, _) = self.text_area.cursor();
        row + 1 >= self.text_area.lines().len()
    }
}

fn is_default_navigation(input: &Input) -> bool {
    let has_control = input.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = input.modifiers.contains(KeyModifiers::ALT);
    match input.key_code {
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown => true,
        KeyCode::Char('p') if has_control || has_alt => true,
        KeyCode::Char('n') if has_control || has_alt => true,
        KeyCode::Char('f') if !has_control && has_alt => true,
        KeyCode::Char('b') if !has_control && has_alt => true,
        KeyCode::Char('e') if has_control || has_alt => true,
        KeyCode::Char('a') if has_control || has_alt => true,
        KeyCode::Char('v') if has_control || has_alt => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui::editor::Editor;
    use tui_textarea::TextArea;

    fn editor_with(lines: &[&str]) -> Editor<'static> {
        let mut e = Editor::new();
        e.text_area = TextArea::from(lines.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        e.set_editor_mode(EditorMode::Normal);
        e
    }

    fn key(c: char) -> Input {
        Input::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn esc() -> Input {
        Input::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    fn lines_of(e: &Editor<'_>) -> Vec<String> {
        e.text_area.lines().to_vec()
    }

    #[test]
    fn dd_deletes_middle_line() {
        let mut e = editor_with(&["alpha", "beta", "gamma"]);
        e.text_area.move_cursor(CursorMove::Down);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["alpha", "gamma"]);
        assert!(e.pending_operator.is_none());
    }

    #[test]
    fn dd_on_first_line() {
        let mut e = editor_with(&["alpha", "beta"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["beta"]);
    }

    #[test]
    fn dd_on_last_line() {
        let mut e = editor_with(&["alpha", "beta"]);
        e.text_area.move_cursor(CursorMove::Down);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["alpha"]);
    }

    #[test]
    fn dd_on_only_line() {
        let mut e = editor_with(&["solo"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert_eq!(lines_of(&e), vec![""]);
    }

    #[test]
    fn dw_deletes_next_word() {
        let mut e = editor_with(&["aaa bbb ccc"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('w'), false).unwrap();
        assert_eq!(lines_of(&e), vec![" bbb ccc"]);
    }

    #[test]
    fn db_deletes_previous_word() {
        let mut e = editor_with(&["aaa bbb ccc"]);
        e.text_area.move_cursor(CursorMove::End);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('b'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["aaa bbb "]);
    }

    #[test]
    fn dj_deletes_two_lines() {
        let mut e = editor_with(&["one", "two", "three", "four"]);
        e.text_area.move_cursor(CursorMove::Down);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('j'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["one", "four"]);
    }

    #[test]
    fn dk_deletes_current_and_above() {
        let mut e = editor_with(&["one", "two", "three", "four"]);
        e.text_area.move_cursor(CursorMove::Down);
        e.text_area.move_cursor(CursorMove::Down);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('k'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["one", "four"]);
    }

    #[test]
    fn dh_deletes_previous_char() {
        let mut e = editor_with(&["abcd"]);
        e.text_area.move_cursor(CursorMove::Forward);
        e.text_area.move_cursor(CursorMove::Forward);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('h'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["acd"]);
    }

    #[test]
    fn dl_deletes_next_char() {
        let mut e = editor_with(&["abcd"]);
        e.text_area.move_cursor(CursorMove::Forward);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('l'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["acd"]);
    }

    #[test]
    fn d_dollar_deletes_to_end() {
        let mut e = editor_with(&["abcdef"]);
        e.text_area.move_cursor(CursorMove::Forward);
        e.text_area.move_cursor(CursorMove::Forward);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('$'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["ab"]);
    }

    #[test]
    fn d_caret_deletes_to_head() {
        let mut e = editor_with(&["abcdef"]);
        e.text_area.move_cursor(CursorMove::Forward);
        e.text_area.move_cursor(CursorMove::Forward);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('^'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["cdef"]);
    }

    #[test]
    fn cw_deletes_word_and_enters_insert() {
        let mut e = editor_with(&["aaa bbb"]);
        e.handle_vim_motions(&key('c'), false).unwrap();
        e.handle_vim_motions(&key('w'), false).unwrap();
        assert_eq!(lines_of(&e), vec![" bbb"]);
        assert_eq!(e.get_editor_mode(), EditorMode::Insert);
    }

    #[test]
    fn cc_clears_line_and_enters_insert() {
        let mut e = editor_with(&["alpha", "beta"]);
        e.handle_vim_motions(&key('c'), false).unwrap();
        e.handle_vim_motions(&key('c'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["", "beta"]);
        assert_eq!(e.get_editor_mode(), EditorMode::Insert);
    }

    #[test]
    fn esc_clears_pending_operator() {
        let mut e = editor_with(&["alpha"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert!(e.pending_operator.is_some());
        e.handle_vim_motions(&esc(), false).unwrap();
        assert!(e.pending_operator.is_none());
        assert_eq!(lines_of(&e), vec!["alpha"]);
    }

    #[test]
    fn unsupported_motion_drops_strict() {
        let mut e = editor_with(&["alpha"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('q'), false).unwrap();
        assert!(e.pending_operator.is_none());
        assert_eq!(lines_of(&e), vec!["alpha"]);
        assert_eq!(e.text_area.cursor(), (0, 0));
    }

    #[test]
    fn mode_change_clears_pending() {
        let mut e = editor_with(&["alpha"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.set_editor_mode(EditorMode::Insert);
        assert!(e.pending_operator.is_none());
    }

    #[test]
    fn undo_after_dd_restores_line() {
        let mut e = editor_with(&["alpha", "beta", "gamma"]);
        e.text_area.move_cursor(CursorMove::Down);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('d'), false).unwrap();
        assert_eq!(lines_of(&e), vec!["alpha", "gamma"]);
        e.text_area.undo();
        assert_eq!(lines_of(&e), vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn undo_after_dw_restores_word() {
        let mut e = editor_with(&["aaa bbb ccc"]);
        e.handle_vim_motions(&key('d'), false).unwrap();
        e.handle_vim_motions(&key('w'), false).unwrap();
        assert_eq!(lines_of(&e), vec![" bbb ccc"]);
        e.text_area.undo();
        assert_eq!(lines_of(&e), vec!["aaa bbb ccc"]);
    }
}
