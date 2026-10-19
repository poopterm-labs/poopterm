//! Pointer shapes programs ask for with OSC 22: the CSS names kitty
//! defines, xterm's cursor font names as aliases, and a stack per screen.

/// Shapes kept per stack; the bottom one goes when it is full.
pub(crate) const MAX_STACK: usize = 16;

/// The shape names every terminal must support.
pub(crate) const NAMES: [&str; 30] = [
	"alias",
	"cell",
	"copy",
	"crosshair",
	"default",
	"e-resize",
	"ew-resize",
	"grab",
	"grabbing",
	"help",
	"move",
	"n-resize",
	"ne-resize",
	"nesw-resize",
	"no-drop",
	"not-allowed",
	"ns-resize",
	"nw-resize",
	"nwse-resize",
	"pointer",
	"progress",
	"s-resize",
	"se-resize",
	"sw-resize",
	"text",
	"vertical-text",
	"w-resize",
	"wait",
	"zoom-in",
	"zoom-out",
];

/// xterm's names, from the X11 cursor font, and the shapes they look like.
const ALIASES: [(&str, &str); 24] = [
	("left_ptr", "default"),
	("arrow", "default"),
	("xterm", "text"),
	("ibeam", "text"),
	("hand1", "pointer"),
	("hand2", "pointer"),
	("watch", "wait"),
	("clock", "wait"),
	("cross", "crosshair"),
	("tcross", "crosshair"),
	("crosshair", "crosshair"),
	("plus", "cell"),
	("fleur", "move"),
	("question_arrow", "help"),
	("pirate", "not-allowed"),
	("sb_h_double_arrow", "ew-resize"),
	("sb_v_double_arrow", "ns-resize"),
	("top_side", "n-resize"),
	("bottom_side", "s-resize"),
	("left_side", "w-resize"),
	("right_side", "e-resize"),
	("top_left_corner", "nw-resize"),
	("top_right_corner", "ne-resize"),
	("bottom_left_corner", "sw-resize"),
];

/// The shape `name` stands for, as an index into [`NAMES`].
pub(crate) fn shape(name: &[u8]) -> Option<u8> {
	let name = std::str::from_utf8(name).ok()?;
	let name = ALIASES
		.iter()
		.find(|(alias, _)| *alias == name)
		.map_or(name, |(_, shape)| shape);
	NAMES
		.iter()
		.position(|known| *known == name)
		.map(|index| index as u8)
}

/// The main and alternate screens' shape stacks.
#[derive(Clone, Default)]
pub(crate) struct PointerShapes {
	stacks: [Vec<u8>; 2],
}

impl PointerShapes {
	pub(crate) fn stack(&mut self, alternate: bool) -> &mut Vec<u8> {
		&mut self.stacks[usize::from(alternate)]
	}

	pub(crate) fn current(&self, alternate: bool) -> Option<&'static str> {
		let top = *self.stacks[usize::from(alternate)].last()?;
		Some(NAMES[usize::from(top)])
	}

	pub(crate) fn push(&mut self, alternate: bool, shape: u8) {
		let stack = self.stack(alternate);
		if stack.len() >= MAX_STACK {
			stack.remove(0);
		}
		stack.push(shape);
	}
}

impl super::Term {
	/// The shape the program wants on the screen in use, if any.
	pub(crate) fn pointer_shape(&self) -> Option<&'static str> {
		self.pointer_shapes.current(self.primary_screen.is_some())
	}
}
