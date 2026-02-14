use std::cmp;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Debug)]
struct LimiterState {
    delay: Duration,
    min_delay: Duration,
    max_delay: Duration,
    last_request_at: Option<Instant>,
}

#[derive(Debug)]
pub struct RateLimiter {
    state: Mutex<LimiterState>,
}

impl RateLimiter {
    pub fn new(requests_per_second: u32) -> Self {
        let min_delay = if requests_per_second == 0 {
            Duration::from_millis(500)
        } else {
            Duration::from_secs_f64(1.0 / requests_per_second as f64)
        };

        Self {
            state: Mutex::new(LimiterState {
                delay: min_delay,
                min_delay,
                max_delay: Duration::from_secs(5),
                last_request_at: None,
            }),
        }
    }

    pub async fn acquire(&self) {
        let wait_duration = {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let wait = match state.last_request_at {
                Some(last) => {
                    let target = last + state.delay;
                    if target > now {
                        target.duration_since(now)
                    } else {
                        Duration::from_millis(0)
                    }
                }
                None => Duration::from_millis(0),
            };
            state.last_request_at = Some(now + wait);
            wait
        };

        if !wait_duration.is_zero() {
            sleep(wait_duration).await;
        }
    }

    pub async fn on_success(&self) {
        let mut state = self.state.lock().await;
        let reduced_ms = (state.delay.as_millis() as f64 * 0.95).round() as u64;
        let reduced = Duration::from_millis(cmp::max(1, reduced_ms));
        state.delay = cmp::max(state.min_delay, reduced);
    }

    pub async fn on_error(&self) {
        let mut state = self.state.lock().await;
        let increased_ms = (state.delay.as_millis() as f64 * 1.25).round() as u64;
        let increased = Duration::from_millis(cmp::max(1, increased_ms));
        state.delay = cmp::min(state.max_delay, increased);
    }
}
