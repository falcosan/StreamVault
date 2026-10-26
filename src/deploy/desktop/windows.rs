use crate::deploy::{Session, VERSION, copy, say};
use std::path::Path;
use tokio::process::Command;

pub const APP: &str = "StreamVault-windows-x64.exe";
pub const FFMPEG: &str =
    "https://github.com/GyanD/codexffmpeg/releases/download/7.1/ffmpeg-7.1-essentials_build.zip";
pub const DOWNLOADER: &str = "win-x64";
pub const DOWNLOADER_ARCHIVE: &str = "zip";
const WEBVIEW: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
const RUNTIME: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\StreamVault";
const EXE: &str = "StreamVault.exe";

pub async fn install(session: &Session) -> Result<(), String> {
    webview(session).await?;
    let app = dirs::data_local_dir()
        .ok_or("Windows has no local app data folder.")?
        .join(r"Programs\StreamVault");
    let exe = app.join(EXE);
    replace(&super::app(session).await?, &exe)?;
    for tool in super::tools(session).await? {
        replace(
            &tool,
            &app.join("bin").join(tool.file_name().unwrap_or_default()),
        )?;
    }
    let shortcut = dirs::data_dir()
        .ok_or("Windows has no app data folder.")?
        .join(r"Microsoft\Windows\Start Menu\Programs\StreamVault.lnk");
    let link = format!(
        "$link = (New-Object -ComObject WScript.Shell).CreateShortcut({}); $link.TargetPath = {}; $link.WorkingDirectory = {}; $link.Save()",
        quote(shortcut.display()),
        quote(exe.display()),
        quote(app.display())
    );
    session
        .run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &link]))
        .await?;
    let uninstall = format!(
        "powershell.exe -NoProfile -WindowStyle Hidden -Command \"Get-Process | Where-Object Path -Like {} | Stop-Process -Force; Remove-Item -LiteralPath {}, {}, {} -Recurse -Force\"",
        quote(app.join("*").display()),
        quote(app.display()),
        quote(shortcut.display()),
        quote(format_args!(r"HKCU:\{UNINSTALL}"))
    );
    let key = format!(r"HKCU\{UNINSTALL}");
    let exe = exe.display().to_string();
    let app = app.display().to_string();
    for (name, kind, value) in [
        ("DisplayName", "REG_SZ", "StreamVault"),
        ("DisplayVersion", "REG_SZ", VERSION),
        ("DisplayIcon", "REG_SZ", exe.as_str()),
        ("InstallLocation", "REG_SZ", app.as_str()),
        ("UninstallString", "REG_SZ", uninstall.as_str()),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ] {
        session
            .run(Command::new("reg").args(["add", &key, "/v", name, "/t", kind, "/d", value, "/f"]))
            .await?;
    }
    say(format!(
        "StreamVault {VERSION} installed: open it from the Start menu."
    ));
    Ok(())
}

async fn webview(session: &Session) -> Result<(), String> {
    for root in [r"HKLM\SOFTWARE\WOW6432Node", r"HKCU\Software"] {
        let query = Command::new("reg")
            .args(["query", &format!(r"{root}\{RUNTIME}"), "/v", "pv"])
            .output()
            .await;
        if query.is_ok_and(|output| {
            output.status.success() && installed(&String::from_utf8_lossy(&output.stdout))
        }) {
            return Ok(());
        }
    }
    say("Installing the Microsoft Edge WebView2 Runtime…");
    let setup = session
        .download(WEBVIEW, "MicrosoftEdgeWebview2Setup.exe")
        .await?;
    session
        .run(Command::new(setup).args(["/silent", "/install"]))
        .await
}

fn installed(query: &str) -> bool {
    query
        .lines()
        .filter_map(|line| line.split_once("REG_SZ"))
        .map(|(_, version)| version.trim())
        .any(|version| !version.is_empty() && version != "0.0.0.0")
}

fn replace(file: &Path, path: &Path) -> Result<(), String> {
    if path.exists() {
        let old = path.with_added_extension("old");
        std::fs::rename(path, &old)
            .map_err(|e| format!("Cannot move {} aside: {e}", path.display()))?;
    }
    copy(file, path)
}

fn quote(text: impl std::fmt::Display) -> String {
    format!("'{}'", text.to_string().replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;

    #[test]
    fn only_a_registered_runtime_version_counts() {
        let query = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}\r\n    pv    REG_SZ    131.0.2903.70\r\n";
        assert!(installed(query));
        assert!(!installed(&query.replace("131.0.2903.70", "0.0.0.0")));
        assert!(!installed(&query.replace("131.0.2903.70", "")));
        assert!(!installed(""));
    }

    #[test]
    fn a_running_file_is_moved_aside_before_the_new_one_lands() {
        let dir = scratch_dir("replace");
        let (new, path) = (dir.join("new.exe"), dir.join(r"app\StreamVault.exe"));
        std::fs::write(&new, "new").unwrap();
        replace(&new, &path).unwrap();
        std::fs::write(&path, "old").unwrap();
        replace(&new, &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(dir.join(r"app\StreamVault.exe.old")).unwrap(),
            "old"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quotes_survive_in_powershell_strings() {
        assert_eq!(quote(r"C:\Users\O'Neil"), r"'C:\Users\O''Neil'");
    }
}
