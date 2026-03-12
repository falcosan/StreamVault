use crate::providers::MediaEntry;

const POSTER_COLORS: &[(u8, u8, u8)] = &[
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
    let (r, g, b) =
        POSTER_COLORS[crate::providers::provider_hash(name) as usize % POSTER_COLORS.len()];
    format!("rgb({r},{g},{b})")
}

pub(super) fn format_score(s: &str) -> String {
    match s.parse::<f64>() {
        Ok(v) => format!("{v:.1}"),
        Err(_) => s.to_string(),
    }
}

use dioxus::prelude::*;

#[component]
pub(super) fn PosterCard(entry: MediaEntry, on_select: EventHandler<MediaEntry>) -> Element {
    let bg = poster_color(&entry.name);
    let style = match &entry.image_url {
        Some(url) => format!("background-color: {bg}; background-image: url('{url}');"),
        None => format!("background-color: {bg};"),
    };
    let is_movie = entry.is_movie();
    let badge_class = if is_movie {
        "badge badge-movie"
    } else {
        "badge badge-series"
    };
    let yr = entry.year_display().to_string();
    let kind_label = if is_movie { "MOVIE" } else { "SERIES" };
    let prov = entry.provider_name.clone();

    rsx! {
        button {
            class: "poster-card",
            style: "{style}",
            onclick: move |_| on_select.call(entry.clone()),
            span { class: "poster-provider", "{prov}" }
            if let Some(ref sc) = entry.score {
                { let s = format_score(sc); rsx! { span { class: "poster-score", "★ {s}" } } }
            }
            div { class: "poster-overlay",
                div { class: "poster-title", "{entry.name}" }
                div { class: "poster-meta",
                    span { class: "{badge_class}", "{kind_label}" }
                    span { class: "poster-year", "{yr}" }
                }
            }
        }
    }
}
