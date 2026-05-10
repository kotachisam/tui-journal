use anyhow::Result;
use backend::DataProvider;

use super::{
    App, EntryPopup, EntryPopupInputReturn, HandleInputReturnType, HelpInputInputReturn, Input,
    MentionPeekReturn, Popup, PopupReturn, RevisionPopupReturn, TemplatePopupReturn, UIComponents,
    file_reveal, fuzz_find, msg_box,
};

impl UIComponents<'_> {
    pub(super) fn handle_mention_peek_popup(
        &mut self,
        input: &Input,
    ) -> Result<HandleInputReturnType> {
        let close = if let Some(Popup::MentionPeek(popup)) = self.popup_stack.last_mut() {
            matches!(popup.handle_input(input), MentionPeekReturn::Close)
        } else {
            false
        };
        if close {
            self.popup_stack.pop().expect("popup stack isn't empty");
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) fn handle_help_popup(&mut self, input: &Input) -> Result<HandleInputReturnType> {
        let close = if let Some(Popup::Help(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input) == HelpInputInputReturn::Close
        } else {
            false
        };
        if close {
            self.popup_stack.pop().expect("popup stack isn't empty");
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) async fn handle_entry_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Entry(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input, app).await?
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        let close_popup = match result {
            EntryPopupInputReturn::Cancel => true,
            EntryPopupInputReturn::KeepPopup => false,
            EntryPopupInputReturn::AddEntry(entry_id) => {
                self.set_current_entry(Some(entry_id), app);
                true
            }
            EntryPopupInputReturn::UpdateCurrentEntry => {
                self.set_current_entry(app.current_entry_id, app);
                true
            }
        };
        if close_popup {
            self.popup_stack.pop().expect("popup stack isn't empty");
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) async fn handle_msg_box_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::MsgBox(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            msg_box::MsgBoxInputResult::Keep => Ok(HandleInputReturnType::Handled),
            msg_box::MsgBoxInputResult::Close(msg_box_result) => {
                self.popup_stack.pop().expect("popup stack isn't empty");
                if let Some(cmd) = self.pending_command.take() {
                    return cmd.continue_executing(self, app, msg_box_result).await;
                }
                if self.pending_exit_after_push {
                    self.pending_exit_after_push = false;
                    return Ok(HandleInputReturnType::ExitApp);
                }
                Ok(HandleInputReturnType::Handled)
            }
            msg_box::MsgBoxInputResult::Reveal(path) => {
                self.popup_stack.pop().expect("popup stack isn't empty");
                if let Err(err) = file_reveal::reveal_in_file_manager(&path) {
                    self.show_err_msg(format!("Failed to reveal file: {err}"));
                }
                Ok(HandleInputReturnType::Handled)
            }
        }
    }

    pub(super) async fn handle_export_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Export(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            PopupReturn::KeepPopup => {}
            PopupReturn::Cancel => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            PopupReturn::Apply((path, entry_id)) => {
                self.handle_export_popup_return(path, entry_id, app).await;
            }
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) fn handle_filter_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Filter(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            PopupReturn::KeepPopup => {}
            PopupReturn::Cancel => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            PopupReturn::Apply(filter) => {
                app.apply_filter(filter);
                self.popup_stack.pop().expect("popup stack isn't empty");

                if app.get_active_entries().count() == 1 {
                    let entry_id = app.get_active_entries().next().map(|entry| entry.id);
                    self.set_current_entry(entry_id, app);
                }
            }
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) fn handle_fuzz_find_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let (result, query) = if let Some(Popup::FuzzFind(popup)) = self.popup_stack.last_mut() {
            let result = popup.handle_input(input);
            let query = popup.query().map(|q| q.to_owned());
            (result, query)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            fuzz_find::FuzzFindReturn::Close => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            fuzz_find::FuzzFindReturn::Commit => {
                app.last_search_query = query.filter(|q| !q.is_empty());
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            fuzz_find::FuzzFindReturn::SelectEntry(entry_id) => {
                if entry_id.is_some() {
                    self.set_current_entry(entry_id, app);
                }
            }
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) fn handle_sort_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Sort(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            PopupReturn::KeepPopup => {}
            PopupReturn::Cancel => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            PopupReturn::Apply(sort_result) => {
                self.popup_stack.pop().expect("popup stack isn't empty");

                let current_entry_id = app.current_entry_id;
                app.apply_sort(sort_result.applied_criteria, sort_result.order);
                self.set_current_entry(current_entry_id, app);
            }
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) fn handle_template_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Template(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            TemplatePopupReturn::Keep => {}
            TemplatePopupReturn::Cancel => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            TemplatePopupReturn::Apply(template) => {
                self.popup_stack.pop().expect("popup stack isn't empty");
                let entry_popup =
                    EntryPopup::from_template(&template, &app.settings, &app.view_category);
                self.popup_stack.push(Popup::Entry(Box::new(entry_popup)));
            }
        }
        Ok(HandleInputReturnType::Handled)
    }

    pub(super) async fn handle_revision_popup<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = if let Some(Popup::Revision(popup)) = self.popup_stack.last_mut() {
            popup.handle_input(input)
        } else {
            return Ok(HandleInputReturnType::Handled);
        };
        match result {
            RevisionPopupReturn::Keep => {}
            RevisionPopupReturn::Close => {
                self.popup_stack.pop().expect("popup stack isn't empty");
            }
            RevisionPopupReturn::Restore(revision) => {
                self.popup_stack.pop().expect("popup stack isn't empty");
                let target_id = revision.entry_id;
                match app.restore_from_revision(target_id, &revision).await {
                    Ok(()) => {
                        self.set_current_entry(Some(target_id), app);
                        self.show_info_msg(
                            "Revision restored. The pre-restore state has been saved as a new entry in the history."
                                .to_owned(),
                        );
                    }
                    Err(err) => {
                        self.show_err_msg(format!("Failed to restore revision: {err}"));
                    }
                }
            }
        }
        Ok(HandleInputReturnType::Handled)
    }
}
