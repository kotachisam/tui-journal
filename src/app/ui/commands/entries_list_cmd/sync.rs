use crate::app::{App, UIComponents, ui::*};

use backend::DataProvider;

use crate::app::ui::commands::CmdResult;

pub fn exec_sync_notion<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &App<D>,
) -> CmdResult {
    let unsynced = app.unsynced_count();
    if unsynced == 0 {
        ui_components.show_toast("Nothing to sync.".to_string());
        return Ok(HandleInputReturnType::Handled);
    }
    let noun = if unsynced == 1 { "entry" } else { "entries" };
    let msg = MsgBoxType::Question(format!("{unsynced} unsynced {noun}. Push to Notion now?"));
    ui_components.show_msg_box(msg, MsgBoxActions::YesNo, Some(UICommand::SyncNotion));
    Ok(HandleInputReturnType::Handled)
}

pub async fn continue_sync_notion<D: DataProvider>(
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    if msg_box_result == MsgBoxResult::Yes {
        app.should_push_on_exit = true;
    }
    Ok(HandleInputReturnType::Handled)
}
