use crate::settings::SYNC_FILE;
use std::path::PathBuf;
use std::sync::OnceLock;

pub fn sync_file() -> PathBuf {
    config_dir().join(SYNC_FILE)
}

pub fn config_dir() -> PathBuf {
    icloud_dir().unwrap_or_else(local_dir)
}

pub fn icloud_dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let drive = dirs::home_dir()?.join("Library/Mobile Documents/com~apple~CloudDocs");
        drive
            .is_dir()
            .then(|| drive.join("Contents/Video/StreamVault"))
    })
    .clone()
}

pub fn local_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("StreamVault")
}
