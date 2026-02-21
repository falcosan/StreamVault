use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Muxing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: DownloadStatus,
    pub percent: f64,
    pub speed: String,
    pub downloaded: String,
    pub total: String,
}

impl DownloadProgress {
    pub fn new(id: uuid::Uuid, title: String) -> Self {
        Self {
            id,
            title,
            status: DownloadStatus::Queued,
            percent: 0.0,
            speed: String::new(),
            downloaded: String::new(),
            total: String::new(),
        }
    }

    pub fn parse_n_m3u8dl_line(&mut self, line: &str) {
        static PERCENT_RE: OnceLock<Regex> = OnceLock::new();
        static SPEED_RE: OnceLock<Regex> = OnceLock::new();
        static SIZE_RE: OnceLock<Regex> = OnceLock::new();

        let percent_re = PERCENT_RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)%").unwrap());
        let speed_re =
            SPEED_RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?(?:MB|KB|GB|B)ps)").unwrap());
        let size_re = SIZE_RE.get_or_init(|| {
            Regex::new(r"(\d+(?:\.\d+)?(?:MB|GB|KB|B))/(\d+(?:\.\d+)?(?:MB|GB|KB|B))").unwrap()
        });

        if let Some(caps) = percent_re.captures(line) {
            if let Ok(p) = caps[1].parse::<f64>() {
                self.percent = p;
            }
        }

        if let Some(caps) = speed_re.captures(line) {
            self.speed = caps[1].to_string();
        }

        if let Some(caps) = size_re.captures(line) {
            self.downloaded = caps[1].to_string();
            self.total = caps[2].to_string();
        }
    }
}
