use std::sync::{Once, OnceLock};
use std::time::{Duration, Instant};

static PROCESS_STARTED: OnceLock<Instant> = OnceLock::new();

pub fn start_clock() {
    PROCESS_STARTED.get_or_init(Instant::now);
}

pub fn timed<T>(phase: &'static str, run: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = run();
    tracing::info!(phase, elapsed_ms = millis(started.elapsed()), since_start_ms = since_start_ms(), "startup phase");
    value
}

pub struct Milestone {
    name: &'static str,
    reached: Once,
}

impl Milestone {
    pub const fn new(name: &'static str) -> Self {
        Self { name, reached: Once::new() }
    }

    pub fn reach(&self) {
        self.reached.call_once(|| {
            tracing::info!(milestone = self.name, since_start_ms = since_start_ms(), "startup milestone");
        });
    }

    pub fn is_reached(&self) -> bool {
        self.reached.is_completed()
    }
}

fn since_start_ms() -> f64 {
    millis(PROCESS_STARTED.get_or_init(Instant::now).elapsed())
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestone_is_reached_once() {
        let milestone = Milestone::new("test");
        assert!(!milestone.is_reached());
        milestone.reach();
        milestone.reach();
        assert!(milestone.is_reached());
    }

    #[test]
    fn timed_returns_the_value() {
        assert_eq!(timed("test", || 7), 7);
    }
}
