//! A token bucket limiting how many notifications a session may show.
use std::time::{Duration, Instant};

/// Notifications shown at once before the limit applies.
const BURST: f32 = 5.0;
/// Time to earn one more notification.
const REFILL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub(crate) struct RateLimit {
	tokens: f32,
	updated: Instant,
}

impl RateLimit {
	pub(crate) fn new(now: Instant) -> Self {
		Self {
			tokens: BURST,
			updated: now,
		}
	}

	/// Whether a notification may be shown at `now`, using up a token.
	pub(crate) fn allow(&mut self, now: Instant) -> bool {
		let earned = now.saturating_duration_since(self.updated).as_secs_f32() / REFILL.as_secs_f32();
		self.tokens = (self.tokens + earned).min(BURST);
		self.updated = now;
		if self.tokens < 1.0 {
			return false;
		}
		self.tokens -= 1.0;
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bursts_are_allowed_then_one_per_refill() {
		let start = Instant::now();
		let mut limit = RateLimit::new(start);
		let allowed = (0..10).filter(|_| limit.allow(start)).count();
		assert_eq!(allowed, BURST as usize);
		assert!(!limit.allow(start + REFILL / 2));
		assert!(limit.allow(start + REFILL));
		assert!(!limit.allow(start + REFILL));
		// Idle time refills up to the burst, not beyond.
		let later = start + REFILL * 100;
		assert_eq!(
			(0..10).filter(|_| limit.allow(later)).count(),
			BURST as usize
		);
	}
}
