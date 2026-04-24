use anyhow::{anyhow, bail};
use arboard::Clipboard;

use crate::app::{runner::HandleInputReturnType, ui::commands::ClipboardOperation};

use super::Editor;

impl Editor<'_> {
    pub fn exec_os_clipboard(
        &mut self,
        operation: ClipboardOperation,
    ) -> anyhow::Result<HandleInputReturnType> {
        let mut clipboard = Clipboard::new().map_err(map_clipboard_error)?;

        match operation {
            ClipboardOperation::Copy => {
                self.text_area.copy();
                let selected_text = self.text_area.yank_text();
                clipboard
                    .set_text(selected_text)
                    .map_err(map_clipboard_error)?;
            }
            ClipboardOperation::Cut => {
                if self.text_area.cut() {
                    self.is_dirty = true;
                    self.has_unsaved = true;
                }
                let selected_text = self.text_area.yank_text();
                clipboard
                    .set_text(selected_text)
                    .map_err(map_clipboard_error)?;
            }
            ClipboardOperation::Paste => {
                let content = clipboard.get_text().map_err(map_clipboard_error)?;
                if content.is_empty() {
                    return Ok(HandleInputReturnType::Handled);
                }

                if !self.text_area.insert_str(content) {
                    bail!("Text can't be pasted into editor")
                }
                self.is_dirty = true;
                self.has_unsaved = true;
            }
        }

        Ok(HandleInputReturnType::Handled)
    }
}

fn map_clipboard_error(err: arboard::Error) -> anyhow::Error {
    anyhow!("Error while communicating with the operation system clipboard.\nError Details: {err}",)
}
