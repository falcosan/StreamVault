mod desktop;
mod tv;

use crate::release::RELEASES;
use std::cell::RefCell;
use std::env::consts::EXE_SUFFIX;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{BufRead, ErrorKind, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TAR: &str = if cfg!(windows) { "tar" } else { "/usr/bin/tar" };
const LOG_TAIL: usize = 20;
const POLL: Duration = Duration::from_secs(3);
const ATTEMPTS: usize = 3;

pub async fn main() -> ExitCode {
    let result = match std::env::args().nth(1).as_deref() {
        Some("desktop") => desktop::deploy().await,
        Some("tv") => tv::deploy().await,
        _ => Err("Usage: install desktop|tv".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("\n  {error}");
            ExitCode::FAILURE
        }
    }
}

fn say(message: impl std::fmt::Display) {
    println!("  {message}");
}

fn ask(question: &str) -> Option<String> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return None;
    }
    print!("  {question} ");
    std::io::stdout().flush().ok()?;
    let mut answer = String::new();
    (stdin.lock().read_line(&mut answer).ok()? > 0).then(|| answer.trim().to_owned())
}

async fn wait_until<T, F: Future<Output = Option<T>>>(
    message: &str,
    mut ready: impl FnMut() -> F,
) -> T {
    if let Some(value) = ready().await {
        return value;
    }
    say(message);
    loop {
        tokio::time::sleep(POLL).await;
        if let Some(value) = ready().await {
            return value;
        }
    }
}

fn binary(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{EXE_SUFFIX}"))
}

async fn attempt(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

fn tidy(program: &Path, args: &[&str]) {
    let done = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !done {
        eprintln!("  Cleanup failed: {} {}", program.display(), args.join(" "));
    }
}

fn remove(path: &Path) {
    if let Err(e) = fs::remove_dir_all(path)
        && e.kind() != ErrorKind::NotFound
    {
        eprintln!("  Cannot remove {}: {e}", path.display());
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|text| text.replace("\r\n", "\n"))
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    parent(path)?;
    fs::write(path, contents).map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    parent(to)?;
    fs::copy(from, to)
        .map(drop)
        .map_err(|e| format!("Cannot copy {} to {}: {e}", from.display(), to.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from).map_err(|e| format!("Cannot read {}: {e}", from.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            copy(&path, &target)?;
        }
    }
    Ok(())
}

fn find(dir: &Path, matches: &impl Fn(&str) -> bool) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        if path.is_dir() {
            find(&path, matches)
        } else {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(matches)
                .then_some(path)
        }
    })
}

fn parent(path: &Path) -> Result<(), String> {
    path.parent().map_or(Ok(()), |dir| {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))
    })
}

fn describe(command: &Command) -> String {
    let command = command.as_std();
    let program = Path::new(command.get_program());
    let name = program.file_name().unwrap_or(program.as_os_str());
    let action = command.get_args().next().unwrap_or_default();
    format!("{} {}", name.to_string_lossy(), action.to_string_lossy())
}

fn asset(name: &str) -> String {
    format!("{RELEASES}/download/v{VERSION}/{name}")
}

struct Session {
    source: Option<PathBuf>,
    out: PathBuf,
    scratch: PathBuf,
    http: reqwest::Client,
    log: PathBuf,
    cleanup: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Session {
    fn new(device: &str) -> Result<Self, String> {
        let source = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
        let scratch = std::env::temp_dir().join(format!("streamvault-{device}"));
        let out = source.as_ref().map_or_else(
            || scratch.join("out"),
            |root| root.join("target").join(device),
        );
        let log = std::env::temp_dir().join(format!("streamvault-{device}.log"));
        for dir in [&scratch, &out] {
            remove(dir);
            fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        }
        File::create(&log).map_err(|e| format!("Cannot write {}: {e}", log.display()))?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            log,
            source,
            out,
            scratch,
            http,
            cleanup: RefCell::default(),
        })
    }

    fn on_exit(&self, undo: impl FnOnce() + 'static) {
        self.cleanup.borrow_mut().push(Box::new(undo));
    }

    async fn run(&self, command: &mut Command) -> Result<(), String> {
        let log = self.log_file()?;
        let output = log.try_clone().map_err(|e| e.to_string())?;
        let status = command
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(log)
            .status()
            .await
            .map_err(|e| format!("Cannot run {}: {e}", describe(command)))?;
        self.check(command, status.success())
    }

    async fn output(&self, command: &mut Command) -> Result<String, String> {
        self.feed(command, "").await
    }

    async fn feed(&self, command: &mut Command, input: &str) -> Result<String, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(self.log_file()?)
            .spawn()
            .map_err(|e| format!("Cannot run {}: {e}", describe(command)))?;
        if let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(input.as_bytes()).await
            && e.kind() != ErrorKind::BrokenPipe
        {
            return Err(format!("Cannot answer {}: {e}", describe(command)));
        }
        let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).replace('\r', "");
        self.append(&stdout)?;
        self.check(command, output.status.success())
            .map(|()| stdout)
    }

    async fn transcript(&self, command: &mut Command) -> Result<String, String> {
        let output = command
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("Cannot run {}: {e}", describe(command)))?;
        let text = [output.stdout, output.stderr]
            .map(|bytes| String::from_utf8_lossy(&bytes).replace('\r', ""))
            .concat();
        self.append(&text)?;
        Ok(text)
    }

    async fn download(&self, url: &str, name: &str) -> Result<PathBuf, String> {
        let path = self.scratch.join(name);
        let mut failure = String::new();
        for _ in 0..ATTEMPTS {
            match self.fetch(url, &path).await {
                Ok(()) => return Ok(path),
                Err(e) => failure = e,
            }
        }
        Err(format!("Cannot download {url}: {failure}"))
    }

    async fn fetch(&self, url: &str, path: &Path) -> Result<(), String> {
        parent(path)?;
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| e.to_string())?;
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| e.to_string())?;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        file.flush().await.map_err(|e| e.to_string())
    }

    async fn unpack(&self, archive: &Path, dir: &Path, members: &[&str]) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        self.run(
            Command::new(TAR)
                .arg("-xf")
                .arg(archive)
                .arg("-C")
                .arg(dir)
                .args(members),
        )
        .await
    }

    fn log_file(&self) -> Result<File, String> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|e| format!("Cannot write {}: {e}", self.log.display()))
    }

    fn append(&self, text: &str) -> Result<(), String> {
        self.log_file()?
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())
    }

    fn check(&self, command: &Command, success: bool) -> Result<(), String> {
        if success {
            return Ok(());
        }
        let log = fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        Err(format!(
            "{}\n\n  Failed: {} (log: {})",
            lines[lines.len().saturating_sub(LOG_TAIL)..].join("\n"),
            describe(command),
            self.log.display()
        ))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for undo in self.cleanup.take().into_iter().rev() {
            undo();
        }
        remove(&self.scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;

    #[test]
    fn files_are_found_anywhere_under_a_directory() {
        let dir = scratch_dir("deploy_find");
        write(&dir.join("a/b/org.tizen.common.cert_1.0.jar"), "").unwrap();
        write(&dir.join("a/other.jar"), "").unwrap();
        let found = find(&dir, &|name| name.starts_with("org.tizen.common.cert_"));
        assert_eq!(found, Some(dir.join("a/b/org.tizen.common.cert_1.0.jar")));
        assert_eq!(find(&dir, &|name| name == "missing"), None);
        remove(&dir);
    }

    #[test]
    fn directories_copy_with_their_contents() {
        let dir = scratch_dir("deploy_copy");
        write(&dir.join("from/assets/app.js"), "js").unwrap();
        write(&dir.join("from/index.html"), "html").unwrap();
        copy_dir(&dir.join("from"), &dir.join("to")).unwrap();
        assert_eq!(read(&dir.join("to/assets/app.js")).unwrap(), "js");
        assert_eq!(read(&dir.join("to/index.html")).unwrap(), "html");
        remove(&dir);
    }

    #[test]
    fn text_reads_with_unix_line_ends() {
        let dir = scratch_dir("deploy_read");
        fs::write(dir.join("text"), "a\r\nb\n").unwrap();
        assert_eq!(read(&dir.join("text")).unwrap(), "a\nb\n");
        remove(&dir);
    }
}
