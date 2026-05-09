use std::io::{self, Stdout};

use anyhow::{Context, Result};
use app::ui::Styles;
use clap::Parser;
use crossterm::{
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use settings::Settings;

mod app;
mod cli;
mod logging;
mod notion;
mod settings;

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = execute!(io::stdout(), DisableMouseCapture);
    drain_pending_events();
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

fn drain_pending_events() {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_millis(50);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match crossterm::event::poll(remaining.min(Duration::from_millis(10))) {
            Ok(true) => {
                let _ = crossterm::event::read();
            }
            _ => break,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Non-overwriting load: shell-exported values win over dotfile contents.
    let _ = dotenvy::from_filename(".dev.vars");
    let _ = dotenvy::dotenv();

    let cli = cli::Cli::parse();

    let custom_config = cli.config_path.clone();
    let mut settings = Settings::new(custom_config.clone()).await?;

    let mut pending_cmd = None;

    match cli.handle_cli(&mut settings).await? {
        cli::CliResult::Return => return Ok(()),
        cli::CliResult::Continue => {}
        cli::CliResult::PendingCommand(cmd) => pending_cmd = Some(cmd),
    }

    if pending_cmd.as_ref().is_some_and(|c| c.is_headless()) {
        let cmd = pending_cmd.take().expect("checked just above");
        return app::run_headless(settings, cmd).await;
    }

    let styles =
        Styles::load(custom_config.as_ref()).context("Error while retrieving app styles")?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    let mut guard = TerminalGuard { terminal };

    chain_panic_hook();

    app::run(&mut guard.terminal, settings, styles, pending_cmd)
        .await
        .inspect_err(|err| {
            log::error!("[PANIC] {err:?}");
        })?;

    Ok(())
}

fn chain_panic_hook() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic| {
        restore_terminal();
        original_hook(panic);
    }));
}
