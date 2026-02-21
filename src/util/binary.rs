use std::path::PathBuf;

pub fn bundled_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    let resources_bin = exe_dir
        .parent()
        .map(|contents| contents.join("Resources").join("bin"));
    if let Some(ref p) = resources_bin {
        if p.is_dir() {
            return Some(p.clone());
        }
    }

    let dev_bin = exe_dir.join("bin");
    if dev_bin.is_dir() {
        return Some(dev_bin);
    }

    None
}

pub fn find_binary(name: &str) -> PathBuf {
    if let Some(bin_dir) = bundled_bin_dir() {
        let bundled = bin_dir.join(name);
        if bundled.exists() {
            return bundled;
        }
    }

    let system_candidates = [
        PathBuf::from(format!("/opt/homebrew/bin/{name}")),
        PathBuf::from(format!("/usr/local/bin/{name}")),
        PathBuf::from(format!("/usr/bin/{name}")),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".local/bin")
            .join(name),
    ];

    system_candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(name))
}
