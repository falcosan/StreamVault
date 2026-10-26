pub const RELEASES: &str = "https://github.com/falcosan/StreamVault/releases";

#[cfg(feature = "desktop")]
pub async fn latest(releases: &str) -> Result<String, String> {
    let release = reqwest::Client::new()
        .head(format!("{releases}/latest"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    tag_version(release.url().path())
        .map(str::to_owned)
        .ok_or_else(|| format!("{} is not a release", release.url()))
}

#[cfg(feature = "desktop")]
pub async fn download(releases: &str, name: &str, to: &std::path::Path) -> Result<(), String> {
    let file = reqwest::get(format!("{releases}/latest/download/{name}"))
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::write(to, file)
        .await
        .map_err(|e| format!("Cannot write {}: {e}", to.display()))
}

#[cfg(feature = "desktop")]
fn tag_version(path: &str) -> Option<&str> {
    path.rsplit_once("/tag/v").map(|(_, version)| version)
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    use crate::check::{scratch_dir, serve};

    #[tokio::test]
    async fn only_a_reachable_release_page_gives_a_version() {
        let (url, requests) = serve(&[(404, "Not Found"), (200, "")]).await;
        assert!(latest(&url).await.unwrap_err().contains("404"));
        assert_eq!(
            latest(&url).await.unwrap_err(),
            format!("{url}/latest is not a release")
        );
        assert!(requests.lock().unwrap()[0].starts_with("HEAD /latest "));
    }

    #[tokio::test]
    async fn files_come_from_the_latest_release() {
        let (url, requests) = serve(&[(404, "Not Found"), (200, "installer")]).await;
        let dir = scratch_dir("release_download");
        let file = dir.join("install");
        assert!(
            download(&url, "StreamVault-install", &file)
                .await
                .unwrap_err()
                .contains("404")
        );
        download(&url, "StreamVault-install", &file).await.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "installer");
        assert!(
            requests.lock().unwrap()[1].starts_with("GET /latest/download/StreamVault-install ")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_version_comes_from_the_release_tag() {
        assert_eq!(
            tag_version("/falcosan/StreamVault/releases/tag/v1.2.3"),
            Some("1.2.3")
        );
        assert_eq!(tag_version("/falcosan/StreamVault/releases"), None);
    }
}
