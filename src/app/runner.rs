use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream, KeyEventKind, MouseButton, MouseEventKind};
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

#[cfg(unix)]
struct TerminationSignals {
    sigterm: tokio::signal::unix::Signal,
    sigint: tokio::signal::unix::Signal,
    sighup: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl TerminationSignals {
    fn new() -> Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            sigterm: signal(SignalKind::terminate()).context("registering SIGTERM handler")?,
            sigint: signal(SignalKind::interrupt()).context("registering SIGINT handler")?,
            sighup: signal(SignalKind::hangup()).context("registering SIGHUP handler")?,
        })
    }

    async fn recv(&mut self) -> &'static str {
        tokio::select! {
            _ = self.sigterm.recv() => "SIGTERM",
            _ = self.sigint.recv() => "SIGINT",
            _ = self.sighup.recv() => "SIGHUP",
        }
    }
}

#[cfg(not(unix))]
struct TerminationSignals;

#[cfg(not(unix))]
impl TerminationSignals {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    async fn recv(&mut self) -> &'static str {
        std::future::pending().await
    }
}

/// One-shot CLI entry point that skips terminal init. Used for headless
/// commands (export, log) so their stdout/stderr output is visible instead
/// of being swallowed by the alternate screen.
pub async fn run_headless(settings: Settings, cmd: PendingCliCommand) -> Result<()> {
    debug_assert!(
        cmd.is_headless(),
        "run_headless invoked with non-headless command"
    );
    match settings.backend_type.unwrap_or_default() {
        #[cfg(feature = "json")]
        BackendType::Json => {
            let path = if let Some(path) = &settings.json_backend.file_path {
                path.clone()
            } else {
                crate::settings::json_backend::get_default_json_path()?
            };
            let data_provider = JsonDataProvide::new(path);
            let app = App::new(data_provider, settings);
            exec_headless_cmd(&app, cmd).await
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
            let app = App::new(data_provider, settings);
            exec_headless_cmd(&app, cmd).await
        }
        #[cfg(not(feature = "sqlite"))]
        BackendType::Sqlite => {
            anyhow::bail!(
                "Feature 'sqlite' is not installed. Please check your configs and set your backend to an installed feature, or reinstall the program with 'sqlite' feature"
            )
        }
    }
}

async fn exec_headless_cmd<D: DataProvider>(app: &App<D>, cmd: PendingCliCommand) -> Result<()> {
    match cmd {
        PendingCliCommand::ExportActivityLog => {
            let entries = app.get_activity_log().await?;
            let json = serde_json::to_string_pretty(&entries)
                .context("Serializing activity log to JSON")?;
            println!("{json}");
        }
        PendingCliCommand::ExportToDirectory {
            dir,
            tag,
            filename_format,
        } => {
            println!("Exporting entries to {} ...", dir.display());
            let written = app
                .export_to_directory(dir.clone(), tag, filename_format)
                .await?;
            log::info!("Exported {written} entries to {}", dir.display());
            println!("Exported {written} entries to {}", dir.display());
        }
        PendingCliCommand::ObsidianSync { force } => {
            run_obsidian_sync_headless(&app.data_provide, &app.settings.obsidian, force).await?;
        }
        PendingCliCommand::ObsidianStatus => {
            print_obsidian_status(&app.data_provide, &app.settings.obsidian).await?;
        }
        PendingCliCommand::SyncAll { force_obsidian } => {
            run_sync_all_headless(app, force_obsidian).await?;
        }
        _ => unreachable!("exec_headless_cmd called with non-headless command"),
    }
    Ok(())
}

async fn run_obsidian_sync_headless<D: DataProvider>(
    provider: &D,
    settings: &crate::settings::obsidian::ObsidianSettings,
    force: bool,
) -> Result<()> {
    if !settings.is_configured() {
        anyhow::bail!(
            "obsidian.vault_dir is not configured; add it to your config.toml under [obsidian]"
        );
    }
    let vault = settings.vault_dir.as_ref().unwrap().display();
    println!("Syncing entries to obsidian vault {vault} ...");
    let outcome = crate::obsidian::push_to_obsidian(provider, settings, force).await?;
    print_sync_outcome(&outcome);
    if outcome.errored() > 0 {
        anyhow::bail!("obsidian sync completed with {} errors", outcome.errored());
    }
    if outcome.conflicted() > 0 {
        anyhow::bail!(
            "obsidian sync refused to overwrite {} files modified since last sync; re-run with --force after merging",
            outcome.conflicted()
        );
    }
    Ok(())
}

async fn print_obsidian_status<D: DataProvider>(
    provider: &D,
    settings: &crate::settings::obsidian::ObsidianSettings,
) -> Result<()> {
    if !settings.is_configured() {
        println!("obsidian: not configured (set obsidian.vault_dir in config.toml)");
        return Ok(());
    }
    let entries = provider.load_all_entries().await?;
    let live: Vec<_> = entries.iter().filter(|e| e.deleted_at.is_none()).collect();
    let synced = live
        .iter()
        .filter(|e| e.obsidian_synced_at.is_some())
        .count();
    let unsynced = live.len() - synced;
    let pending_deletes = entries
        .iter()
        .filter(|e| e.deleted_at.is_some() && e.obsidian_filename.is_some())
        .count();
    let last_sync = live
        .iter()
        .filter_map(|e| e.obsidian_synced_at)
        .max()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| "never".to_string());
    let vault = settings.vault_dir.as_ref().unwrap().display();
    println!("obsidian.vault_dir = {vault}");
    println!(
        "filename_format    = {}",
        settings.filename_format_or_default()
    );
    println!(
        "category_dirs      = {} mapped",
        settings.category_dirs.len()
    );
    println!("synced entries     = {synced} / {}", live.len());
    println!("unsynced           = {unsynced}");
    println!("pending deletes    = {pending_deletes}");
    println!("last sync          = {last_sync}");
    Ok(())
}

async fn run_sync_all_headless<D: DataProvider>(app: &App<D>, force_obsidian: bool) -> Result<()> {
    println!("Running notion sync ...");
    let notion_settings = app.settings.notion.clone();
    use crate::settings::notion::SyncMode;
    let notion_result = match notion_settings.sync_mode {
        SyncMode::Pull | SyncMode::TwoWay => {
            crate::notion::pull_from_notion(&app.data_provide, &notion_settings, None).await
                .map(|o| format!(
                    "notion pull: inserted={}, updated={}, unchanged={}, local_wins={}, errored={}",
                    o.inserted, o.updated, o.unchanged, o.local_wins, o.errored
                ))
        }
        SyncMode::Push => {
            crate::notion::push_to_notion(&app.data_provide, &notion_settings, None).await
                .map(|o| format!(
                    "notion push: created={}, updated={}, archived={}, skipped_unchanged={}, skipped_conflict={}, errored={}",
                    o.created, o.updated, o.archived, o.skipped_unchanged, o.skipped_conflict, o.errored
                ))
        }
        SyncMode::LocalOnly => Ok("notion: skipped (sync_mode = local_only)".to_string()),
    };
    let notion_ok = match &notion_result {
        Ok(msg) => {
            println!("{msg}");
            true
        }
        Err(err) => {
            println!("notion sync failed: {err}");
            false
        }
    };

    if app.settings.obsidian.is_configured() {
        println!("Running obsidian sync ...");
        let vault = app.settings.obsidian.vault_dir.as_ref().unwrap().display();
        match crate::obsidian::push_to_obsidian(
            &app.data_provide,
            &app.settings.obsidian,
            force_obsidian,
        )
        .await
        {
            Ok(outcome) => {
                println!("obsidian sync to {vault}:");
                print_sync_outcome(&outcome);
            }
            Err(err) => {
                println!("obsidian sync failed: {err}");
                if notion_ok {
                    log::warn!("sync-all: notion succeeded but obsidian failed: {err}");
                }
            }
        }
    } else {
        println!("obsidian: not configured, skipping");
    }

    if !notion_ok {
        anyhow::bail!("notion sync failed (see message above)");
    }
    Ok(())
}

async fn fire_obsidian_after_notion<D: DataProvider>(
    provider: &D,
    settings: &crate::settings::obsidian::ObsidianSettings,
) {
    if !settings.enable_on_notion_sync || !settings.is_configured() {
        return;
    }
    match crate::obsidian::push_to_obsidian(provider, settings, false).await {
        Ok(outcome) => {
            log::info!(
                "Obsidian sync (after notion): written={}, skipped_unchanged={}, deleted={}, conflicts={}, unmapped={}, errored={}",
                outcome.written(),
                outcome.skipped(),
                outcome.deleted(),
                outcome.conflicted(),
                outcome.unmapped(),
                outcome.errored()
            );
        }
        Err(err) => {
            log::warn!("Obsidian sync after notion failed (notion sync was successful): {err}");
        }
    }
}

fn print_sync_outcome(outcome: &crate::obsidian::SyncOutcome) {
    println!(
        "  written={}, skipped_unchanged={}, deleted={}, conflicts={}, unmapped={}, errored={}",
        outcome.written(),
        outcome.skipped(),
        outcome.deleted(),
        outcome.conflicted(),
        outcome.unmapped(),
        outcome.errored()
    );
    for action in &outcome.actions {
        match action {
            crate::obsidian::EntryAction::Conflict { entry_id, path } => {
                println!("  conflict: entry {entry_id} -> {}", path.display());
            }
            crate::obsidian::EntryAction::UnmappedCategory { entry_id, category } => {
                println!("  unmapped: entry {entry_id} (category '{category}')");
            }
            crate::obsidian::EntryAction::Errored { entry_id, message } => {
                println!("  error: entry {entry_id}: {message}");
            }
            _ => {}
        }
    }
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

    let initial_entry_id = app.get_active_entries().next().map(|entry| entry.id);
    ui_components.set_current_entry(initial_entry_id, &mut app);

    draw_ui(terminal, &mut app, &mut ui_components)?;

    let mut input_stream = EventStream::new();
    let mut termination = TerminationSignals::new()?;
    loop {
        let event = tokio::select! {
            evt = input_stream.next() => match evt {
                Some(Ok(e)) => e,
                Some(Err(err)) => {
                    return Err(err).context("Error getting input stream");
                }
                None => break,
            },
            sig = termination.recv() => {
                log::info!("{sig} received, exiting gracefully");
                if let Err(err) = app.persist_state() {
                    log::error!("Persisting app state failed: Error info {err}");
                }
                return Ok(());
            }
        };
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
                        if let Err(err) = app.persist_state() {
                            log::error!("Persisting app state failed: Error info {err}");
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

        if app.should_push_on_exit {
            app.should_push_on_exit = false;
            match run_notion_push(terminal, &app.data_provide, &app.settings.notion).await {
                Ok(outcome) => {
                    log::info!(
                        "Notion push: created={}, updated={}, archived={}, skipped_unchanged={}, skipped_conflict={}, errored={}",
                        outcome.created,
                        outcome.updated,
                        outcome.archived,
                        outcome.skipped_unchanged,
                        outcome.skipped_conflict,
                        outcome.errored,
                    );
                    fire_obsidian_after_notion(&app.data_provide, &app.settings.obsidian).await;
                    if let Err(err) = app.load_entries().await {
                        log::warn!("Failed to refresh entries after push: {err}");
                    }
                    if outcome.errored > 0 {
                        ui_components.show_err_msg(format!(
                            "Notion push completed with {} errored entries. Check log for details.",
                            outcome.errored
                        ));
                    } else if ui_components.pending_exit_after_push {
                        ui_components.pending_exit_after_push = false;
                        if let Err(err) = app.persist_state() {
                            log::error!("Persisting app state failed: Error info {err}");
                        }
                        return Ok(());
                    }
                }
                Err(err) => {
                    log::error!("Notion push failed: {err}");
                    ui_components.show_err_msg(format!("Notion push failed: {err}"));
                }
            }
            draw_ui(terminal, &mut app, &mut ui_components)?;
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
            let outcome = run_notion_pull(terminal, &app.data_provide, &notion_settings).await?;
            log::info!(
                "Notion pull finished: inserted={}, updated={}, unchanged={}, local_wins={}, errored={}",
                outcome.inserted,
                outcome.updated,
                outcome.unchanged,
                outcome.local_wins,
                outcome.errored
            );
            fire_obsidian_after_notion(&app.data_provide, &app.settings.obsidian).await;
        }
        PendingCliCommand::ExportActivityLog => {
            let entries = app.get_activity_log().await?;
            let json = serde_json::to_string_pretty(&entries)
                .context("Serializing activity log to JSON")?;
            println!("{json}");
        }
        PendingCliCommand::ExportToDirectory {
            dir,
            tag,
            filename_format,
        } => {
            let written = app
                .export_to_directory(dir.clone(), tag, filename_format)
                .await?;
            log::info!("Exported {written} entries to {}", dir.display());
            println!("Exported {written} entries to {}", dir.display());
        }
        PendingCliCommand::NotionPush { database_id } => {
            let mut notion_settings = app.settings.notion.clone();
            if let Some(id) = database_id {
                notion_settings.database_id = Some(id);
            }
            let outcome = run_notion_push(terminal, &app.data_provide, &notion_settings).await?;
            log::info!(
                "Notion push finished: created={}, updated={}, archived={}, skipped_unchanged={}, skipped_conflict={}, errored={}",
                outcome.created,
                outcome.updated,
                outcome.archived,
                outcome.skipped_unchanged,
                outcome.skipped_conflict,
                outcome.errored
            );
            fire_obsidian_after_notion(&app.data_provide, &app.settings.obsidian).await;
        }
        PendingCliCommand::ObsidianSync { .. }
        | PendingCliCommand::ObsidianStatus
        | PendingCliCommand::SyncAll { .. } => {
            unreachable!("headless command leaked into TUI exec path")
        }
    }

    Ok(())
}

async fn run_notion_bootstrap<B: Backend, D: DataProvider>(
    terminal: &mut Terminal<B>,
    provider: &D,
    settings: &NotionSettings,
    force: bool,
) -> anyhow::Result<crate::notion::BootstrapOutcome> {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
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
    match event {
        Event::Key(key) => match key.kind {
            KeyEventKind::Press => {
                let input = Input::from(&key);
                ui_components.handle_input(&input, app).await
            }
            KeyEventKind::Repeat | KeyEventKind::Release => Ok(HandleInputReturnType::Ignore),
        },
        Event::Mouse(mouse) => {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                if !app.state.full_screen && ui_components.is_on_divider(mouse.column, mouse.row) {
                    ui_components.resizing_divider = true;
                    return Ok(HandleInputReturnType::Handled);
                }
                ui_components
                    .handle_mouse_click(mouse.column, mouse.row, app)
                    .await
            } else if matches!(mouse.kind, MouseEventKind::Drag(MouseButton::Left)) {
                if ui_components.resizing_divider
                    && ui_components.update_divider_from_drag(mouse.column, app)
                    && let Err(err) = app.persist_state()
                {
                    log::error!("Persisting state after divider drag failed: {err}");
                }
                Ok(HandleInputReturnType::Handled)
            } else if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
                if ui_components.resizing_divider {
                    ui_components.resizing_divider = false;
                    if let Err(err) = app.persist_state() {
                        log::error!("Persisting state after divider release failed: {err}");
                    }
                    Ok(HandleInputReturnType::Handled)
                } else {
                    Ok(HandleInputReturnType::Ignore)
                }
            } else if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                ui_components.handle_mouse_scroll(
                    mouse.column,
                    mouse.row,
                    crate::app::ui::ScrollDirection::Up,
                    app,
                );
                Ok(HandleInputReturnType::Handled)
            } else if matches!(mouse.kind, MouseEventKind::ScrollDown) {
                ui_components.handle_mouse_scroll(
                    mouse.column,
                    mouse.row,
                    crate::app::ui::ScrollDirection::Down,
                    app,
                );
                Ok(HandleInputReturnType::Handled)
            } else if matches!(mouse.kind, MouseEventKind::Moved) {
                let on_divider =
                    !app.state.full_screen && ui_components.is_on_divider(mouse.column, mouse.row);
                if on_divider != ui_components.hover_on_divider {
                    ui_components.hover_on_divider = on_divider;
                    Ok(HandleInputReturnType::Handled)
                } else {
                    Ok(HandleInputReturnType::Ignore)
                }
            } else {
                Ok(HandleInputReturnType::Ignore)
            }
        }
        _ => Ok(HandleInputReturnType::NotFound),
    }
}
