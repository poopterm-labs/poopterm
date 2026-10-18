//! Bytes read from a PTY, waiting for the parser thread.
//!
//! The reader thread keeps reading while the parser thread parses, so the
//! kernel's PTY buffer keeps draining. When parsing falls behind by `limit`
//! bytes the reader waits, which pushes back on the program producing
//! output instead of buffering without bound.
use std::sync::{Condvar, Mutex, MutexGuard};

pub(crate) struct OutputQueue {
	state: Mutex<State>,
	space: Condvar,
	data: Condvar,
	limit: usize,
}

#[derive(Default)]
struct State {
	bytes: Vec<u8>,
	closed: bool,
}

impl OutputQueue {
	pub(crate) fn new(limit: usize) -> Self {
		Self {
			state: Mutex::default(),
			space: Condvar::new(),
			data: Condvar::new(),
			limit,
		}
	}

	/// Bytes the queue holds before the reader waits.
	pub(crate) fn limit(&self) -> usize {
		self.limit
	}

	fn lock(&self) -> MutexGuard<'_, State> {
		self
			.state
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Appends `data`, waiting while the queue is full.
	pub(crate) fn push(&self, data: &[u8]) {
		let mut state = self.lock();
		while state.bytes.len() >= self.limit && !state.closed {
			state = self
				.space
				.wait(state)
				.unwrap_or_else(|poisoned| poisoned.into_inner());
		}
		state.bytes.extend_from_slice(data);
		self.data.notify_one();
	}

	/// Waits for queued bytes and moves them into `out`. Returns false once
	/// the queue is closed and fully drained.
	pub(crate) fn wait_take(&self, out: &mut Vec<u8>) -> bool {
		out.clear();
		let mut state = self.lock();
		while state.bytes.is_empty() && !state.closed {
			state = self
				.data
				.wait(state)
				.unwrap_or_else(|poisoned| poisoned.into_inner());
		}
		if state.bytes.is_empty() {
			return false;
		}
		// Swapping keeps both buffers' capacity.
		std::mem::swap(out, &mut state.bytes);
		self.space.notify_all();
		true
	}

	/// Ends the stream: wakes a reader blocked in [`push`](Self::push) (e.g.
	/// when the tab closes) and lets the parser finish once drained.
	pub(crate) fn close(&self) {
		self.lock().closed = true;
		self.space.notify_all();
		self.data.notify_all();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{sync::Arc, thread, time::Duration};

	#[test]
	fn a_full_queue_blocks_the_reader_until_the_parser_takes_bytes() {
		let queue = Arc::new(OutputQueue::new(4));
		queue.push(b"1234");
		let reader = {
			let queue = queue.clone();
			thread::spawn(move || queue.push(b"5"))
		};
		thread::sleep(Duration::from_millis(50));
		assert!(
			!reader.is_finished(),
			"push must wait while the queue is full"
		);
		let mut out = Vec::new();
		assert!(queue.wait_take(&mut out));
		reader.join().unwrap();
		assert!(queue.wait_take(&mut out));
		assert_eq!(out, b"5");
	}

	#[test]
	fn closing_releases_a_blocked_reader() {
		let queue = Arc::new(OutputQueue::new(1));
		queue.push(b"x");
		let reader = {
			let queue = queue.clone();
			thread::spawn(move || queue.push(b"y"))
		};
		thread::sleep(Duration::from_millis(20));
		queue.close();
		reader.join().unwrap();
	}

	#[test]
	fn the_parser_waits_for_data_and_drains_after_close() {
		let queue = Arc::new(OutputQueue::new(1024));
		let parser = {
			let queue = queue.clone();
			thread::spawn(move || {
				let mut out = Vec::new();
				let mut seen = Vec::new();
				while queue.wait_take(&mut out) {
					seen.extend_from_slice(&out);
				}
				seen
			})
		};
		thread::sleep(Duration::from_millis(20));
		queue.push(b"ab");
		queue.push(b"cd");
		queue.close();
		assert_eq!(parser.join().unwrap(), b"abcd");
	}
}
