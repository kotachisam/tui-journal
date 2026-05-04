use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct SaveDialogRequest<'a> {
    pub prompt: &'a str,
    pub default_dir: &'a Path,
    pub default_name: &'a str,
}

pub fn save_file_dialog(req: &SaveDialogRequest<'_>) -> io::Result<Option<PathBuf>> {
    platform_dialog(req)
}

#[cfg(target_os = "macos")]
fn platform_dialog(req: &SaveDialogRequest<'_>) -> io::Result<Option<PathBuf>> {
    let prompt = applescript_escape(req.prompt);
    let name = applescript_escape(req.default_name);
    let dir = applescript_escape(&req.default_dir.display().to_string());

    let script = format!(
        r#"POSIX path of (choose file name with prompt "{prompt}" default name "{name}" default location POSIX file "{dir}")"#
    );

    let output = Command::new("osascript").arg("-e").arg(&script).output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', r#"\""#)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_dialog(req: &SaveDialogRequest<'_>) -> io::Result<Option<PathBuf>> {
    let mut filename = req.default_dir.to_path_buf();
    filename.push(req.default_name);
    let result = Command::new("zenity")
        .arg("--file-selection")
        .arg("--save")
        .arg("--confirm-overwrite")
        .arg("--title")
        .arg(req.prompt)
        .arg(format!("--filename={}", filename.display()))
        .output();
    let output = match result {
        Ok(o) => o,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "zenity not installed; cannot show native save dialog",
            ));
        }
        Err(e) => return Err(e),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

#[cfg(target_os = "windows")]
fn platform_dialog(req: &SaveDialogRequest<'_>) -> io::Result<Option<PathBuf>> {
    let initial_dir = req.default_dir.display().to_string().replace('\'', "''");
    let initial_name = req.default_name.replace('\'', "''");
    let title = req.prompt.replace('\'', "''");

    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.SaveFileDialog; \
         $d.InitialDirectory = '{initial_dir}'; \
         $d.FileName = '{initial_name}'; \
         $d.Title = '{title}'; \
         if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.FileName }}"
    );

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}
