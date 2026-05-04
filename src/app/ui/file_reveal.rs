use std::path::Path;
use std::process::{Command, Stdio};

pub fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    let mut command = build_command(path);
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    command.spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
fn build_command(path: &Path) -> Command {
    let mut cmd = Command::new("open");
    cmd.arg("-R").arg(path);
    cmd
}

#[cfg(target_os = "windows")]
fn build_command(path: &Path) -> Command {
    let mut cmd = Command::new("explorer");
    cmd.arg(format!("/select,{}", path.display()));
    cmd
}

#[cfg(all(unix, not(target_os = "macos")))]
fn build_command(path: &Path) -> Command {
    let target = path.parent().unwrap_or(path);
    let mut cmd = Command::new("xdg-open");
    cmd.arg(target);
    cmd
}
