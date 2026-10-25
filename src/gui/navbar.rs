use super::{LOGO, UPDATE_ICON};
use crate::app::{AppState, Screen};
use crate::platform::{NAV, focus, install_update};
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
            UpdateButton {}
        }
    }
}

#[component]
fn UpdateButton() -> Element {
    let app = use_context::<AppState>();
    let update = app.device.update;
    let installing = *update.installing.read();
    if !installing && !*update.available.read() {
        return VNode::empty();
    }
    rsx! {
        button {
            class: if installing { "update-btn updating" } else { "update-btn" },
            aria_label: if installing { "Updating" } else { "Install update" },
            disabled: installing,
            onclick: move |_| install_update(app),
            span { class: "update-icon", dangerous_inner_html: UPDATE_ICON }
            if !installing {
                span { class: "update-dot" }
            }
        }
    }
}
