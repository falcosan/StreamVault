use super::{LOGO, focus};
use crate::app::{AppState, Screen};
use crate::platform::{NAV, NavTools};
use dioxus::prelude::*;

const SEARCH_ICON: &str = include_str!("../../assets/img/vector/search.svg");

#[component]
pub fn Navbar() -> Element {
    let app = use_context::<AppState>();
    let mut query = app.search.query;
    let screen = *app.screen.read();
    rsx! {
        nav { class: "navbar",
            button {
                class: "logo",
                aria_label: "Home",
                onclick: move |_| app.navigate(Screen::Home),
                span { class: "logo-icon", dangerous_inner_html: LOGO }
            }
            for &(target, label) in NAV {
                button {
                    key: "{label}",
                    class: if screen == target { "nav-link active" } else { "nav-link" },
                    aria_current: if screen == target { "page" },
                    onclick: move |_| app.navigate(target),
                    "{label}"
                }
            }
            div { class: "search-bar",
                input {
                    id: "search-input",
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
                    id: "search-go",
                    class: "search-go",
                    aria_label: "Submit search",
                    disabled: *app.search.busy.read(),
                    onclick: move |_| {
                        if query.peek().trim().is_empty() {
                            focus("#search-input");
                        } else {
                            app.search();
                        }
                    },
                    span { class: "search-icon", dangerous_inner_html: SEARCH_ICON }
                }
            }
            NavTools {}
        }
    }
}
