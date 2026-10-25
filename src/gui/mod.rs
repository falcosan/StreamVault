mod details_view;
mod home_view;
mod navbar;
mod player_view;
pub mod poster;
mod search_view;

pub use details_view::DetailsView;
pub use home_view::HomeView;
pub use navbar::Navbar;
pub use player_view::PlayerView;
pub use search_view::SearchView;

use crate::app::AppState;
use dioxus::prelude::*;

const LOGO: &str = include_str!("../../assets/img/vector/logo.svg");
pub const CLOSE_ICON: &str = include_str!("../../assets/img/vector/close.svg");
pub const PLAY_ICON: &str = include_str!("../../assets/img/vector/play.svg");
pub const UPDATE_ICON: &str = include_str!("../../assets/img/vector/update.svg");
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
