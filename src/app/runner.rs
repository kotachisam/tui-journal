use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream, KeyEventKind};
use ratatui::{Terminal, backend::Backend};

use crate::app::{App, UIComponents};
use crate::cli::PendingCliCommand;
use crate::settings::{BackendType, Settings};
use futures_util::StreamExt;

use backend::DataProvider;
#[cfg(feature = "json")]
use backend::JsonDataProvide;
#[cfg(feature = "sqlite")]
use backend::SqliteDataProvide;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap};
use tokio::sync::mpsc::unbounded_channel;

use crate::notion::{
    SyncProgress, SyncStage, bootstrap_from_notion, pull_from_notion, push_to_notion,
};
use crate::settings::notion::NotionSettings;

use super::keymap::Input;
use super::ui::Styles;
use super::ui::ui_functions::{centered_rect_exact_height, render_message_centered};

#[derive(Debug, PartialEq, Eq)]
pub enum HandleInputReturnType {
    Handled,
    NotFound,
    ExitApp,
    Ignore,
}

pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    settings: Settings,
    styles: Styles,
    pending_cmd: Option<PendingCliCommand>,
) -> Result<()> {
    match settings.backend_type.unwrap_or_default() {
        #[cfg(feature = "json")]
        BackendType::Json => {
            let path = if let Some(path) = &settings.json_backend.file_path {
                path.clone()
            } else {
                crate::settings::json_backend::get_default_json_path()?
            };
            let data_provider = JsonDataProvide::new(path);
            run_intern(terminal, data_provider, settings, styles, pending_cmd).await
        }
        #[cfg(not(feature = "json"))]
        BackendType::Json => {
            anyhow::bail!(
                "Feature 'json' is not installed. Please check your configs and set your backend to an installed feature, or reinstall the program with 'json' feature"
            )
        }
        #[cfg(feature = "sqlite")]
        BackendType::Sqlite => {
            let path = if let Some(path) = &settings.sqlite_backend.file_path {
                path.clone()
            } else {
                crate::settings::sqlite_backend::get_default_sqlite_path()?
            };
            let data_provider = SqliteDataProvide::from_file(path).await?;
            run_intern(terminal, data_provider, settings, styles, pending_cmd).await
        }
        #[cfg(not(feature = "sqlite"))]
        BackendType::Sqlite => {
            anyhow::bail!(
                "Feature 'sqlite' is not installed. Please check your configs and set your backend to an installed feature, or reinstall the program with 'sqlite' feature"
            )
        }
    }
}

async fn run_intern<B, D>(
    terminal: &mut Terminal<B>,
    data_provider: D,
    settings: Settings,
    styles: Styles,
    pending_cmd: Option<PendingCliCommand>,
) -> anyhow::Result<()>
where
    B: Backend,
    D: DataProvider,
{
    let mut ui_components = UIComponents::new(styles);
    let mut app = App::new(data_provider, settings);
    if let Some(cmd) = pending_cmd {
        let exit_after = matches!(
            &cmd,
            PendingCliCommand::ExportToDirectory { .. } | PendingCliCommand::ExportActivityLog
        );
        if let Err(err) = exec_pending_cmd(terminal, &app, cmd).await {
            ui_components.show_err_msg(err.to_string());
        }
        if exit_after {
            return Ok(());
        }
    }

    app.load_state(&mut ui_components);

    if let Err(err) = app.load_entries().await {
        ui_components.show_err_msg(err.to_string());
    }

    ui_components.set_current_entry(app.entries.first().map(|entry| entry.id), &mut app);

    draw_ui(terminal, &mut app, &mut ui_components)?;

    let mut input_stream = EventStream::new();
    while let Some(event) = input_stream.next().await {
        let event = event.context("Error getting input stream")?;
        match handle_input(event, &mut app, &mut ui_components).await {
            Ok(result) => {
                match result {
                    HandleInputReturnType::Handled => {
                        ui_components.update_current_entry(&mut app);
                        draw_ui(terminal, &mut app, &mut ui_components)?;
                    }
                    HandleInputReturnType::NotFound => {
                        // UI should be drawn even if the input isn't handled in the app logic to
                        // catch events like resize, Font resize, Mouse activation...
                        draw_ui(terminal, &mut app, &mut ui_components)?;
                    }
                    HandleInputReturnType::ExitApp => {
                        // Logging persisting errors by closing the app is enough
                        if let Err(err) = app.persist_state() {
                            log::error!("Persisting app state failed: Error info {err}");
                        }

                        if app.should_push_on_exit {
                            match run_notion_push(
                                terminal,
                                &app.data_provide,
                                &app.settings.notion,
                            )
                            .await
                            {
                                Ok(outcome) => log::info!(
                                    "Exit-time Notion push: created={}, updated={}, archived={}, skipped_unchanged={}, skipped_conflict={}, errored={}",
                                    outcome.created,
                                    outcome.updated,
                                    outcome.archived,
                                    outcome.skipped_unchanged,
                                    outcome.skipped_conflict,
                                    outcome.errored,
                                ),
                                Err(err) => log::error!("Exit-time Notion push failed: {err}"),
                            }
                        }

                        return Ok(());
                    }
                    HandleInputReturnType::Ignore => {}
                };
            }
            Err(err) => {
                ui_components.show_err_msg(err.to_string());
                draw_ui(terminal, &mut app, &mut ui_components)?;
            }
        }
    }

    Ok(())
}

async fn exec_pending_cmd<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    app: &App<D>,
    pending_cmd: PendingCliCommand,
) -> anyhow::Result<()> {
    match pending_cmd {
        PendingCliCommand::ImportJournals(file_path) => {
            terminal.draw(|f| render_message_centered(f, "Importing journals..."))?;

            app.import_entries(file_path).await?;
        }
        PendingCliCommand::AssignPriority(priority) => {
            terminal.draw(|f| render_message_centered(f, "Assigning Priority to Journals..."))?;
            app.assign_priority_to_entries(priority).await?;
        }
        PendingCliCommand::NotionBootstrap { force, database_id } => {
            let mut notion_settings = app.settings.notion.clone();
            if let Some(id) = database_id {
                notion_settings.database_id = Some(id);
            }
            let outcome =
                run_notion_bootstrap(terminal, &app.data_provide, &notion_settings, force).await?;
            log::info!(
                "Notion bootstrap finished: inserted={}, skipped={}",
                outcome.inserted,
                outcome.skipped
            );
        }
        PendingCliCommand::NotionPull { database_id } => {
            let mut notion_settings = app.settings.notion.clone();
            if let Some(id) = database_id {
                notion_settings.database_id = Some(id);
            }
            let outcome =
                run_notion_pull(terminal, &app.data_provide, &notion_settings).await?;
            log::info!(
                "Notion pull finished: inserted={}, updated={}, unchanged={}, local_wins={}, errored={}",
                outcome.inserted,
                outcome.updated,
                outcome.unchanged,
                outcome.local_wins,
                outcome.errored
            );
        }
        PendingCliCommand::ExportActivityLog => {
            let entries = app.get_activity_log().await?;
            let json = serde_json::to_string_pretty(&entries)
                .context("Serializing activity log to JSON")?;
            println!("{json}");
        }
        PendingCliCommand::ExportToDirectory { dir, tag } => {
            let written = app.export_to_directory(dir.clone(), tag).await?;
            log::info!("Exported {written} entries to {}", dir.display());
            println!("Exported {written} entries to {}", dir.display());
        }
        PendingCliCommand::NotionPush { database_id } => {
            let mut notion_settings = app.settings.notion.clone();
            if let Some(id) = database_id {
                notion_settings.database_id = Some(id);
            }
            let outcome =
                run_notion_push(terminal, &app.data_provide, &notion_settings).await?;
            log::info!(
                "Notion push finished: created={}, updated={}, archived={}, skipped_unchanged={}, skipped_conflict={}, errored={}",
                outcome.created,
                outcome.updated,
                outcome.archived,
                outcome.skipped_unchanged,
                outcome.skipped_conflict,
                outcome.errored
            );
        }
    }

    Ok(())
}

async fn run_notion_bootstrap<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    provider: &D,
    settings: &NotionSettings,
    force: bool,
) -> anyhow::Result<crate::notion::bootstrap::BootstrapOutcome> {
    let (tx, mut rx) = unbounded_channel::<SyncProgress>();
    let mut latest = SyncProgress {
        stage: SyncStage::ResolvingDataSource,
        current: 0,
        total: 0,
    };
    terminal.draw(|f| render_sync_progress(f, &latest))?;

    let bootstrap = bootstrap_from_notion(provider, settings, force, Some(tx));
    tokio::pin!(bootstrap);

    loop {
        tokio::select! {
            result = &mut bootstrap => {
                return result;
            }
            progress = rx.recv() => {
                if let Some(p) = progress {
                    latest = p;
                    terminal.draw(|f| render_sync_progress(f, &latest))?;
                }
            }
        }
    }
}

async fn run_notion_pull<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    provider: &D,
    settings: &NotionSettings,
) -> anyhow::Result<crate::notion::PullOutcome> {
    let (tx, mut rx) = unbounded_channel::<SyncProgress>();
    let mut latest = SyncProgress {
        stage: SyncStage::ResolvingDataSource,
        current: 0,
        total: 0,
    };
    terminal.draw(|f| render_sync_progress(f, &latest))?;

    let pull = pull_from_notion(provider, settings, Some(tx));
    tokio::pin!(pull);

    loop {
        tokio::select! {
            result = &mut pull => {
                return result;
            }
            progress = rx.recv() => {
                if let Some(p) = progress {
                    latest = p;
                    terminal.draw(|f| render_sync_progress(f, &latest))?;
                }
            }
        }
    }
}

async fn run_notion_push<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    provider: &D,
    settings: &NotionSettings,
) -> anyhow::Result<crate::notion::PushOutcome> {
    let (tx, mut rx) = unbounded_channel::<SyncProgress>();
    let mut latest = SyncProgress {
        stage: SyncStage::ResolvingDataSource,
        current: 0,
        total: 0,
    };
    terminal.draw(|f| render_sync_progress(f, &latest))?;

    let push = push_to_notion(provider, settings, Some(tx));
    tokio::pin!(push);

    loop {
        tokio::select! {
            result = &mut push => {
                return result;
            }
            progress = rx.recv() => {
                if let Some(p) = progress {
                    latest = p;
                    terminal.draw(|f| render_sync_progress(f, &latest))?;
                }
            }
        }
    }
}

fn render_sync_progress(frame: &mut Frame, progress: &SyncProgress) {
    let area = centered_rect_exact_height(60, 7, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Notion sync ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let stage_line = Paragraph::new(progress.stage.label()).wrap(Wrap { trim: false });
    frame.render_widget(stage_line, chunks[0]);

    let ratio = if progress.total == 0 {
        0.0
    } else {
        (progress.current as f64 / progress.total as f64).min(1.0)
    };
    let gauge_label = if progress.total == 0 {
        String::new()
    } else {
        format!("{} / {}", progress.current, progress.total)
    };
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(ratio)
        .label(gauge_label);
    frame.render_widget(gauge, chunks[1]);
}

fn draw_ui<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    app: &mut App<D>,
    ui_components: &mut UIComponents,
) -> anyhow::Result<()> {
    if app.redraw_after_restore {
        app.redraw_after_restore = false;
        // Apply hide cursor again after closing the external editor
        terminal.hide_cursor()?;
        // Clear the terminal and force a full redraw on the next draw call.
        terminal.clear()?;
    }

    terminal.draw(|f| ui_components.render_ui(f, app))?;

    Ok(())
}

async fn handle_input<D: DataProvider>(
    event: Event,
    app: &mut App<D>,
    ui_components: &mut UIComponents<'_>,
) -> Result<HandleInputReturnType> {
    if let Event::Key(key) = event {
        match key.kind {
            KeyEventKind::Press => {
                let input = Input::from(&key);
                ui_components.handle_input(&input, app).await
            }
            KeyEventKind::Repeat | KeyEventKind::Release => Ok(HandleInputReturnType::Ignore),
        }
    } else {
        Ok(HandleInputReturnType::NotFound)
    }
}
