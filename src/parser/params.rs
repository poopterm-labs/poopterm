//! Shared CSI semantics for vte's grouped parameters and the common flat path.
pub(crate) trait ParamList {
	fn iter(&self) -> impl Iterator<Item = &[u16]>;
}

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
