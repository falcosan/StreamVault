use dioxus::desktop::wry::http::{HeaderValue, Request, Response, StatusCode, header};
use dioxus::logger::tracing::warn;
use std::io::SeekFrom;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;

pub const ROUTE: &str = "media";
const CHUNK: u64 = 4 * 1024 * 1024;

pub fn url(id: Uuid) -> String {
    format!("/{ROUTE}/{id}")
}

pub fn requested_range(request: &Request<Vec<u8>>) -> Option<String> {
    request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(String::from)
}

pub async fn respond(file: Option<&Path>, range: Option<&str>) -> Response<Vec<u8>> {
    let Some(file) = file else {
        return status(StatusCode::NOT_FOUND);
    };
    match read(file, range).await {
        Ok(response) => response,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => status(StatusCode::NOT_FOUND),
        Err(e) => {
            warn!("media read error {}: {e}", file.display());
            status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn read(file: &Path, range: Option<&str>) -> std::io::Result<Response<Vec<u8>>> {
    let mut source = tokio::fs::File::open(file).await?;
    let size = source.metadata().await?.len();
    let Some((start, end)) = byte_range(range, size) else {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{size}"))
            .body(Vec::new())
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)));
    };
    source.seek(SeekFrom::Start(start)).await?;
    let mut body = Vec::new();
    source.take(end - start + 1).read_to_end(&mut body).await?;
    Ok(Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, content_type(file))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
        .header(header::CONTENT_LENGTH, HeaderValue::from(body.len()))
        .body(body)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)))
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}

fn byte_range(header: Option<&str>, size: u64) -> Option<(u64, u64)> {
    let last = size.checked_sub(1)?;
    let (start, end) = match header {
        None => (0, last),
        Some(value) => {
            let (first, second) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
            match (first.trim(), second.trim()) {
                ("", suffix) => (size.saturating_sub(suffix.parse().ok()?), last),
                (start, "") => (start.parse().ok()?, last),
                (start, end) => (start.parse().ok()?, end.parse::<u64>().ok()?.min(last)),
            }
        }
    };
    (start <= end).then(|| (start, end.min(start.saturating_add(CHUNK - 1))))
}

fn content_type(file: &Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("m4v") => "video/mp4",
        Some(e) if e.eq_ignore_ascii_case("mov") => "video/quicktime",
        Some(e) if e.eq_ignore_ascii_case("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::test_support::scratch_dir;

    fn request(path: &str, range: Option<&str>) -> Request<Vec<u8>> {
        let mut builder = Request::builder().uri(format!("dioxus://index.html{path}"));
        if let Some(range) = range {
            builder = builder.header(header::RANGE, range);
        }
        builder.body(Vec::new()).unwrap()
    }

    #[test]
    fn url_lives_under_the_route() {
        let id = Uuid::new_v4();
        assert_eq!(url(id), format!("/{ROUTE}/{id}"));
    }

    #[test]
    fn requested_range_reads_header() {
        assert_eq!(
            requested_range(&request("/media/x", Some("bytes=0-1"))).as_deref(),
            Some("bytes=0-1")
        );
        assert_eq!(requested_range(&request("/media/x", None)), None);
    }

    #[test]
    fn byte_range_parses_standard_forms() {
        assert_eq!(byte_range(Some("bytes=0-1"), 100), Some((0, 1)));
        assert_eq!(byte_range(Some("bytes=10-"), 100), Some((10, 99)));
        assert_eq!(byte_range(Some("bytes=-30"), 100), Some((70, 99)));
        assert_eq!(byte_range(Some("bytes=90-500"), 100), Some((90, 99)));
        assert_eq!(byte_range(None, 100), Some((0, 99)));
    }

    #[test]
    fn byte_range_caps_chunk_size() {
        let size = CHUNK * 3;
        assert_eq!(byte_range(Some("bytes=0-"), size), Some((0, CHUNK - 1)));
        assert_eq!(byte_range(Some("bytes=5-"), size), Some((5, CHUNK + 4)));
    }

    #[test]
    fn byte_range_rejects_unsatisfiable_or_malformed() {
        assert_eq!(byte_range(Some("bytes=100-"), 100), None);
        assert_eq!(byte_range(Some("bytes=5-2"), 100), None);
        assert_eq!(byte_range(Some("bytes=-0"), 100), None);
        assert_eq!(byte_range(Some("items=0-1"), 100), None);
        assert_eq!(byte_range(Some("bytes=a-b"), 100), None);
        assert_eq!(byte_range(None, 0), None);
    }

    #[test]
    fn content_type_follows_extension() {
        assert_eq!(content_type(Path::new("a/b.mp4")), "video/mp4");
        assert_eq!(content_type(Path::new("a/b.MKV")), "video/x-matroska");
        assert_eq!(content_type(Path::new("a/b")), "application/octet-stream");
    }

    #[tokio::test]
    async fn respond_serves_requested_bytes() {
        let dir = scratch_dir("media_respond");
        let file = dir.join("clip.mp4");
        std::fs::write(&file, b"0123456789").unwrap();
        let response = respond(Some(&file), Some("bytes=2-5")).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), b"2345");
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn respond_rejects_out_of_range_and_missing_files() {
        let dir = scratch_dir("media_reject");
        let file = dir.join("clip.mp4");
        std::fs::write(&file, b"0123").unwrap();
        let response = respond(Some(&file), Some("bytes=9-")).await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */4");
        let missing = respond(Some(&dir.join("gone.mp4")), None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(respond(None, None).await.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
