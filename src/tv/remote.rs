use std::cell::RefCell;
use std::cmp::Ordering;
use wasm_bindgen::prelude::*;
use web_sys::{
    Document, Element, FocusOptions, HtmlElement, KeyboardEvent, MutationObserver,
    MutationObserverInit, ScrollBehavior, ScrollIntoViewOptions, ScrollLogicalPosition,
    ScrollToOptions,
};

const MEDIA_KEYS: [&str; 6] = [
    "MediaPlayPause",
    "MediaPlay",
    "MediaPause",
    "MediaStop",
    "MediaRewind",
    "MediaFastForward",
];
const FOCUSABLE: &str = "button:not([disabled]), input[type='range']:not([disabled])";
const RESTORABLE: &str = "[data-focus]";
const EDITABLE: &str = "input:not([type='range'])";
const CONTENT: &str = ".content";

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["tizen", "tvinputdevice"], js_name = registerKey, catch)]
    fn register_key(name: &str) -> Result<(), JsValue>;

    type TizenApplication;

    #[wasm_bindgen(js_namespace = ["tizen", "application"], js_name = getCurrentApplication, catch)]
    fn current_application() -> Result<TizenApplication, JsValue>;

    #[wasm_bindgen(method, catch)]
    fn exit(this: &TizenApplication) -> Result<(), JsValue>;
}

thread_local! {
    static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Move(Direction),
    Select,
    Back,
    PlayPause,
    Play,
    Pause,
    Stop,
    Rewind,
    FastForward,
    Done,
    Cancel,
}

fn key(code: u32, name: &str) -> Option<Key> {
    Some(match (code, name) {
        (38, _) | (_, "ArrowUp") => Key::Move(Direction::Up),
        (40, _) | (_, "ArrowDown") => Key::Move(Direction::Down),
        (37, _) | (_, "ArrowLeft") => Key::Move(Direction::Left),
        (39, _) | (_, "ArrowRight") => Key::Move(Direction::Right),
        (13, _) | (_, "Enter") => Key::Select,
        (10009, _) | (_, "XF86Back" | "GoBack" | "Escape") => Key::Back,
        (10252, _) | (_, "MediaPlayPause") => Key::PlayPause,
        (415, _) | (_, "MediaPlay") => Key::Play,
        (19, _) | (_, "MediaPause") => Key::Pause,
        (413, _) | (_, "MediaStop") => Key::Stop,
        (412, _) | (_, "MediaRewind") => Key::Rewind,
        (417, _) | (_, "MediaFastForward") => Key::FastForward,
        (65376, _) => Key::Done,
        (65385, _) => Key::Cancel,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Rect {
    fn center(&self) -> (f64, f64) {
        (
            (self.left + self.right) / 2.0,
            (self.top + self.bottom) / 2.0,
        )
    }

    fn intersects(&self, other: &Rect) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rank {
    misaligned: bool,
    distance: f64,
    offset: f64,
}

impl Rank {
    fn compare(&self, other: &Rank) -> Ordering {
        self.misaligned
            .cmp(&other.misaligned)
            .then(self.distance.total_cmp(&other.distance))
            .then(self.offset.total_cmp(&other.offset))
    }
}

fn nearest(
    direction: Direction,
    from: Rect,
    rects: impl IntoIterator<Item = Rect>,
) -> Option<usize> {
    rects
        .into_iter()
        .enumerate()
        .filter_map(|(i, to)| Some((rank(direction, from, to)?, i)))
        .min_by(|(a, _), (b, _)| a.compare(b))
        .map(|(_, i)| i)
}

fn rank(direction: Direction, from: Rect, to: Rect) -> Option<Rank> {
    let ((fx, fy), (tx, ty)) = (from.center(), to.center());
    let span_gap = |a0: f64, a1: f64, b0: f64, b1: f64| (b0 - a1).max(a0 - b1).max(0.0);
    let horizontal = span_gap(from.left, from.right, to.left, to.right);
    let vertical = span_gap(from.top, from.bottom, to.top, to.bottom);
    let (along, offset, gap, cross) = match direction {
        Direction::Up => (fy - ty, tx - fx, from.top - to.bottom, horizontal),
        Direction::Down => (ty - fy, tx - fx, to.top - from.bottom, horizontal),
        Direction::Left => (fx - tx, ty - fy, from.left - to.right, vertical),
        Direction::Right => (tx - fx, ty - fy, to.left - from.right, vertical),
    };
    let sideways = matches!(direction, Direction::Left | Direction::Right);
    if along <= 1.0 || (sideways && cross > 0.0) {
        return None;
    }
    let primary = if from.intersects(&to) {
        along
    } else {
        gap.max(0.0)
    };
    Some(Rank {
        misaligned: cross > 0.0,
        distance: primary * 2.0 + cross,
        offset: offset.abs(),
    })
}

pub fn install(mut handle: impl FnMut(Key) -> bool + 'static) {
    for name in MEDIA_KEYS {
        let _ = register_key(name);
    }
    let Some(document) = document() else {
        return;
    };
    let listener = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        let Some(key) = key(event.key_code(), &event.key()) else {
            return;
        };
        PENDING.set(None);
        if handle(key) {
            event.prevent_default();
            event.stop_propagation();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback_and_bool(
            "keydown",
            listener.as_ref().unchecked_ref(),
            true,
        );
    }
    listener.forget();

    let settle = Closure::<dyn FnMut()>::new(settle);
    if let (Ok(observer), Some(body)) = (
        MutationObserver::new(settle.as_ref().unchecked_ref()),
        document.body(),
    ) {
        let options = MutationObserverInit::new();
        options.set_child_list(true);
        options.set_subtree(true);
        let _ = observer.observe_with_options(&body, &options);
    }
    settle.forget();
}

pub fn navigate(direction: Direction) -> bool {
    navigate_in(direction, ":root")
}

pub fn navigate_in(direction: Direction, scope: &str) -> bool {
    let Some(document) = document() else {
        return false;
    };
    let Some(root) = document.query_selector(scope).ok().flatten() else {
        return false;
    };
    let candidates = focusables(&root);
    let current = document.active_element().and_then(|active| {
        candidates
            .iter()
            .position(|(element, _)| *element == active)
    });
    let target = match current {
        Some(index) => nearest(
            direction,
            candidates[index].1,
            candidates.iter().map(|(_, rect)| *rect),
        )
        .map(|i| candidates[i].0.clone()),
        None => root
            .query_selector(RESTORABLE)
            .ok()
            .flatten()
            .or_else(|| candidates.first().map(|(element, _)| element.clone())),
    };
    target.inspect(reveal).is_some()
}

pub fn activate() -> bool {
    focused().inspect(HtmlElement::click).is_some()
}

pub fn focus(selector: &str) -> bool {
    document()
        .and_then(|document| document.query_selector(selector).ok().flatten())
        .inspect(reveal)
        .is_some()
}

pub fn blur() {
    if let Some(element) = focused() {
        let _ = element.blur();
    }
}

pub fn is_focused(selector: &str) -> bool {
    focused().is_some_and(|element| element.matches(selector).unwrap_or(false))
}

pub fn editing() -> bool {
    focused().is_some_and(|element| element.matches(EDITABLE).unwrap_or(false))
}

pub fn focus_key() -> Option<String> {
    focused()?.get_attribute("data-focus")
}

pub fn restore(key: Option<String>) {
    PENDING.set(key);
}

pub fn exit() -> bool {
    current_application()
        .and_then(|application| application.exit())
        .is_ok()
}

fn settle() {
    let Some(document) = document() else {
        return;
    };
    if let Some(key) = PENDING.with_borrow(Clone::clone) {
        if let Ok(Some(target)) = document.query_selector(&format!("[data-focus=\"{key}\"]")) {
            PENDING.set(None);
            reveal(&target);
        }
        return;
    }
    if focused().is_none()
        && let Ok(Some(target)) = document.query_selector(RESTORABLE)
    {
        reveal(&target);
    }
}

fn document() -> Option<Document> {
    web_sys::window()?.document()
}

fn focused() -> Option<HtmlElement> {
    let document = document()?;
    let active = document.active_element()?;
    if document.body().is_some_and(|body| *body == active) {
        return None;
    }
    active.dyn_into().ok()
}

fn focusables(root: &Element) -> Vec<(Element, Rect)> {
    let Ok(nodes) = root.query_selector_all(FOCUSABLE) else {
        return Vec::new();
    };
    (0..nodes.length())
        .filter_map(|i| nodes.item(i)?.dyn_into::<Element>().ok())
        .filter_map(|element| {
            let bounds = element.get_bounding_client_rect();
            let rect = Rect {
                left: bounds.left(),
                top: bounds.top(),
                right: bounds.right(),
                bottom: bounds.bottom(),
            };
            (bounds.width() > 0.0 && bounds.height() > 0.0).then_some((element, rect))
        })
        .collect()
}

fn reveal(target: &Element) {
    let Some(element) = target.dyn_ref::<HtmlElement>() else {
        return;
    };
    let focus = FocusOptions::new();
    focus.set_prevent_scroll(true);
    let _ = element.focus_with_options(&focus);
    let content_at_top = document()
        .and_then(|document| document.query_selector(CONTENT).ok().flatten())
        .filter(|content| shown_from_top(content, target));
    if let Some(content) = &content_at_top {
        let options = ScrollToOptions::new();
        options.set_top(0.0);
        options.set_behavior(ScrollBehavior::Instant);
        content.scroll_to_with_scroll_to_options(&options);
    }
    let scroll = ScrollIntoViewOptions::new();
    scroll.set_behavior(ScrollBehavior::Instant);
    scroll.set_block(if content_at_top.is_some() {
        ScrollLogicalPosition::Nearest
    } else {
        ScrollLogicalPosition::Center
    });
    scroll.set_inline(ScrollLogicalPosition::Nearest);
    element.scroll_into_view_with_scroll_into_view_options(&scroll);
}

fn shown_from_top(content: &Element, target: &Element) -> bool {
    if !content.contains(Some(target)) {
        return true;
    }
    let bottom = target.get_bounding_client_rect().bottom()
        - content.get_bounding_client_rect().top()
        + f64::from(content.scroll_top());
    bottom <= f64::from(content.client_height())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn remote_codes_map_to_keys_even_without_a_key_name() {
        assert_eq!(key(13, ""), Some(Key::Select));
        assert_eq!(key(10009, ""), Some(Key::Back));
        assert_eq!(key(40, ""), Some(Key::Move(Direction::Down)));
        assert_eq!(key(10252, ""), Some(Key::PlayPause));
        assert_eq!(key(65376, ""), Some(Key::Done));
        assert_eq!(key(0, "ArrowLeft"), Some(Key::Move(Direction::Left)));
        assert_eq!(key(0, "Escape"), Some(Key::Back));
        assert_eq!(key(65, "a"), None);
    }

    #[test]
    fn elements_behind_the_direction_are_ignored() {
        let from = rect(100.0, 100.0, 200.0, 200.0);
        assert!(rank(Direction::Down, from, rect(100.0, 0.0, 200.0, 90.0)).is_none());
        assert!(rank(Direction::Right, from, rect(0.0, 100.0, 90.0, 200.0)).is_none());
        assert!(rank(Direction::Up, from, from).is_none());
    }

    #[test]
    fn grid_moves_prefer_the_aligned_neighbour() {
        let from = rect(0.0, 0.0, 100.0, 150.0);
        let below = rect(0.0, 170.0, 100.0, 320.0);
        let diagonal = rect(120.0, 170.0, 220.0, 320.0);
        let far_below = rect(0.0, 340.0, 100.0, 490.0);
        assert_eq!(
            nearest(Direction::Down, from, [diagonal, far_below, below]),
            Some(2)
        );
        let right = rect(120.0, 0.0, 220.0, 150.0);
        assert_eq!(nearest(Direction::Right, from, [diagonal, right]), Some(1));
    }

    #[test]
    fn misaligned_moves_prefer_the_nearest_row() {
        let poster = rect(1100.0, 322.0, 1335.0, 672.0);
        let continue_card = rect(342.0, 140.0, 592.0, 281.0);
        let search = rect(1644.0, 10.0, 1900.0, 46.0);
        assert_eq!(
            nearest(Direction::Up, poster, [search, continue_card]),
            Some(1)
        );
    }

    #[test]
    fn sideways_moves_stay_in_the_row() {
        let last_poster = rect(769.0, 72.0, 940.0, 329.0);
        let search = rect(905.0, 12.0, 937.0, 44.0);
        let next_row = rect(20.0, 345.0, 191.0, 602.0);
        assert_eq!(
            nearest(Direction::Right, last_poster, [search, next_row]),
            None
        );
        assert_eq!(nearest(Direction::Up, last_poster, [search]), Some(0));
    }

    #[test]
    fn nested_buttons_are_reached_vertically_but_skipped_horizontally() {
        let card = rect(68.0, 140.0, 318.0, 281.0);
        let remove = rect(288.0, 146.0, 312.0, 170.0);
        let next_card = rect(342.0, 140.0, 592.0, 281.0);
        let nav_link = rect(74.0, 10.0, 109.0, 46.0);
        assert_eq!(
            nearest(Direction::Right, card, [remove, next_card]),
            Some(1)
        );
        assert_eq!(nearest(Direction::Up, card, [nav_link, remove]), Some(1));
        assert_eq!(nearest(Direction::Down, remove, [card, next_card]), Some(0));
    }
}
