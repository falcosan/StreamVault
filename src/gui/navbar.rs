use super::{Choice, LOGO, Picker, UPDATE_ICON};
use crate::app::{AppState, Screen};
use crate::i18n::{Lang, text};
use crate::platform::{focus, install_update, nav};
use dioxus::prelude::*;

const SEARCH_ICON: &str = include_str!("../../assets/img/vector/search.svg");

#[component]
pub fn Navbar() -> Element {
    let app = use_context::<AppState>();
    let mut query = app.search.query;
    let screen = *app.screen.read();
    let t = text();
    rsx! {
        nav { class: "navbar",
            button {
                class: "logo",
                aria_label: t.home,
                onclick: move |_| app.navigate(Screen::Home),
                span { class: "logo-icon", dangerous_inner_html: LOGO }
            }
            for (target, label) in nav() {
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
                    placeholder: t.search_placeholder,
                    aria_label: t.search,
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
                    aria_label: t.submit_search,
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
            LanguageMenu {}
            UpdateButton {}
        }
    }
}

#[component]
fn LanguageMenu() -> Element {
    let app = use_context::<AppState>();
    let mut menu = app.language_menu;
    let open = *menu.read();
    let t = text();
    let current = Lang::current();
    rsx! {
        div {
            class: "language",
            onkeydown: move |e: KeyboardEvent| {
                if open && e.key() == Key::Escape {
                    app.close_language_menu();
                }
            },
            button {
                id: "language",
                class: "language-btn",
                title: t.language,
                aria_expanded: open,
                onclick: move |_| menu.toggle(),
                "{current.code()}"
            }
            if open {
                div { class: "menu-backdrop", onclick: move |_| app.close_language_menu() }
                Picker {
                    class: "language-menu",
                    title: t.language,
                    options: Lang::ALL
                        .map(|lang| Choice {
                            value: lang,
                            name: lang.text().name_of(lang).to_owned(),
                            lang: Some(lang.code()),
                        })
                        .to_vec(),
                    selected: current,
                    onchoose: move |lang| app.choose_language(lang),
                }
            }
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
            aria_label: if installing { text().updating } else { text().install_update },
            disabled: installing,
            onclick: move |_| install_update(app),
            span { class: "update-icon", dangerous_inner_html: UPDATE_ICON }
            if !installing {
                span { class: "update-dot" }
            }
        }
    }
}
