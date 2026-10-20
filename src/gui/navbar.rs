use super::LOGO;
use crate::app::{AppState, Screen};
use dioxus::prelude::*;

const SEARCH_ICON: &str = include_str!("../../assets/img/vector/search.svg");
const UPDATE_ICON: &str = include_str!("../../assets/img/vector/update.svg");

#[component]
pub fn Navbar() -> Element {
    let app = use_context::<AppState>();
    let mut query = app.search.query;
    let screen = *app.screen.read();
    let link = |target: Screen| {
        if screen == target {
            "nav-link active"
        } else {
            "nav-link"
        }
    };
    let updating = *app.updating.read();
    rsx! {
        nav { class: "navbar",
            button {
                class: "logo",
                aria_label: "Home",
                onclick: move |_| app.navigate(Screen::Home),
                div { class: "logo-icon", dangerous_inner_html: LOGO }
            }
            div { class: "nav-spacer" }
            button {
                class: link(Screen::Home),
                onclick: move |_| app.navigate(Screen::Home),
                "Home"
            }
            button {
                class: link(Screen::Downloads),
                onclick: move |_| app.navigate(Screen::Downloads),
                "Downloads"
            }
            div { class: "nav-fill" }
            div { class: "search-bar",
                input {
                    class: "search-input",
                    placeholder: "Search...",
                    aria_label: "Search",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            app.search();
                        }
                    },
                }
                button {
                    class: "search-go",
                    aria_label: "Search",
                    disabled: *app.search.busy.read(),
                    onclick: move |_| app.search(),
                    div { class: "search-icon", dangerous_inner_html: SEARCH_ICON }
                }
            }
            if updating || *app.update_available.read() {
                button {
                    class: if updating { "update-btn updating" } else { "update-btn" },
                    aria_label: if updating { "Updating" } else { "Install update" },
                    disabled: updating,
                    onclick: move |_| app.update(),
                    div { class: "update-icon", dangerous_inner_html: UPDATE_ICON }
                    if !updating {
                        span { class: "update-dot" }
                    }
                }
            }
        }
    }
}
