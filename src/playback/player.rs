use crate::util::find_binary;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing(String),
    Paused(String),
}

#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play(String),
    Pause,
    Resume,
    Stop,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
}

pub struct PlaybackEngine {
    process: Option<Child>,
    state: PlaybackState,
    ipc_socket: PathBuf,
}

impl Default for PlaybackEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackEngine {
    pub fn new() -> Self {
        let ipc_socket = std::env::temp_dir().join("streamvault-mpv.sock");
        Self {
            process: None,
            state: PlaybackState::Stopped,
            ipc_socket,
        }
    }

    pub async fn play(&mut self, url: &str) -> Result<(), String> {
        self.stop().await;

        let mpv_path = find_binary("mpv");
        let _ = tokio::fs::remove_file(&self.ipc_socket).await;

        let child = Command::new(&mpv_path)
            .arg(url)
            .arg(format!(
                "--input-ipc-server={}",
                self.ipc_socket.display()
            ))
            .arg("--force-window=yes")
            .arg("--keep-open=yes")
            .arg("--hwdec=auto")
            .arg("--sub-auto=fuzzy")
            .arg("--osd-level=1")
            .arg("--title=StreamVault")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start mpv: {e}"))?;

        self.process = Some(child);
        self.state = PlaybackState::Playing(url.to_string());
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        self.state = PlaybackState::Stopped;
    }

    async fn send_ipc(&self, cmd: &str) -> Result<(), String> {
        if !self.ipc_socket.exists() {
            return Err("mpv not running".into());
        }

        let payload = format!("{{ \"command\": [{cmd}] }}\n");

        #[cfg(unix)]
        {
            use tokio::io::AsyncWriteExt;
            use tokio::net::UnixStream;

            let mut stream = UnixStream::connect(&self.ipc_socket)
                .await
                .map_err(|e| format!("IPC connect error: {e}"))?;

            stream
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| format!("IPC write error: {e}"))?;
        }

        Ok(())
    }

    pub async fn handle_command(&mut self, command: PlaybackCommand) -> Result<(), String> {
        match command {
            PlaybackCommand::Play(url) => self.play(&url).await,
            PlaybackCommand::Pause => {
                self.send_ipc("\"set_property\", \"pause\", true").await?;
                if let PlaybackState::Playing(ref url) = self.state {
                    self.state = PlaybackState::Paused(url.clone());
                }
                Ok(())
            }
            PlaybackCommand::Resume => {
                self.send_ipc("\"set_property\", \"pause\", false").await?;
                if let PlaybackState::Paused(ref url) = self.state {
                    self.state = PlaybackState::Playing(url.clone());
                }
                Ok(())
            }
            PlaybackCommand::Stop => {
                self.stop().await;
                Ok(())
            }
            PlaybackCommand::SeekForward => self.send_ipc("\"seek\", 10").await,
            PlaybackCommand::SeekBackward => self.send_ipc("\"seek\", -10").await,
            PlaybackCommand::VolumeUp => self.send_ipc("\"add\", \"volume\", 5").await,
            PlaybackCommand::VolumeDown => self.send_ipc("\"add\", \"volume\", -5").await,
        }
    }

    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.state, PlaybackState::Playing(_))
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.state, PlaybackState::Paused(_))
    }

    pub fn current_url(&self) -> Option<&str> {
        match &self.state {
            PlaybackState::Playing(url) | PlaybackState::Paused(url) => Some(url),
            PlaybackState::Stopped => None,
        }
    }

    pub async fn send_ipc_static(cmd: &str) -> Result<(), String> {
        let socket = std::env::temp_dir().join("streamvault-mpv.sock");
        if !socket.exists() {
            return Err("mpv not running".into());
        }

        let payload = format!("{{ \"command\": [{cmd}] }}\n");

        #[cfg(unix)]
        {
            use tokio::io::AsyncWriteExt;
            use tokio::net::UnixStream;

            let mut stream = UnixStream::connect(&socket)
                .await
                .map_err(|e| format!("IPC connect error: {e}"))?;

            stream
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| format!("IPC write error: {e}"))?;
        }

        Ok(())
    }
}
