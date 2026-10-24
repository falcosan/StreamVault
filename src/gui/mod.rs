mod details;
mod home;
mod navbar;
mod player;
mod poster;
mod search_view;

pub use details::DetailsView;
pub use home::HomeView;
pub use navbar::Navbar;
pub use player::PlayerView;
pub use search_view::SearchView;

use crate::app::AppState;
use dioxus::prelude::*;

pub const LOGO: &str = include_str!("../../assets/img/vector/logo.svg");
pub const CLOSE_ICON: &str = include_str!("../../assets/img/vector/close.svg");
pub const PLAY_ICON: &str = include_str!("../../assets/img/vector/play.svg");
const PAUSE_ICON: &str = include_str!("../../assets/img/vector/pause.svg");
const BACK_ICON: &str = include_str!("../../assets/img/vector/back.svg");

#[component]
pub fn BackButton() -> Element {
    let app = use_context::<AppState>();
    rsx! {
        button {
            class: "back-button",
            aria_label: "Back",
            onclick: move |_| {
                app.back();
            },
            span { class: "back-icon", dangerous_inner_html: BACK_ICON }
        }
    }
}

pub fn focus(selector: &str) {
    document::eval(&format!(
        "document.querySelector({selector:?})?.focus({{ preventScroll: true }});"
    ));
}

pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_pluralizes() {
        assert_eq!(count(1, "Season"), "1 Season");
        assert_eq!(count(25, "Season"), "25 Seasons");
    }
}
