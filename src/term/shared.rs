//! A terminal shared between its parser thread and the UI thread.
//!
//! The parser thread holds the lock while applying a bounded chunk of
//! output; the UI thread holds it to read state or copy the visible rows
//! for a frame. `std::sync::Mutex` deadlocks if one thread locks twice, so
//! debug builds track the locks each thread holds and panic instead.
use std::{
	ops::{Deref, DerefMut},
	sync::{Mutex, MutexGuard},
};

use super::Term;

pub(crate) struct SharedTerm {
	term: Mutex<Term>,
}

impl SharedTerm {
	pub(crate) fn new(term: Term) -> Self {
		Self {
			term: Mutex::new(term),
		}
	}

	/// Locks the terminal. Never call this while already holding the lock
	/// on the same thread; pass `&Term` to helpers instead.
	pub(crate) fn lock(&self) -> TermGuard<'_> {
		#[cfg(debug_assertions)]
		held::acquire(self);
		TermGuard {
			guard: self
				.term
				.lock()
				.unwrap_or_else(|poisoned| poisoned.into_inner()),
			#[cfg(debug_assertions)]
			owner: self,
		}
	}
}

pub(crate) struct TermGuard<'a> {
	guard: MutexGuard<'a, Term>,
	#[cfg(debug_assertions)]
	owner: &'a SharedTerm,
}

impl Deref for TermGuard<'_> {
	type Target = Term;

	fn deref(&self) -> &Term {
		&self.guard
	}
}

impl DerefMut for TermGuard<'_> {
	fn deref_mut(&mut self) -> &mut Term {
		&mut self.guard
	}
}

#[cfg(debug_assertions)]
impl Drop for TermGuard<'_> {
	fn drop(&mut self) {
		held::release(self.owner);
	}
}

/// Debug-only record of the terminals each thread has locked.
#[cfg(debug_assertions)]
mod held {
	use std::cell::RefCell;

	thread_local! {
		static HELD: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
	}

	pub(super) fn acquire(term: &super::SharedTerm) {
		let address = term as *const _ as usize;
		HELD.with_borrow_mut(|held| {
			assert!(
				!held.contains(&address),
				"terminal already locked by this thread (would deadlock)"
			);
			held.push(address);
		});
	}

	pub(super) fn release(term: &super::SharedTerm) {
		let address = term as *const _ as usize;
		HELD.with_borrow_mut(|held| held.retain(|&a| a != address));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::theme::{Theme, ThemeKind};
	use std::{sync::Arc, thread};

	fn shared() -> Arc<SharedTerm> {
		Arc::new(SharedTerm::new(Term::new(
			4,
			2,
			Theme::from_kind(ThemeKind::Poop),
			"/".into(),
		)))
	}

	#[test]
	fn other_threads_see_updates() {
		let term = shared();
		let writer = term.clone();
		thread::spawn(move || writer.lock().title = "from parser".into())
			.join()
			.unwrap();
		assert_eq!(term.lock().title, "from parser");
	}

	#[test]
	fn a_lock_can_be_taken_again_after_it_is_released() {
		let term = shared();
		drop(term.lock());
		drop(term.lock());
	}

	#[cfg(debug_assertions)]
	#[test]
	#[should_panic(expected = "already locked by this thread")]
	fn locking_twice_on_one_thread_panics_instead_of_deadlocking() {
		let term = shared();
		let _first = term.lock();
		let _second = term.lock();
	}
}
