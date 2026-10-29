use crate::app::AppState;
use crate::i18n::{Text, text};
use crate::providers::{MediaEntry, stable_hash};
use dioxus::prelude::*;

const POSTER_COLORS: [&str; 16] = [
    "#8b1a1a", "#0d3b66", "#3b0a5c", "#144014", "#6b3a00", "#5c0e0e", "#0a2a4a", "#400a50",
    "#4a0e0e", "#003a3a", "#3e2123", "#1b2a41", "#2d1b00", "#1a0a2e", "#0e331a", "#331a0e",
];

pub(super) fn poster_color(name: &str) -> &'static str {
    let index = stable_hash(name) % POSTER_COLORS.len() as u64;
    POSTER_COLORS[usize::try_from(index).unwrap_or_default()]
}

pub fn card_background(name: &str, image: Option<&str>) -> String {
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

pub(super) fn format_score(t: &Text, score: &str) -> String {
    score.parse::<f64>().map_or_else(
        |_| score.to_owned(),
        |v| format!("{v:.1}").replace('.', t.decimal),
    )
}

pub(super) fn badge(t: &Text, entry: &MediaEntry) -> (&'static str, &'static str) {
    if entry.is_movie() {
        ("badge-movie", t.movie)
    } else {
        ("badge-series", t.series)
    }
}

#[component]
pub(super) fn PosterCard(entry: MediaEntry) -> Element {
    let app = use_context::<AppState>();
    let t = text();
    let background = card_background(&entry.name, entry.image_url.as_deref());
    let (tone, label) = badge(t, &entry);
    let score = entry.score.as_deref().map(|score| format_score(t, score));
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
            span { class: "poster-overlay",
                span { class: "poster-title", "{entry.name}" }
                span { class: "poster-meta",
                    span { class: "badge {tone}", "{label}" }
                    if let Some(year) = &entry.year {
                        span { class: "poster-year", "{year}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    #[test]
    fn poster_color_is_deterministic() {
        assert_eq!(poster_color("Lost"), "#6b3a00");
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
        assert_eq!(format_score(Lang::En.text(), "7.456"), "7.5");
        assert_eq!(format_score(Lang::It.text(), "7.456"), "7,5");
        assert_eq!(format_score(Lang::En.text(), "N/A"), "N/A");
    }
}
