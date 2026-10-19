//! Shared CSI semantics over parameter storage: the parser's grouped
//! [`crate::vt::Params`], vte's (for the reference tests) and flat lists.
pub(crate) trait ParamList {
	fn iter(&self) -> impl Iterator<Item = &[u16]>;

	/// Group `index`.
	fn get(&self, index: usize) -> Option<&[u16]> {
		self.iter().nth(index)
	}
}

impl ParamList for crate::vt::Params {
	fn iter(&self) -> impl Iterator<Item = &[u16]> {
		self.iter()
	}

	#[inline]
	fn get(&self, index: usize) -> Option<&[u16]> {
		self.get(index)
	}
}

#[cfg(test)]
impl ParamList for vte::Params {
	fn iter(&self) -> impl Iterator<Item = &[u16]> {
		self.iter()
	}
}

impl ParamList for [u16] {
	fn iter(&self) -> impl Iterator<Item = &[u16]> {
		self.iter().map(std::slice::from_ref)
	}
}
