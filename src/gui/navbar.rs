use crate::style::{LOGO_SVG, UPDATE_SVG};
use dioxus::prelude::*;

use super::Screen;

#[component]
pub fn Navbar(
    screen: Signal<Screen>,
    history: Signal<Vec<Screen>>,
    search_query: Signal<String>,
    has_update: ReadSignal<bool>,
    is_updating: ReadSignal<bool>,
    is_searching: ReadSignal<bool>,
    on_update: EventHandler<()>,
    on_search_submit: EventHandler<String>,
) -> Element {
    let current = screen();
    let update = has_update();
    let updating = is_updating();
    let searching = is_searching();
    rsx! {
        nav { class: "navbar",
            button { class: "logo", onclick: move |_| {
                    if screen() != Screen::Home { history.write().push(screen()); screen.set(Screen::Home); }
                },
                div { class: "logo-icon", dangerous_inner_html: LOGO_SVG }
            }
            div { class: "nav-spacer" }
            button {
                class: if current == Screen::Home { "nav-link active" } else { "nav-link" },
                onclick: move |_| {
                    if screen() != Screen::Home { history.write().push(screen()); screen.set(Screen::Home); }
                },
                "Home"
            }
            button {
                class: if current == Screen::Downloads { "nav-link active" } else { "nav-link" },
                onclick: move |_| {
                    if screen() != Screen::Downloads { history.write().push(screen()); screen.set(Screen::Downloads); }
                },
                "Downloads"
            }
            div { class: "nav-fill" }
            div { class: "search-bar",
                input {
                    class: "search-input",
                    placeholder: "Search...",
                    value: "{search_query}",
                    oninput: move |e| search_query.set(e.value()),
                    onkeypress: {
                        let q = search_query;
                        move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                on_search_submit.call(q());
                            }
                        }
                    },
                }
                button {
                    class: "search-go",
                    disabled: searching,
                    onclick: {
                        let q = search_query;
                        move |_| on_search_submit.call(q())
                    },
                    div { class: "search-icon", dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>"# }
                }
            }
            if update || updating {
                button {
                    class: if updating { "update-btn updating" } else { "update-btn" },
                    onclick: move |_| { if !updating { on_update.call(()); } },
                    div { class: "update-icon", dangerous_inner_html: UPDATE_SVG }
                    if !updating { span { class: "update-dot" } }
                }
            }
        }
    }
}
