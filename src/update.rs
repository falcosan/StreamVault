use crate::app::AppState;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use std::fmt::Display;
use std::future::Future;

#[derive(Clone, Copy, Default)]
pub struct Update {
    pub available: Signal<bool>,
    pub installing: Signal<bool>,
}

impl Update {
    pub async fn check<E: Display>(mut self, latest: impl Future<Output = Result<String, E>>) {
        match latest.await {
            Ok(version) if is_newer(&version, env!("CARGO_PKG_VERSION")) => {
                self.available.set(true);
            }
            Ok(_) => {}
            Err(e) => warn!("update check failed: {e}"),
        }
    }

    pub fn install<E: Display>(
        mut self,
        app: AppState,
        install: impl Future<Output = Result<(), E>> + 'static,
    ) {
        if *self.installing.peek() {
            return;
        }
        self.installing.set(true);
        spawn_forever(async move {
            let result = install.await;
            self.installing.set(false);
            if let Err(e) = result {
                app.report("Update failed", e);
            }
        });
    }
}

fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| {
        v.split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parse(remote), parse(local)) {
        (Some(remote), Some(local)) => remote > local,
        _ => remote != local,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("1.0.31", "1.0.30"));
        assert!(is_newer("1.0.100", "1.0.99"));
        assert!(!is_newer("1.0.30", "1.0.30"));
        assert!(!is_newer("1.0.29", "1.0.30"));
        assert!(is_newer("1.1.0-beta", "1.0.30"));
    }
}
