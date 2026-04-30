use std::env;

use crate::app::{App, UIComponents, external_editor, ui::*};

use backend::DataProvider;

use crate::app::ui::commands::{
    CmdResult,
    editor_cmd::{discard_current_content, exec_save_entry_content},
};

pub fn exec_create_entry<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::CreateEntry));
    } else {
        create_entry(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn create_entry<D: DataProvider>(ui_components: &mut UIComponents, app: &App<D>) {
    ui_components
        .popup_stack
        .push(Popup::Entry(Box::new(EntryPopup::new_entry(
            &app.settings,
            &app.view_category,
        ))));
}

pub async fn continue_create_entry<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            create_entry(ui_components, app);
        }
        MsgBoxResult::No => create_entry(ui_components, app),
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_edit_current_entry<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::EditCurrentEntry));
    } else {
        edit_current_entry(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

fn edit_current_entry<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    if let Some(entry) = app.get_current_entry() {
        ui_components
            .popup_stack
            .push(Popup::Entry(Box::new(EntryPopup::from_entry(
                entry,
                &app.settings,
            ))));
    }
}

pub async fn continue_edit_current_entry<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            edit_current_entry(ui_components, app);
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);
            edit_current_entry(ui_components, app);
        }
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_delete_current_entry<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &App<D>,
) -> CmdResult {
    if app.current_entry_id.is_some() {
        let msg = MsgBoxType::Question("Do you want to remove the current journal?".into());
        let msg_actions = MsgBoxActions::YesNo;
        ui_components.show_msg_box(msg, msg_actions, Some(UICommand::DeleteCurrentEntry));
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn continue_delete_current_entry<D: DataProvider>(
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Yes => {
            app.delete_entry(
                app.current_entry_id
                    .expect("current entry must have a value"),
            )
            .await?;
        }
        MsgBoxResult::No => {}
        _ => unreachable!(
            "{:?} not implemented for delete current entry",
            msg_box_result
        ),
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_export_entry_content<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::ExportEntryContent));
    } else {
        export_entry_content(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn export_entry_content<D: DataProvider>(ui_components: &mut UIComponents, app: &App<D>) {
    if let Some(entry) = app.get_current_entry() {
        match ExportPopup::create_entry_content(entry, app) {
            Ok(popup) => ui_components
                .popup_stack
                .push(Popup::Export(Box::new(popup))),
            Err(err) => ui_components
                .show_err_msg(format!("Error while creating export dialog.\n Err: {err}")),
        }
    }
}

pub async fn continue_export_entry_content<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            export_entry_content(ui_components, app);
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);
            export_entry_content(ui_components, app);
        }
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn exec_edit_in_external_editor<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::EditInExternalEditor));
    } else {
        edit_in_external_editor(ui_components, app).await?;
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn edit_in_external_editor<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> anyhow::Result<()> {
    use tokio::fs;

    if let Some(entry) = app.get_current_entry() {
        const TEMP_FILENAME: &str = "tui_journal";
        let temp_extension = &app.settings.external_editor.temp_file_extension;

        let mut builder = tempfile::Builder::new();
        builder.prefix(TEMP_FILENAME);

        if !temp_extension.is_empty() {
            builder.suffix(temp_extension);
        };

        let temp_file = builder.tempfile_in(env::temp_dir())?;
        let file_path = temp_file.path();

        fs::write(file_path, entry.content.as_str()).await?;

        app.redraw_after_restore = true;

        external_editor::open_editor(file_path, &app.settings).await?;

        if file_path.exists() {
            let new_content = fs::read_to_string(&file_path).await?;
            ui_components.editor.set_entry_content(&new_content, app);
            ui_components.change_active_control(ControlType::EntriesList);

            if app.settings.external_editor.auto_save {
                exec_save_entry_content(ui_components, app).await?;
            }
        }
    }

    Ok(())
}

pub async fn continue_edit_in_external_editor<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            edit_in_external_editor(ui_components, app).await?;
        }
        MsgBoxResult::No => edit_in_external_editor(ui_components, app).await?,
    }

    Ok(HandleInputReturnType::Handled)
}
