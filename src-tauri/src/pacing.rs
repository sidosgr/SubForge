//! Shared account pacing; network requests never hold the state lock.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

pub struct Rate {
    next: Instant,
    blocked_until: Instant,
    in_flight: usize,
    penalty_ms: u64,
    strikes: u32,
    successes: u32,
}
impl Default for Rate {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            next: now,
            blocked_until: now,
            in_flight: 0,
            penalty_ms: 0,
            strikes: 0,
            successes: 0,
        }
    }
}
pub enum Slot {
    Ready(Permit),
    Spacing(Duration),
    Busy,
    Limited(Duration),
}
pub struct Permit {
    gate: Arc<Mutex<Rate>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut rate = self.gate.lock().unwrap();
        rate.in_flight = rate.in_flight.saturating_sub(1);
    }
}
impl Rate {
    pub fn acquire(gate: &Arc<Mutex<Self>>, gap_ms: u64, parallel: usize) -> Slot {
        let mut rate = gate.lock().unwrap();
        let now = Instant::now();
        if rate.blocked_until > now {
            return Slot::Limited(rate.blocked_until - now);
        }
        if rate.in_flight >= parallel.max(1) {
            return Slot::Busy;
        }
        if rate.next > now {
            return Slot::Spacing(rate.next - now);
        }
        let gap_ms = gap_ms.max(rate.penalty_ms);
        rate.in_flight += 1;
        rate.next = now + Duration::from_millis(gap_ms);
        Slot::Ready(Permit {
            gate: gate.clone(),
        })
    }
    pub fn limited(&mut self, retry_seconds: u64, base_ms: u64) -> (u64, u64) {
        self.strikes = (self.strikes + 1).min(7);
        self.successes = 0;
        let seconds = retry_seconds
            .max((1u64 << (self.strikes - 1)).min(30))
            .min(86400);
        self.blocked_until = self
            .blocked_until
            .max(Instant::now() + Duration::from_secs(seconds));
        self.penalty_ms = base_ms.max((250u64 << (self.strikes - 1)).min(2000));
        (seconds, self.penalty_ms)
    }
    pub fn succeeded(&mut self, base_ms: u64) {
        if self.blocked_until > Instant::now() {
            return;
        }
        self.successes += 1;
        if self.successes >= 5 {
            self.successes = 0;
            self.strikes = self.strikes.saturating_sub(1);
            self.penalty_ms = (self.penalty_ms / 2).max(base_ms);
            if self.strikes == 0 { self.penalty_ms = base_ms; }
        }
    }
    pub fn cooldown(&self) -> Duration {
        self.blocked_until.saturating_duration_since(Instant::now())
    }
    pub fn interval_ms(&self, base_ms: u64) -> u64 {
        base_ms.max(self.penalty_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_slots_are_bounded_and_cancellation_releases_them() {
        let gate = Arc::new(Mutex::new(Rate::default()));
        let Slot::Ready(first) = Rate::acquire(&gate, 0, 2) else {
            panic!("first slot");
        };
        let Slot::Ready(second) = Rate::acquire(&gate, 0, 2) else {
            panic!("second slot");
        };
        assert!(matches!(Rate::acquire(&gate, 0, 2), Slot::Busy));
        drop(first);
        assert!(matches!(Rate::acquire(&gate, 0, 2), Slot::Ready(_)));
        drop(second);
        assert_eq!(gate.lock().unwrap().in_flight, 0);
    }
    #[test]
    fn default_sequential_slot_cannot_be_bypassed_by_another_file() {
        let gate = Arc::new(Mutex::new(Rate::default()));
        let Slot::Ready(permit) = Rate::acquire(&gate, 0, 1) else {
            panic!("slot");
        };
        assert!(matches!(Rate::acquire(&gate, 0, 1), Slot::Busy));
        drop(permit);
        assert!(matches!(Rate::acquire(&gate, 0, 1), Slot::Ready(_)));
    }
    #[test]
    fn explicit_spacing_applies_between_starts() {
        let gate = Arc::new(Mutex::new(Rate::default()));
        let Slot::Ready(permit) = Rate::acquire(&gate, 15_000, 2) else {
            panic!("slot");
        };
        assert!(matches!(Rate::acquire(&gate, 15_000, 2), Slot::Spacing(_)));
        drop(permit);
        assert!(matches!(Rate::acquire(&gate, 15_000, 2), Slot::Spacing(_)));
    }
    #[test]
    fn repeated_limits_increase_backoff_and_honor_server_delay() {
        let mut rate = Rate::default();
        assert_eq!(rate.limited(0, 0), (1, 250));
        assert_eq!(rate.limited(180, 0), (180, 500));
        assert_eq!(rate.limited(0, 0), (4, 1000));
        assert!(rate.cooldown() >= Duration::from_secs(179));
        rate.succeeded(0);
        assert_eq!(rate.successes, 0); // An older in-flight success cannot erase a new limit.
    }
    #[test]
    fn successful_requests_ease_a_learned_pause_gradually() {
        let mut rate = Rate::default();
        rate.limited(0, 0);
        rate.blocked_until = Instant::now();
        for _ in 0..4 {
            rate.succeeded(0);
        }
        assert_eq!(rate.interval_ms(0), 250);
        rate.succeeded(0);
        assert_eq!(rate.interval_ms(0), 0);
    }
}
