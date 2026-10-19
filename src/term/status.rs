//! What programs say they are doing: OSC 7501 program status records and
//! OSC 9;4 progress, kept for the tab strip.

/// Most records per terminal; the least recently updated goes first.
pub(crate) const MAX_RECORDS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
	Idle,
	Working,
	Done,
	Blocked,
	Error,
}

/// What a blocked program waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
	Permission,
	Question,
	Auth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Record {
	/// Segments joined by `/`; empty for the root record.
	pub id: String,
	pub state: State,
	pub kind: Option<Kind>,
	/// Percent done; `None` is indeterminate.
	pub progress: Option<u8>,
	pub app: Option<String>,
	pub title: Option<String>,
	pub msg: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProgressState {
	Normal,
	Error,
	Indeterminate,
	Paused,
}

/// An OSC 9;4 progress report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Progress {
	pub state: ProgressState,
	pub percent: Option<u8>,
}

#[derive(Clone, Default)]
pub(crate) struct Status {
	/// Least recently updated first.
	records: Vec<Record>,
	pub progress: Option<Progress>,
	/// Changes whenever something shown may have changed.
	pub generation: u64,
}

impl Status {
	#[cfg(test)]
	/// Records, least recently updated first.
	pub(crate) fn records(&self) -> impl Iterator<Item = &Record> {
		self.records.iter()
	}

	#[cfg(test)]
	pub(crate) fn get(&self, id: &str) -> Option<&Record> {
		self.records.iter().find(|record| record.id == id)
	}

	#[cfg(test)]
	/// The app of record `id`, or of its nearest ancestor that has one.
	pub(crate) fn app(&self, id: &str) -> Option<&str> {
		let mut id = id;
		loop {
			if let Some(app) = self.get(id).and_then(|record| record.app.as_deref()) {
				return Some(app);
			}
			if id.is_empty() {
				return None;
			}
			id = id.rfind('/').map_or("", |slash| &id[..slash]);
		}
	}

	/// Stores `record`, replacing the one with its id.
	pub(crate) fn set(&mut self, record: Record) {
		self.records.retain(|old| old.id != record.id);
		if self.records.len() >= MAX_RECORDS {
			self.records.remove(0);
		}
		self.records.push(record);
		self.changed();
	}

	/// Removes record `id` and every record beneath it; the root id removes
	/// them all.
	pub(crate) fn clear(&mut self, id: &str) {
		self.records.retain(|record| {
			!(id.is_empty()
				|| record.id == id
				|| record
					.id
					.strip_prefix(id)
					.is_some_and(|rest| rest.starts_with('/')))
		});
		self.changed();
	}

	/// A shell prompt began: running programs are gone.
	pub(crate) fn prompt(&mut self) {
		let before = self.records.len();
		self
			.records
			.retain(|record| !matches!(record.state, State::Working | State::Blocked));
		if self.records.len() != before {
			self.changed();
		}
	}

	pub(crate) fn set_progress(&mut self, progress: Option<Progress>) {
		if self.progress != progress {
			self.progress = progress;
			self.changed();
		}
	}

	/// The state the tab shows: what most needs the user.
	pub(crate) fn summary(&self) -> Option<State> {
		let rank = |state: State| match state {
			State::Blocked => 4,
			State::Error => 3,
			State::Done => 2,
			State::Working => 1,
			State::Idle => 0,
		};
		self
			.records
			.iter()
			.map(|record| record.state)
			.max_by_key(|&state| rank(state))
	}

	/// The progress the tab shows: OSC 9;4's, else the most recently
	/// updated record's, paused while that record is blocked.
	pub(crate) fn shown_progress(&self) -> Option<Progress> {
		self.progress.or_else(|| {
			let record = self
				.records
				.iter()
				.rev()
				.find(|record| record.progress.is_some())?;
			let state = match record.state {
				State::Blocked => ProgressState::Paused,
				_ => ProgressState::Normal,
			};
			Some(Progress {
				state,
				percent: record.progress,
			})
		})
	}

	/// The user looked at the terminal: finished work has been seen.
	pub(crate) fn acknowledge(&mut self) {
		let before = self.records.len();
		self
			.records
			.retain(|record| !matches!(record.state, State::Done | State::Error));
		if self.records.len() != before {
			self.changed();
		}
	}

	fn changed(&mut self) {
		self.generation = self.generation.wrapping_add(1);
	}
}
