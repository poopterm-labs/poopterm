//! Grid coordinates and text selections.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CellPoint {
	pub(crate) row: usize,
	pub(crate) col: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Selection {
	pub(crate) start: CellPoint,
	pub(crate) end: CellPoint,
}

impl Selection {
	pub(crate) fn ordered(self) -> (CellPoint, CellPoint) {
		if self.start <= self.end {
			(self.start, self.end)
		} else {
			(self.end, self.start)
		}
	}
}
