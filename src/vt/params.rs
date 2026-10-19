//! CSI and DCS parameters: up to [`MAX`] values, grouped by `;`, with `:`
//! subparameters (`38:2::1:2:3`) kept in the same group.

/// Values per sequence, subparameters included. More set the ignore flag.
pub(crate) const MAX: usize = 32;

#[derive(Clone)]
pub(crate) struct Params {
	values: [u16; MAX],
	/// Where each closed group ends in `values`.
	ends: [u8; MAX],
	groups: usize,
	len: usize,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			values: [0; MAX],
			ends: [0; MAX],
			groups: 0,
			len: 0,
		}
	}
}

impl Params {
	#[inline]
	pub(crate) fn is_full(&self) -> bool {
		self.len == MAX
	}

	#[inline]
	pub(crate) fn clear(&mut self) {
		self.len = 0;
		self.groups = 0;
	}

	/// Adds a value to the open group (`:`) without closing it.
	#[inline]
	pub(crate) fn push_sub(&mut self, value: u16) {
		self.values[self.len] = value;
		self.len += 1;
	}

	/// Adds a value and closes its group (`;` or the final byte).
	#[inline]
	pub(crate) fn push(&mut self, value: u16) {
		self.push_sub(value);
		self.ends[self.groups] = self.len as u8;
		self.groups += 1;
	}

	/// Closes the open group, if any: at dispatch when storage filled up
	/// before the final byte.
	#[inline]
	pub(crate) fn close(&mut self) {
		let closed = self
			.groups
			.checked_sub(1)
			.map_or(0, |last| usize::from(self.ends[last]));
		if self.len > closed {
			self.ends[self.groups] = self.len as u8;
			self.groups += 1;
		}
	}

	/// Group `index`: a parameter followed by its subparameters.
	#[inline]
	pub(crate) fn get(&self, index: usize) -> Option<&[u16]> {
		let end = usize::from(*self.ends[..self.groups].get(index)?);
		let start = index
			.checked_sub(1)
			.map_or(0, |previous| usize::from(self.ends[previous]));
		Some(&self.values[start..end])
	}

	/// Each group: a parameter followed by its subparameters.
	#[inline]
	pub(crate) fn iter(&self) -> impl Iterator<Item = &[u16]> + '_ {
		let mut start = 0;
		self.ends[..self.groups].iter().map(move |&end| {
			let group = &self.values[start..usize::from(end)];
			start = usize::from(end);
			group
		})
	}
}

impl std::fmt::Debug for Params {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_list().entries(self.iter()).finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn groups_keep_their_subparameters() {
		let mut params = Params::default();
		params.push(1);
		params.push_sub(38);
		params.push_sub(2);
		params.push(7);
		params.push(0);
		let groups: Vec<&[u16]> = params.iter().collect();
		assert_eq!(groups, [&[1][..], &[38, 2, 7], &[0]]);
		assert_eq!(params.get(1), Some(&[38, 2, 7][..]));
		assert_eq!(params.get(3), None);
		params.clear();
		assert_eq!(params.iter().count(), 0);
	}

	#[test]
	fn closing_reports_a_group_left_open_by_full_storage() {
		let mut params = Params::default();
		params.push(5);
		while !params.is_full() {
			params.push_sub(1);
		}
		params.close();
		params.close();
		assert_eq!(params.iter().count(), 2);
		assert_eq!(params.get(1).map(<[u16]>::len), Some(MAX - 1));
	}
}
