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
use crate::i18n::text;
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
            aria_label: text().back,
            onclick: move |_| {
                app.back();
            },
            span { class: "back-icon", dangerous_inner_html: BACK_ICON }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub name: String,
    pub lang: Option<&'static str>,
}

#[component]
pub fn Picker<T: Copy + PartialEq + 'static>(
    class: &'static str,
    title: &'static str,
    options: Vec<Choice<T>>,
    selected: T,
    onchoose: EventHandler<T>,
) -> Element {
    rsx! {
        div { class: "menu {class}", role: "group", aria_label: title,
            span { class: "menu-title", "{title}" }
            for (position, Choice { value, name, lang }) in options.into_iter().enumerate() {
                button {
                    key: "{position}",
                    class: if value == selected { "menu-option active" } else { "menu-option" },
                    lang,
                    aria_pressed: value == selected,
                    onmounted: move |e| async move {
                        if value == selected {
                            let _ = e.set_focus(true).await;
                        }
                    },
                    onclick: move |_| onchoose.call(value),
                    "{name}"
                }
            }
        }
    }
}
