use tui_textarea::{CursorMove, TextArea};

use backend::DataProvider;

use crate::app::App;

use super::{Editor, EditorMode};

impl Editor<'_> {
    pub fn set_current_entry<D: DataProvider>(&mut self, entry_id: Option<u32>, app: &App<D>) {
        let text_area = match entry_id {
            Some(id) => {
                if let Some(entry) = app.get_entry(id) {
                    self.is_dirty = false;
                    let lines = entry.content.lines().map(|line| line.to_owned()).collect();
                    TextArea::new(lines)
                } else {
                    TextArea::default()
                }
            }
            None => TextArea::default(),
        };

        self.text_area = text_area;
        self.mode = EditorMode::Normal;
        self.show_preview = true;
        self.preview_scroll = 0;
        self.reroll_placeholder_seed();

        self.refresh_has_unsaved(app);
    }

    pub fn get_content(&self) -> String {
        let lines = self.text_area.lines().to_vec();

        lines.join("\n")
    }

    pub fn has_unsaved(&self) -> bool {
        self.has_unsaved
    }

    pub fn refresh_has_unsaved<D: DataProvider>(&mut self, app: &App<D>) {
        self.has_unsaved = match self.is_dirty {
            true => {
                if let Some(entry) = app.get_current_entry() {
                    self.is_dirty && entry.content != self.get_content()
                } else {
                    false
                }
            }
            false => false,
        }
    }

    pub fn text_undo(&mut self) -> bool {
        let acted = self.text_area.undo();
        if acted {
            self.is_dirty = true;
        }
        acted
    }

    pub fn text_redo(&mut self) -> bool {
        let acted = self.text_area.redo();
        if acted {
            self.is_dirty = true;
        }
        acted
    }

    pub fn set_entry_content<D: DataProvider>(&mut self, entry_content: &str, app: &App<D>) {
        self.is_dirty = true;
        let lines = entry_content.lines().map(|line| line.to_owned()).collect();
        let mut text_area = TextArea::new(lines);
        text_area.move_cursor(CursorMove::Bottom);
        text_area.move_cursor(CursorMove::End);

        self.text_area = text_area;

        self.refresh_has_unsaved(app);
    }
}
