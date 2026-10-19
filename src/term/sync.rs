//! Synchronized output (`CSI ? 2026 h/l`): updates apply at once, but frames
//! show the screen as it was when the block began, until it ends.
use std::time::{Duration, Instant};

use super::{Frame, Term};

impl Term {
	/// Freezes what frames show: one copy of the visible screen, so the cost
	/// does not grow with history.
	pub(crate) fn begin_sync(&mut self) {
		if self.synchronized.is_none() {
			let mut frozen = Box::default();
			self.snapshot(&mut frozen);
			self.synchronized = Some(frozen);
			self.sync_deadline = Some(Instant::now() + Duration::from_millis(250));
		}
	}

	pub(crate) fn finish_sync(&mut self) -> bool {
		self.sync_deadline = None;
		self.synchronized.take().is_some()
	}

	pub(crate) fn finish_sync_if_expired(&mut self, now: Instant) -> bool {
		self.sync_deadline.is_some_and(|deadline| now >= deadline) && self.finish_sync()
	}

	/// The screen frames show while a block is open.
	pub(crate) fn frozen_frame(&self) -> Option<&Frame> {
		self.synchronized.as_deref()
	}
}
