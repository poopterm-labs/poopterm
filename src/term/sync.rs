//! Synchronized output (`CSI ? 2026 h/l`): buffer updates in a pending snapshot.
use std::time::{Duration, Instant};

use super::Term;

impl Term {
	pub(crate) fn begin_sync(&mut self) {
		if self.synchronized.is_none() {
			let mut pending = self.clone();
			pending.responses.clear();
			self.synchronized = Some(Box::new(pending));
			self.sync_deadline = Some(Instant::now() + Duration::from_millis(250));
		}
	}

	pub(crate) fn finish_sync(&mut self) -> bool {
		if let Some(pending) = self.synchronized.take() {
			let responses = std::mem::take(&mut self.responses);
			*self = *pending;
			self.responses.extend(responses);
			self.sync_deadline = None;
			true
		} else {
			false
		}
	}

	pub(crate) fn finish_sync_if_expired(&mut self, now: Instant) -> bool {
		self.sync_deadline.is_some_and(|deadline| now >= deadline) && self.finish_sync()
	}

	#[inline(always)]
	pub(crate) fn pending(&mut self) -> &mut Self {
		match self.synchronized {
			Some(ref mut pending) => pending,
			None => self,
		}
	}
}
