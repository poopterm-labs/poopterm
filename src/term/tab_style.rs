//! How a program dresses its tab: iTerm2's tab color (OSC 6;1) and session
//! status (OSC 21337).

/// Longest status text, in characters.
pub(crate) const MAX_STATUS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabStyle {
	/// The tab's background.
	pub color: Option<u32>,
	/// A dot before the title.
	pub indicator: Option<u32>,
	/// Text after the title.
	pub status: Option<String>,
	pub status_color: Option<u32>,
}
