use super::config::AppConfig;
use crate::app::Subtitle;
use crate::providers::{Episode, MediaEntry, StreamUrl};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const ERROR_LINES: usize = 5;
const SAVE_PATTERN: &str = "<SaveName>.<Language>.<Id>";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Queued,
    Downloading(u8),
    Muxing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Download {
    pub id: Uuid,
    pub title: String,
    pub file: PathBuf,
    pub entry: MediaEntry,
    pub season: Option<u32>,
    pub episode: Option<Episode>,
    pub status: Status,
}

impl Download {
    pub fn new(
        config: &AppConfig,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> Self {
        let (title, file) = target(config, entry, season, episode);
        Self {
            id: Uuid::new_v4(),
            title,
            file,
            entry: entry.clone(),
            season,
            episode: episode.cloned(),
            status: Status::Queued,
        }
    }

    pub fn restored(self) -> Option<Self> {
        match self.status {
            Status::Completed => self.file.is_file().then_some(self),
            Status::Failed(_) => None,
            _ => Some(Self {
                status: Status::Queued,
                ..self
            }),
        }
    }
}

pub fn local_file(
    config: &AppConfig,
    entry: &MediaEntry,
    season: Option<u32>,
    episode: Option<&Episode>,
) -> Option<PathBuf> {
    let (_, file) = target(config, entry, season, episode);
    file.is_file().then_some(file)
}

pub fn subtitles(video: &Path) -> Vec<Subtitle> {
    let (Some(dir), Some(name)) = (video.parent(), video.file_stem().and_then(OsStr::to_str))
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut subtitles: Vec<(u32, Subtitle)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|file| file.extension().is_some_and(|ext| ext == "vtt"))
        .filter_map(|file| {
            let (language, id) = sidecar(name, &file)?;
            Some((id, Subtitle { file, language }))
        })
        .collect();
    subtitles.sort_by_key(|(id, _)| *id);
    subtitles
        .into_iter()
        .map(|(_, subtitle)| subtitle)
        .collect()
}

fn sidecar(name: &str, file: &Path) -> Option<(String, u32)> {
    let tags = file
        .file_stem()?
        .to_str()?
        .strip_prefix(name)?
        .strip_prefix('.')?;
    let (language, id) = tags.rsplit_once('.').unwrap_or(("", tags));
    Some((language.to_owned(), id.parse().ok()?))
}

fn target(
    config: &AppConfig,
    entry: &MediaEntry,
    season: Option<u32>,
    episode: Option<&Episode>,
) -> (String, PathBuf) {
    let (title, dir) = if let (Some(season), Some(episode)) = (season, episode) {
        let title = config
            .output
            .map_episode_name
            .replace("%(episode_name)", &episode.name)
            .replace("%(season)", &format!("{season:02}"))
            .replace("%(episode)", &format!("{:02}", episode.number))
            .replace("%(show_name)", &entry.name);
        let dir = config
            .serie_dir()
            .join(sanitize_filename(&entry.name))
            .join(format!("S{season:02}"));
        (title, dir)
    } else {
        let title = entry.display_title();
        let dir = config.movie_dir().join(sanitize_filename(&title));
        (title, dir)
    };
    let file = dir.join(format!(
        "{}.{}",
        sanitize_filename(&title),
        config.process.extension
    ));
    (title, file)
}

pub async fn run(
    config: &AppConfig,
    download: &Download,
    stream: &StreamUrl,
    mut report: impl FnMut(Status),
) {
    report(Status::Downloading(0));
    let work = work_dir(download.id);
    let result = fetch(config, download, stream, &work, &mut report).await;
    let _ = tokio::fs::remove_dir_all(&work).await;
    report(match result {
        Ok(()) => Status::Completed,
        Err(e) => Status::Failed(e),
    });
}

async fn fetch(
    config: &AppConfig,
    download: &Download,
    stream: &StreamUrl,
    work: &Path,
    report: &mut impl FnMut(Status),
) -> Result<(), String> {
    let (Some(dir), Some(name), Some(file)) = (
        download.file.parent(),
        download.file.file_stem(),
        download.file.file_name(),
    ) else {
        return Err(format!("Invalid output path: {}", download.file.display()));
    };
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("Cannot create output dir: {e}"))?;
    let tmp = work.join("tmp");
    tokio::fs::create_dir_all(&tmp)
        .await
        .map_err(|e| format!("Cannot create tmp dir: {e}"))?;
    let tracks = work.join("tracks");
    reset_dir(&tracks)
        .await
        .map_err(|e| format!("Cannot create tracks dir: {e}"))?;
    let ffmpeg = find_binary("ffmpeg");
    let mut child = downloader(config, stream, name, &tracks, &tmp, &ffmpeg)
        .spawn()
        .map_err(|e| format!("Failed to start N_m3u8DL-RE: {e}"))?;

    let mut muxing = false;
    let mut progress = Progress::default();
    let mut errors = Vec::new();
    tokio::join!(
        each_line(child.stdout.take(), |line| {
            if muxing {
                return;
            }
            if ["Muxing", "muxing", "MUX"].iter().any(|m| line.contains(m)) {
                muxing = true;
                report(Status::Muxing);
            } else if let Some(percent) = progress.update(line) {
                report(Status::Downloading(percent));
            }
        }),
        each_line(child.stderr.take(), |line| {
            if errors.len() < ERROR_LINES && is_meaningful(line) {
                errors.push(line.to_owned());
            }
        }),
    );

    let exit = child
        .wait()
        .await
        .map_err(|e| format!("Process error: {e}"))?;
    let (media, subtitles) = scan_tracks(&tracks, config.process.merge_subtitle).await;
    if !exit.success() && media.is_empty() {
        return Err(exit_message(exit, &errors));
    }
    if !muxing {
        report(Status::Muxing);
    }
    let muxed = work.join(file);
    mux(&ffmpeg, &muxed, &media)
        .await
        .map_err(|e| format!("Mux failed: {e}"))?;
    save_subtitles(&subtitles, &download.file)
        .await
        .map_err(|e| format!("Cannot save subtitles: {e}"))?;
    move_file(&muxed, &download.file)
        .await
        .map_err(|e| format!("Cannot save output: {e}"))
}

#[derive(Default)]
struct Progress {
    streams: HashMap<String, (u64, u64)>,
    percent: u8,
}

impl Progress {
    fn update(&mut self, line: &str) -> Option<u8> {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let (index, segments) = tokens.iter().enumerate().find_map(|(index, token)| {
            let (done, total) = token.split_once('/')?;
            let segments = (done.parse().ok()?, total.parse().ok()?);
            let percent = tokens
                .iter()
                .skip(index + 1)
                .take(2)
                .any(|t| t.contains('%'));
            percent.then_some((index, segments))
        })?;
        let stream = tokens[..index]
            .iter()
            .filter(|token| token.chars().any(char::is_alphanumeric))
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        self.streams.insert(stream, segments);
        let (done, total) = self
            .streams
            .values()
            .fold((0, 0), |(done, total), (d, t)| (done + d, total + t));
        let percent = u8::try_from((done * 100 / total.max(1)).min(100)).ok()?;
        (percent != self.percent).then(|| {
            self.percent = percent;
            percent
        })
    }
}

async fn each_line(source: Option<impl AsyncRead + Unpin>, mut handle: impl FnMut(&str)) {
    let Some(source) = source else { return };
    let mut reader = BufReader::new(source);
    let mut buffer = Vec::new();
    while reader
        .read_until(b'\n', &mut buffer)
        .await
        .is_ok_and(|read| read > 0)
    {
        for line in buffer.split(|b| matches!(b, b'\r' | b'\n')) {
            handle(&String::from_utf8_lossy(line));
        }
        buffer.clear();
    }
}

fn downloader(
    config: &AppConfig,
    stream: &StreamUrl,
    name: &OsStr,
    tracks: &Path,
    tmp: &Path,
    ffmpeg: &Path,
) -> Command {
    let settings = &config.download;
    let mut cmd = Command::new(find_binary("N_m3u8DL-RE"));
    cmd.kill_on_drop(true)
        .arg(&stream.url)
        .arg("--save-name")
        .arg(name)
        .arg("--save-pattern")
        .arg(SAVE_PATTERN)
        .arg("--save-dir")
        .arg(tracks)
        .arg("--tmp-dir")
        .arg(tmp)
        .arg("--ffmpeg-binary-path")
        .arg(ffmpeg)
        .args([
            "--no-log",
            "--write-meta-json",
            "false",
            "--binary-merge",
            "--del-after-done",
            "--auto-subtitle-fix",
            "false",
            "--check-segments-count",
            "true",
            "--force-ansi-console",
            "--no-ansi-color",
        ]);
    if settings.concurrent_download {
        cmd.arg("--concurrent-download");
    }
    cmd.arg("--thread-count")
        .arg(settings.thread_count.to_string())
        .arg("--download-retry-count")
        .arg(settings.retry_count.to_string());
    if !settings.max_speed.is_empty() {
        cmd.arg("--max-speed").arg(&settings.max_speed);
    }
    cmd.arg("--select-video")
        .arg(&settings.select_video)
        .arg("--select-audio")
        .arg(&settings.select_audio)
        .arg("--select-subtitle")
        .arg(&settings.select_subtitle);
    if !config.process.merge_audio {
        cmd.args(["--drop-audio", "all"]);
    }
    for (key, value) in &stream.headers {
        cmd.arg("--header").arg(format!("{key}: {value}"));
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

async fn mux(ffmpeg: &Path, output: &Path, media: &[PathBuf]) -> Result<(), String> {
    if media.is_empty() {
        return Err("No .ts files found after download".into());
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.kill_on_drop(true).arg("-y");
    for input in media {
        cmd.arg("-i").arg(input);
    }
    for i in 0..media.len() {
        cmd.arg("-map")
            .arg(format!("{i}:v?"))
            .arg("-map")
            .arg(format!("{i}:a?"));
    }
    cmd.args(["-c:v", "copy", "-c:a", "copy"]);
    let result = cmd
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&result.stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with("frame="))
        .collect();
    let tail = lines[lines.len().saturating_sub(ERROR_LINES)..].join("\n");
    Err(format!("ffmpeg exited: {}\n{tail}", result.status))
}

fn work_dir(id: Uuid) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("StreamVault")
        .join(id.to_string())
}

async fn reset_dir(dir: &Path) -> std::io::Result<()> {
    if let Err(e) = tokio::fs::remove_dir_all(dir).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e);
    }
    tokio::fs::create_dir_all(dir).await
}

async fn scan_tracks(dir: &Path, with_subtitles: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut media = Vec::new();
    let mut subtitles = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            match path.extension().and_then(|x| x.to_str()) {
                Some("ts") => {
                    let size = entry.metadata().await.map_or(0, |m| m.len());
                    media.push((path, size));
                }
                Some("vtt") if with_subtitles => subtitles.push(path),
                _ => {}
            }
        }
    }
    media.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    (media.into_iter().map(|(path, _)| path).collect(), subtitles)
}

pub async fn delete(video: &Path) -> std::io::Result<()> {
    if let Err(e) = tokio::fs::remove_file(video).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e);
    }
    remove_subtitles(video).await
}

async fn remove_subtitles(video: &Path) -> std::io::Result<()> {
    for subtitle in subtitles(video) {
        tokio::fs::remove_file(subtitle.file).await?;
    }
    Ok(())
}

async fn save_subtitles(files: &[PathBuf], video: &Path) -> std::io::Result<()> {
    remove_subtitles(video).await?;
    for file in files {
        if let Some(name) = file.file_name() {
            move_file(file, &video.with_file_name(name)).await?;
        }
    }
    Ok(())
}

async fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if tokio::fs::rename(from, to).await.is_ok() {
        return Ok(());
    }
    copy_atomically(from, to).await
}

async fn copy_atomically(from: &Path, to: &Path) -> std::io::Result<()> {
    let partial = to.with_extension("part");
    tokio::fs::copy(from, &partial).await?;
    tokio::fs::rename(&partial, to).await
}

fn is_meaningful(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && !line.starts_with("at ") && !line.starts_with("---")
}

fn exit_message(exit: ExitStatus, errors: &[String]) -> String {
    let summary = format!("N_m3u8DL-RE exited: {exit}");
    if errors.is_empty() {
        summary
    } else {
        format!("{summary}\n{}", errors.join("\n"))
    }
}

fn find_binary(name: &str) -> PathBuf {
    let bundled = bundled_bin_dir().map(|dir| dir.join(name));
    let home = dirs::home_dir()
        .unwrap_or_default()
        .join(".local/bin")
        .join(name);
    bundled
        .into_iter()
        .chain(
            ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
                .map(|dir| Path::new(dir).join(name)),
        )
        .chain([home])
        .find(|p| is_executable(p))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn bundled_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.parent()
        .map(|contents| contents.join("Resources/bin"))
        .into_iter()
        .chain([dir.join("bin")])
        .find(|p| p.is_dir())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn sanitize_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if clean.trim_matches('.').is_empty() {
        "_".into()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::test_support::scratch_dir;
    use std::os::unix::process::ExitStatusExt;

    fn episode(number: u32, name: &str) -> Episode {
        Episode {
            id: 1,
            number,
            name: name.into(),
            duration: None,
            image_url: None,
            description: None,
        }
    }

    #[test]
    fn movie_download_lives_in_its_own_folder() {
        let entry = MediaEntry {
            name: "Inception".into(),
            year: Some("2010".into()),
            ..Default::default()
        };
        let download = Download::new(&AppConfig::default(), &entry, None, None);
        assert_eq!(download.title, "Inception (2010)");
        assert!(
            download
                .file
                .ends_with("Medias/Movie/Inception (2010)/Inception (2010).mp4")
        );
        assert_eq!(download.entry, entry);
        assert_eq!((download.season, download.episode), (None, None));
        assert_eq!(download.status, Status::Queued);
    }

    #[test]
    fn episode_download_uses_name_template_and_season_folder() {
        let mut config = AppConfig::default();
        let show = MediaEntry {
            name: "Breaking/Bad".into(),
            ..Default::default()
        };
        let download = Download::new(&config, &show, Some(2), Some(&episode(5, "Mandala")));
        assert_eq!(download.title, "Mandala S02E05");
        assert!(
            download
                .file
                .ends_with("Medias/Serie/Breaking_Bad/S02/Mandala S02E05.mp4")
        );
        assert_eq!(download.season, Some(2));
        assert_eq!(download.episode, Some(episode(5, "Mandala")));
        config.output.map_episode_name =
            "%(show_name) - %(episode_name) S%(season)E%(episode)".into();
        config.process.extension = "mkv".into();
        let lost = MediaEntry {
            name: "Lost".into(),
            ..Default::default()
        };
        let named = Download::new(&config, &lost, Some(1), Some(&episode(3, "Tabula Rasa")));
        assert_eq!(named.title, "Lost - Tabula Rasa S01E03");
        assert!(named.file.ends_with("Lost - Tabula Rasa S01E03.mkv"));
    }

    #[test]
    fn downloads_get_unique_ids() {
        let entry = MediaEntry::default();
        let config = AppConfig::default();
        assert_ne!(
            Download::new(&config, &entry, None, None).id,
            Download::new(&config, &entry, None, None).id
        );
    }

    #[test]
    fn restored_downloads_keep_saved_files_and_requeue_unfinished_ones() {
        let dir = scratch_dir("restored");
        let saved = Download {
            file: dir.join("saved.mp4"),
            ..Download::new(&AppConfig::default(), &MediaEntry::default(), None, None)
        };
        std::fs::write(&saved.file, "video").unwrap();
        let with = |status| Download {
            status,
            ..saved.clone()
        };
        assert_eq!(
            with(Status::Completed).restored(),
            Some(with(Status::Completed))
        );
        for status in [Status::Queued, Status::Downloading(40), Status::Muxing] {
            assert_eq!(with(status).restored(), Some(with(Status::Queued)));
        }
        assert_eq!(with(Status::Failed("boom".into())).restored(), None);
        let deleted = Download {
            file: dir.join("deleted.mp4"),
            ..with(Status::Completed)
        };
        assert_eq!(deleted.restored(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_round_trips_through_json() {
        let show = MediaEntry {
            name: "Lost".into(),
            ..Default::default()
        };
        let download = Download {
            status: Status::Downloading(40),
            ..Download::new(
                &AppConfig::default(),
                &show,
                Some(1),
                Some(&episode(2, "Pilot")),
            )
        };
        let json = serde_json::to_string(&download).unwrap();
        assert_eq!(serde_json::from_str::<Download>(&json).unwrap(), download);
    }

    #[test]
    fn local_file_finds_only_existing_downloads() {
        let root = scratch_dir("local_file");
        let mut config = AppConfig::default();
        config.output.root_path = root.to_string_lossy().into_owned();
        let show = MediaEntry {
            name: "Lost".into(),
            ..Default::default()
        };
        let pilot = episode(1, "Pilot");
        assert_eq!(local_file(&config, &show, Some(1), Some(&pilot)), None);
        let download = Download::new(&config, &show, Some(1), Some(&pilot));
        std::fs::create_dir_all(download.file.parent().unwrap()).unwrap();
        std::fs::write(&download.file, "video").unwrap();
        assert_eq!(
            local_file(&config, &show, Some(1), Some(&pilot)),
            Some(download.file)
        );
        assert_eq!(local_file(&config, &show, None, None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sanitize_replaces_reserved_and_control_chars() {
        assert_eq!(
            sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_filename("line\nbreak\0"), "line_break_");
        assert_eq!(sanitize_filename("café 日本語"), "café 日本語");
    }

    #[test]
    fn sanitize_never_yields_a_traversal_name() {
        assert_eq!(sanitize_filename(".."), "_");
        assert_eq!(sanitize_filename("."), "_");
        assert_eq!(sanitize_filename(""), "_");
        assert_eq!(sanitize_filename("..a"), "..a");
    }

    #[test]
    fn find_binary_falls_back_to_bare_name() {
        assert_eq!(
            find_binary("not_a_real_binary_xyz"),
            PathBuf::from("not_a_real_binary_xyz")
        );
    }

    #[test]
    fn work_dir_is_unique_and_outside_output_tree() {
        let id = Uuid::new_v4();
        let dir = work_dir(id);
        assert!(dir.ends_with(format!("StreamVault/{id}")));
        assert!(!dir.starts_with(AppConfig::default().movie_dir()));
        assert_ne!(dir, work_dir(Uuid::new_v4()));
    }

    #[test]
    fn exit_message_appends_collected_errors() {
        let exit = ExitStatus::from_raw(256);
        assert_eq!(
            exit_message(exit, &["boom".into(), "l2".into()]),
            format!("N_m3u8DL-RE exited: {exit}\nboom\nl2")
        );
        assert_eq!(
            exit_message(exit, &[]),
            format!("N_m3u8DL-RE exited: {exit}")
        );
    }

    #[test]
    fn progress_reads_segment_counts_from_downloader_lines() {
        let mut progress = Progress::default();
        assert_eq!(
            progress.update("23:52:29 INFO : Start downloading..."),
            None
        );
        assert_eq!(
            progress.update("Vid Kbps ━━━━━━━━━━ 19/40 47.50% 725.94KB/3.15MB 725.94KBps 00:00:00"),
            Some(47)
        );
        assert_eq!(
            progress.update("Vid Kbps ━━━━━━━━━━ 19/40 47.50% 1.52MB/3.20MB 830.95KBps 00:00:00"),
            None
        );
        assert_eq!(
            progress.update(
                "Vid Kbps ━━━━━━━━━━ 40/40 100.00% 2.68MB - 00:00:00   23:52:32 INFO : Done"
            ),
            Some(100)
        );
    }

    #[test]
    fn progress_does_not_depend_on_bar_glyphs() {
        let mut progress = Progress::default();
        assert_eq!(
            progress.update("Vid 1080p | 4 Mbps ----- 12/48 25.00% 1MB/4MB"),
            Some(25)
        );
        assert_eq!(progress.update("Vid 1080p | 4 Mbps 24/48 50.00%"), Some(50));
        assert_eq!(progress.update("INFO : Vid | 48 Segments | 2/3 done"), None);
    }

    #[test]
    fn progress_accepts_localized_percentages() {
        let mut progress = Progress::default();
        assert_eq!(progress.update("Vid ━━━ 12/48 25,00 % 1MB/4MB"), Some(25));
        assert_eq!(progress.update("Vid ━━━ 36/48 75,00% 3MB/4MB"), Some(75));
    }

    #[test]
    fn progress_skips_the_initial_zero() {
        let mut progress = Progress::default();
        assert_eq!(progress.update("Vid ━━━ 0/40 0.00%"), None);
        assert_eq!(progress.update("Vid ━━━ 4/40 10.00%"), Some(10));
    }

    #[test]
    fn progress_aggregates_all_streams() {
        let mut progress = Progress::default();
        assert_eq!(progress.update("Vid 1080p ━━━ 50/100 50.00%"), Some(50));
        assert_eq!(progress.update("Aud ita ━━━ 0/100 0.00%"), Some(25));
        assert_eq!(progress.update("Aud ita ━━━ 100/100 100.00%"), Some(75));
    }

    #[tokio::test]
    async fn each_line_splits_on_any_newline_and_survives_invalid_utf8() {
        let mut lines = Vec::new();
        let source: &[u8] = b"a\rb\nc\xff\nd";
        each_line(Some(source), |line| lines.push(line.to_owned())).await;
        lines.retain(|line| !line.is_empty());
        assert_eq!(lines, ["a", "b", "c\u{fffd}", "d"]);
    }

    #[test]
    fn subtitles_are_the_video_sidecars_in_source_order() {
        let dir = scratch_dir("subtitles");
        let video = dir.join("Dr. House S01E01.mp4");
        let sidecar = |language: &str, id: &str| {
            let name = SAVE_PATTERN
                .replace("<SaveName>", "Dr. House S01E01")
                .replace("<Language>", language)
                .replace("<Id>", id)
                .replace("..", ".");
            dir.join(format!("{name}.vtt"))
        };
        for file in [
            sidecar("ita", "11"),
            sidecar("forced-ita", "10"),
            sidecar("", "2"),
            sidecar("ita", "x"),
            dir.join("Other.ita.1.vtt"),
            dir.join("Dr. House S01E01.ita.3.srt"),
        ] {
            std::fs::write(file, "WEBVTT").unwrap();
        }
        let found: Vec<_> = subtitles(&video)
            .into_iter()
            .map(|s| (s.file, s.language))
            .collect();
        assert_eq!(
            found,
            [
                (sidecar("", "2"), String::new()),
                (sidecar("forced-ita", "10"), "forced-ita".into()),
                (sidecar("ita", "11"), "ita".into()),
            ]
        );
        assert_eq!(subtitles(&dir.join("missing/a.mp4")), []);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn save_subtitles_replaces_previous_sidecars() {
        let dir = scratch_dir("save_subtitles");
        let (work, out) = (dir.join("work"), dir.join("out"));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let video = out.join("Ep.mp4");
        std::fs::write(out.join("Ep.fre.4.vtt"), "old").unwrap();
        std::fs::write(work.join("Ep.ita.3.vtt"), "new").unwrap();
        save_subtitles(&[work.join("Ep.ita.3.vtt")], &video)
            .await
            .unwrap();
        assert_eq!(
            subtitles(&video),
            [Subtitle {
                file: out.join("Ep.ita.3.vtt"),
                language: "ita".into()
            }]
        );
        assert_eq!(
            std::fs::read_to_string(out.join("Ep.ita.3.vtt")).unwrap(),
            "new"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_removes_the_video_and_only_its_subtitles() {
        let dir = scratch_dir("delete");
        let video = dir.join("Ep.mp4");
        for file in [
            "Ep.mp4",
            "Ep.ita.3.vtt",
            "Ep.forced-ita.2.vtt",
            "Other.mp4",
            "Other.ita.1.vtt",
        ] {
            std::fs::write(dir.join(file), "x").unwrap();
        }
        delete(&video).await.unwrap();
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        left.sort();
        assert_eq!(left, ["Other.ita.1.vtt", "Other.mp4"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_of_a_missing_video_succeeds() {
        let dir = scratch_dir("delete_missing");
        std::fs::write(dir.join("Ep.ita.3.vtt"), "x").unwrap();
        delete(&dir.join("Ep.mp4")).await.unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_meaningful_skips_stack_frames_and_rules() {
        assert!(is_meaningful("boom"));
        assert!(!is_meaningful("   at Foo.Bar()"));
        assert!(!is_meaningful("---"));
        assert!(!is_meaningful("  "));
    }

    #[tokio::test]
    async fn scan_tracks_orders_media_by_size_and_filters_subtitles() {
        let dir = scratch_dir("scan_tracks");
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        std::fs::write(dir.join("small.ts"), [0u8; 1]).unwrap();
        std::fs::write(dir.join("large.ts"), [0u8; 8]).unwrap();
        std::fs::write(dir.join("it.vtt"), "WEBVTT").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let (media, subtitles) = scan_tracks(&dir, true).await;
        assert_eq!(media, [dir.join("large.ts"), dir.join("small.ts")]);
        assert_eq!(subtitles, [dir.join("it.vtt")]);
        assert_eq!(scan_tracks(&dir, false).await.1, Vec::<PathBuf>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reset_dir_leaves_an_empty_dir() {
        let dir = scratch_dir("reset_dir");
        let tracks = dir.join("tracks");
        reset_dir(&tracks).await.unwrap();
        std::fs::write(tracks.join("Ep.0.ts"), "stale").unwrap();
        reset_dir(&tracks).await.unwrap();
        assert_eq!(std::fs::read_dir(&tracks).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_tracks_on_missing_dir_is_empty() {
        let (media, subtitles) = scan_tracks(Path::new("/nonexistent/sv"), true).await;
        assert!(media.is_empty() && subtitles.is_empty());
    }

    #[tokio::test]
    async fn move_file_delivers_content() {
        let dir = scratch_dir("move_file");
        let (from, to) = (dir.join("work/a.mp4"), dir.join("out/a.mp4"));
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, "video").unwrap();
        move_file(&from, &to).await.unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "video");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn copy_atomically_never_exposes_a_partial_file() {
        let dir = scratch_dir("copy_atomically");
        let (from, to) = (dir.join("work.mp4"), dir.join("out/final.mp4"));
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, "video").unwrap();
        copy_atomically(&from, &to).await.unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "video");
        assert!(!to.with_extension("part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn mux_without_media_fails() {
        let err = mux(Path::new("ffmpeg"), Path::new("/tmp/out.mp4"), &[])
            .await
            .unwrap_err();
        assert_eq!(err, "No .ts files found after download");
    }
}
