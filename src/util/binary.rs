use std::path::PathBuf;

pub fn find_binary(name: &str) -> PathBuf {
    let candidates = [
        PathBuf::from(format!("/opt/homebrew/bin/{name}")),
        PathBuf::from(format!("/usr/local/bin/{name}")),
        PathBuf::from(format!("/usr/bin/{name}")),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".local/bin")
            .join(name),
    ];

    candidates
        .into_iter()
        .find(|p| p.exists())
        .or_else(|| which::which(name).ok())
        .unwrap_or_else(|| PathBuf::from(name))
}
