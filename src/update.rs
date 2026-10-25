use crate::app::AppState;
use crate::time::sleep;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Default)]
pub struct Update {
    pub available: Signal<bool>,
    pub installing: Signal<bool>,
}

impl Update {
    pub async fn watch<E, F>(mut self, latest: impl Fn() -> F)
    where
        E: Display,
        F: Future<Output = Result<String, E>>,
    {
        newer(latest).await;
        self.available.set(true);
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

async fn newer<E, F>(latest: impl Fn() -> F)
where
    E: Display,
    F: Future<Output = Result<String, E>>,
{
    loop {
        match latest().await {
            Ok(version) if is_newer(&version, env!("CARGO_PKG_VERSION")) => return,
            Ok(_) => {}
            Err(e) => warn!("update check failed: {e}"),
        }
        sleep(CHECK_INTERVAL).await;
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
    use std::cell::RefCell;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn the_check_repeats_every_interval_until_a_newer_version_is_out() {
        let answers = RefCell::new(vec![
            Ok("999.0.0".to_owned()),
            Err("offline"),
            Ok(env!("CARGO_PKG_VERSION").to_owned()),
        ]);
        let started = Instant::now();
        newer(|| {
            let answer = answers.borrow_mut().pop().unwrap();
            async move { answer }
        })
        .await;
        assert_eq!(started.elapsed(), CHECK_INTERVAL * 2);
        assert!(answers.borrow().is_empty());
    }

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("1.0.31", "1.0.30"));
        assert!(is_newer("1.0.100", "1.0.99"));
        assert!(!is_newer("1.0.30", "1.0.30"));
        assert!(!is_newer("1.0.29", "1.0.30"));
        assert!(is_newer("1.1.0-beta", "1.0.30"));
    }
}
