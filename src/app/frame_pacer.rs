//! Limits redraws to the display's frame rate while output is streaming.
//!
//! Without pacing, every chunk of PTY output triggered a full software
//! render, so a fast producer spent most of its time waiting for frames
//! nobody could see.
use std::time::{Duration, Instant};

pub(super) struct FramePacer {
	interval: Duration,
	last_frame: Option<Instant>,
	deadline: Option<Instant>,
}

impl FramePacer {
	pub(super) fn new(interval: Duration) -> Self {
		Self {
			interval,
			last_frame: None,
			deadline: None,
		}
	}

	pub(super) fn set_interval(&mut self, interval: Duration) {
		self.interval = interval;
	}

	/// Content changed at `now`. Returns true if a frame should be requested
	/// now; otherwise the change is folded into the pending deadline.
	pub(super) fn request(&mut self, now: Instant) -> bool {
		match self.last_frame {
			Some(last) if now < last + self.interval => {
				self.deadline.get_or_insert(last + self.interval);
				false
			}
			_ => true,
		}
	}

	/// When a deferred frame should be drawn, if one is pending.
	pub(super) fn deadline(&self) -> Option<Instant> {
		self.deadline
	}

	/// True once a deferred frame's deadline has passed.
	pub(super) fn due(&self, now: Instant) -> bool {
		self.deadline.is_some_and(|deadline| now >= deadline)
	}

	/// Records that a frame was drawn at `now`.
	pub(super) fn drawn(&mut self, now: Instant) {
		self.last_frame = Some(now);
		self.deadline = None;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::{Duration, Instant};

	const FRAME: Duration = Duration::from_millis(16);

	#[test]
	fn the_first_change_draws_immediately() {
		let mut pacer = FramePacer::new(FRAME);
		assert!(pacer.request(Instant::now()));
	}

	#[test]
	fn changes_within_a_frame_are_deferred_to_one_deadline() {
		let mut pacer = FramePacer::new(FRAME);
		let start = Instant::now();
		pacer.drawn(start);
		assert!(!pacer.request(start + Duration::from_millis(2)));
		assert!(!pacer.request(start + Duration::from_millis(5)));
		assert_eq!(pacer.deadline(), Some(start + FRAME));
		assert!(!pacer.due(start + Duration::from_millis(10)));
		assert!(pacer.due(start + FRAME));
	}

	#[test]
	fn drawing_clears_the_deadline() {
		let mut pacer = FramePacer::new(FRAME);
		let start = Instant::now();
		pacer.drawn(start);
		pacer.request(start + Duration::from_millis(1));
		pacer.drawn(start + FRAME);
		assert_eq!(pacer.deadline(), None);
		assert!(pacer.request(start + FRAME * 2));
	}
}
