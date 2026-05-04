use std::path::PathBuf;

use backend::DataProvider;

use super::{App, MsgBox, MsgBoxActions, MsgBoxType, Popup, UIComponents};

impl UIComponents<'_> {
    pub(super) async fn handle_export_popup_return<D: DataProvider>(
        &mut self,
        path: PathBuf,
        entry_id: Option<u32>,
        app: &mut App<D>,
    ) {
        let (result, confirmation_msg) = if self.entries_list.multi_select_mode {
            let result = app.export_entries(path.clone()).await;
            let msg = format!("Journal(s)  exported to file {}", path.display());

            (result, msg)
        } else {
            let entry_id = entry_id.expect("entry id must have a value in normal mode");
            let result = app.export_entry_content(entry_id, path.clone()).await;
            let msg = format!("Journal content exported to file {}", path.display());

            (result, msg)
        };

        match result {
            Ok(_) => {
                self.popup_stack.pop().expect("popup stack isn't empty");

                if app.settings.export.show_confirmation {
                    self.show_export_confirmation(confirmation_msg, path);
                }
            }
            Err(err) => {
                self.show_err_msg(format!("Error while exporting journal(s). Err: {err}",));
            }
        };
    }

    fn show_export_confirmation(&mut self, msg: String, path: PathBuf) {
        self.pending_command = None;
        let msg_box =
            MsgBox::new(MsgBoxType::Info(msg), MsgBoxActions::OkReveal).with_reveal_path(path);
        self.popup_stack.push(Popup::MsgBox(Box::new(msg_box)));
    }
}
