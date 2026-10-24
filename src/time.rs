#[cfg(target_arch = "wasm32")]
pub use gloo_timers::future::sleep;
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::sleep;
use web_time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    epoch_ms(SystemTime::now())
}

pub fn epoch_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }

    #[test]
    fn times_before_the_epoch_are_zero() {
        assert_eq!(epoch_ms(UNIX_EPOCH), 0);
        assert_eq!(epoch_ms(UNIX_EPOCH - std::time::Duration::from_secs(1)), 0);
    }
}
