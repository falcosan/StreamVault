use crate::app::AppState;
use crate::providers::{MediaEntry, stable_hash};
use dioxus::prelude::*;

const POSTER_COLORS: [(u8, u8, u8); 16] = [
    (0x8B, 0x1A, 0x1A),
    (0x0D, 0x3B, 0x66),
    (0x3B, 0x0A, 0x5C),
    (0x14, 0x40, 0x14),
    (0x6B, 0x3A, 0x00),
    (0x5C, 0x0E, 0x0E),
    (0x0A, 0x2A, 0x4A),
    (0x40, 0x0A, 0x50),
    (0x4A, 0x0E, 0x0E),
    (0x00, 0x3A, 0x3A),
    (0x3E, 0x21, 0x23),
    (0x1B, 0x2A, 0x41),
    (0x2D, 0x1B, 0x00),
    (0x1A, 0x0A, 0x2E),
    (0x0E, 0x33, 0x1A),
    (0x33, 0x1A, 0x0E),
];

pub(super) fn poster_color(name: &str) -> String {
    let index = stable_hash(name) % POSTER_COLORS.len() as u64;
    let (r, g, b) = POSTER_COLORS[usize::try_from(index).unwrap_or_default()];
    format!("rgb({r},{g},{b})")
}

pub(super) fn card_background(name: &str, image: Option<&str>) -> String {
    let color = poster_color(name);
    let Some(url) = image else {
        return format!("background-color: {color};");
    };
    let mut escaped = String::with_capacity(url.len());
    for c in url.chars().filter(|c| !c.is_control()) {
        if matches!(c, '"' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    format!("background-color: {color}; background-image: url(\"{escaped}\");")
}

pub(super) fn format_score(score: &str) -> String {
    score
        .parse::<f64>()
        .map_or_else(|_| score.to_owned(), |v| format!("{v:.1}"))
}

pub(super) fn badge(entry: &MediaEntry) -> (&'static str, &'static str) {
    if entry.is_movie() {
        ("badge-movie", "MOVIE")
    } else {
        ("badge-series", "SERIES")
    }
}

#[component]
pub(super) fn PosterCard(entry: MediaEntry) -> Element {
    let app = use_context::<AppState>();
    let background = card_background(&entry.name, entry.image_url.as_deref());
    let (tone, label) = badge(&entry);
    let year = entry.year_display().to_owned();
    let score = entry.score.as_deref().map(format_score);
    let target = entry.clone();
    rsx! {
        button {
            class: "poster-card",
            style: "{background}",
            "data-focus": "entry-{entry.provider}-{entry.id}",
            onclick: move |_| app.open(target.clone()),
            span { class: "poster-provider", "{entry.provider_name}" }
            if let Some(score) = score {
                span { class: "poster-score", "★ {score}" }
            }
            div { class: "poster-overlay",
                div { class: "poster-title", "{entry.name}" }
                div { class: "poster-meta",
                    span { class: "badge {tone}", "{label}" }
                    span { class: "poster-year", "{year}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_color_is_deterministic() {
        assert_eq!(poster_color("Lost"), poster_color("Lost"));
        assert!(poster_color("Lost").starts_with("rgb("));
    }

    #[test]
    fn card_background_escapes_the_image_url() {
        assert_eq!(
            card_background("Lost", Some("https://i/a\"b\\c\n.jpg")),
            format!(
                "background-color: {}; background-image: url(\"https://i/a\\\"b\\\\c.jpg\");",
                poster_color("Lost")
            )
        );
        assert_eq!(
            card_background("Lost", None),
            format!("background-color: {};", poster_color("Lost"))
        );
    }

    #[test]
    fn format_score_rounds_numbers_and_keeps_text() {
        assert_eq!(format_score("7.456"), "7.5");
        assert_eq!(format_score("N/A"), "N/A");
    }
}
