use crate::app::{App, UIComponents, ui::*};

use backend::DataProvider;

use crate::app::ui::commands::{
    CmdResult,
    editor_cmd::{discard_current_content, exec_save_entry_content},
};

pub async fn exec_show_revision_history<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> CmdResult {
    let Some(current) = app.get_current_entry() else {
        return Ok(HandleInputReturnType::Handled);
    };
    let entry_id = current.id;
    let entry_title = current.title.clone();

    match app.get_revisions(entry_id).await {
        Ok(revisions) => {
            ui_components.open_revision_popup(revisions, entry_title, &app.settings);
        }
        Err(err) => {
            ui_components.show_err_msg(format!("Failed to load revisions: {err}"));
        }
    }
    Ok(HandleInputReturnType::Handled)
}

pub fn exec_toggle_full_screen_mode<D: DataProvider>(app: &mut App<D>) -> CmdResult {
    app.state.full_screen = !app.state.full_screen;
    Ok(HandleInputReturnType::Handled)
}

pub fn exec_cycle_view_category<D: DataProvider>(app: &mut App<D>, step: i32) -> CmdResult {
    let categories = crate::app::categories::ordered_categories(&app.entries);
    let next = crate::app::categories::cycle_category(&categories, &app.view_category, step);
    app.view_category = next.clone();
    app.state.last_view_category = next;
    Ok(HandleInputReturnType::Handled)
}

pub fn exec_show_sort_options<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::ShowSortOptions));
    } else {
        show_sort_options(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

fn show_sort_options<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    ui_components
        .popup_stack
        .push(Popup::Sort(Box::new(SortPopup::new(&app.state.sorter))));
}

pub async fn continue_show_sort_options<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            show_sort_options(ui_components, app);
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);

            show_sort_options(ui_components, app);
        }
    }

    Ok(HandleInputReturnType::Handled)
}
